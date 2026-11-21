use std::time::{Duration, Instant};

fn main() {
    let mut source = nb_audio::open_source();
    eprintln!("source: {}", source.source_name());
    let start = Instant::now();
    let mut frames = 0u32;
    let mut samples = 0u64;
    let mut peak = 0i16;
    while start.elapsed() < Duration::from_secs(6) {
        if let Some(pcm) = source.next_frame() {
            frames += 1;
            samples += pcm.len() as u64;
            for s in pcm {
                peak = peak.max(s.abs());
            }
        } else {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    eprintln!("frames={frames} samples={samples} peak={peak}");
}
