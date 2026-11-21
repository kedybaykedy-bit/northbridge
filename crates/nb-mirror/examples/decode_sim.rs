use nb_mirror::H264Decoder;
use std::collections::VecDeque;

fn split_nals(data: &[u8]) -> Vec<Vec<u8>> {
    let mut starts = Vec::new();
    let mut i = 0;
    while i + 2 < data.len() {
        if data[i] == 0 && data[i + 1] == 0 {
            if data[i + 2] == 1 {
                starts.push((i, 3));
                i += 3;
                continue;
            }
            if i + 3 < data.len() && data[i + 2] == 0 && data[i + 3] == 1 {
                starts.push((i, 4));
                i += 4;
                continue;
            }
        }
        i += 1;
    }
    let mut nals = Vec::new();
    for k in 0..starts.len() {
        let begin = starts[k].0 + starts[k].1;
        let end = starts.get(k + 1).map(|s| s.0).unwrap_or(data.len());
        if end > begin {
            nals.push(data[begin..end].to_vec());
        }
    }
    nals
}

fn is_first_slice_of_picture(nal: &[u8]) -> bool {
    nal.len() > 1 && (nal[1] & 0x80) != 0
}

fn group_like_app(nals: &[Vec<u8>]) -> Vec<Vec<u8>> {
    let mut aus: Vec<Vec<u8>> = Vec::new();
    let mut pending: Vec<u8> = Vec::new();
    let mut has_vcl = false;
    let mut flush = |pending: &mut Vec<u8>, has_vcl: &mut bool, aus: &mut Vec<Vec<u8>>| {
        if *has_vcl && !pending.is_empty() {
            aus.push(std::mem::take(pending));
        }
        pending.clear();
        *has_vcl = false;
    };
    for nal in nals {
        if nal.is_empty() {
            continue;
        }
        let t = nal[0] & 0x1F;
        if t == 1 || t == 5 {
            if has_vcl && is_first_slice_of_picture(nal) {
                flush(&mut pending, &mut has_vcl, &mut aus);
            }
            pending.extend_from_slice(&[0, 0, 0, 1]);
            pending.extend_from_slice(nal);
            has_vcl = true;
        } else {
            if (t == 6 || t == 7 || t == 8 || t == 9) && has_vcl {
                flush(&mut pending, &mut has_vcl, &mut aus);
            }
            pending.extend_from_slice(&[0, 0, 0, 1]);
            pending.extend_from_slice(nal);
        }
    }
    if has_vcl && !pending.is_empty() {
        aus.push(pending);
    }
    aus
}

fn run_sequential(aus: &[Vec<u8>], skip: Option<usize>) -> (u64, u64, String) {
    let mut dec = H264Decoder::new().expect("decoder");
    let mut decoded = 0;
    let mut errors = 0;
    let mut first_err = String::new();
    for (i, au) in aus.iter().enumerate() {
        if Some(i) == skip {
            continue;
        }
        let before_err = dec.errors;
        let r = dec.decode(au);
        if r.is_some() {
            decoded += 1;
        }
        if dec.errors > before_err {
            errors += 1;
            if first_err.is_empty() {
                first_err = format!("au#{i} len={} errors_so_far={}", au.len(), dec.errors);
            }
        }
    }
    (decoded, errors, first_err)
}

fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/nb-sim.264".into());
    let data = std::fs::read(&path)?;
    let nals = split_nals(&data);
    let mut type_hist = [0u32; 32];
    for n in &nals {
        type_hist[(n[0] & 0x1F) as usize] += 1;
    }
    println!("file={}B nals={} types(0..31)={:?}", data.len(), nals.len(), &type_hist[..12]);

    let aus = group_like_app(&nals);
    println!("grouped aus={} sizes={:?}", aus.len(), aus.iter().take(8).map(|a| a.len()).collect::<Vec<_>>());

    let (d, e, first) = run_sequential(&aus, None);
    println!("[A] sequential all      : decoded={d} err_aus={e} first={first}");

    let mut whole = H264Decoder::new()?;
    let mut d_whole = 0;
    let mut e_whole = 0;
    for au in &aus {
        let b = whole.errors;
        if whole.decode(au).is_some() {
            d_whole += 1;
        }
        if whole.errors > b {
            e_whole += 1;
        }
    }
    println!("[B] same but tallied    : decoded={d_whole} err_aus={e_whole}");

    let mut one = H264Decoder::new()?;
    let r_one = one.decode(&data);
    println!(
        "[C] whole file one call : some={} decoded={} errors={}",
        r_one.is_some(),
        one.decoded,
        one.errors
    );

    if aus.len() > 2 {
        let (d, e, first) = run_sequential(&aus, Some(2));
        println!("[D] skip AU#2 (drop)    : decoded={d} err_aus={e} first={first}");
    }

    let mut drop_recover = H264Decoder::new()?;
    let mut rec_decoded = 0;
    for (i, au) in aus.iter().enumerate() {
        if i == 2 {
            continue;
        }
        if drop_recover.decode(au).is_some() {
            rec_decoded += 1;
        }
    }
    let idr_au = aus
        .iter()
        .position(|a| a.windows(4).any(|w| w == [0, 0, 0, 1]) && a.contains(&0x65));
    println!(
        "[E] drop#2 + reset at next IDR : idr_au_index={idr_au:?} decoded_before_reset check via [F]"
    );

    let mut fresh = H264Decoder::new()?;
    let mut f_decoded = 0;
    let mut reset_used = false;
    for (i, au) in aus.iter().enumerate() {
        if i == 2 && !reset_used {
            fresh = H264Decoder::new()?;
            reset_used = true;
        }
        if fresh.decode(au).is_some() {
            f_decoded += 1;
        }
    }
    println!("[F] drop#2 + fresh decoder : decoded={f_decoded}");
    Ok(())
}
