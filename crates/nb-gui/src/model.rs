use std::collections::VecDeque;

pub const LOG_CAP: usize = 500;
pub const FP_SHOW: usize = 12;

#[derive(Clone, Debug)]
pub struct ChannelView {
    pub channel: String,
    pub host: String,
    pub http_port: u16,
    pub last_seen: u64,
}

#[derive(Clone, Debug)]
pub struct DeviceView {
    pub fingerprint: String,
    pub alias: String,
    pub trusted: bool,
    pub channels: Vec<ChannelView>,
    pub paired_via: Option<String>,
}

#[derive(Clone, Debug)]
pub struct SelfInfo {
    pub alias: String,
    pub fingerprint: String,
    pub http_port: u16,
    pub packet_port: u16,
}

#[derive(Clone, Debug)]
pub struct InboundPair {
    pub alias: String,
    pub fingerprint: String,
    pub pin: Option<String>,
    pub channel: String,
    pub accepted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpKind {
    Pair,
    Ping,
    Send,
    Clipboard,
}

impl OpKind {
    pub fn label(self) -> &'static str {
        match self {
            OpKind::Pair => "PAIR",
            OpKind::Ping => "PING",
            OpKind::Send => "SEND",
            OpKind::Clipboard => "CLIPBOARD",
        }
    }
}

#[derive(Clone, Debug)]
pub enum Flow {
    Idle,
    Working { target: String },
    Done { target: String, ok: bool, detail: String },
}

impl Flow {
    pub fn start(&mut self, target: &str) {
        *self = Flow::Working {
            target: target.to_string(),
        };
    }

    pub fn finish(&mut self, target: &str, ok: bool, detail: String) {
        *self = Flow::Done {
            target: target.to_string(),
            ok,
            detail,
        };
    }

    pub fn reset(&mut self) {
        *self = Flow::Idle;
    }

    pub fn is_working(&self) -> bool {
        matches!(self, Flow::Working { .. })
    }

    pub fn summary(&self) -> String {
        match self {
            Flow::Idle => String::new(),
            Flow::Working { target } => format!("working: {target}"),
            Flow::Done { target, ok, detail } => {
                let mark = if *ok { "OK" } else { "FAIL" };
                if detail.is_empty() {
                    format!("{mark}: {target}")
                } else {
                    format!("{mark}: {target} — {detail}")
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum UiEvent {
    Log(String),
    Peers(Vec<DeviceView>),
    OpResult {
        op: OpKind,
        target: String,
        ok: bool,
        detail: String,
    },
    InboundPair(InboundPair),
    DaemonStatus { running: bool },
    SelfInfo(SelfInfo),
    Discovering { active: bool },
}

#[derive(Clone, Debug)]
pub enum GuiCmd {
    DiscoverNow,
    Pair { fingerprint: String },
    Ping { fingerprint: String },
    SendFiles { fingerprint: String, paths: Vec<String> },
    PushClipboard { fingerprint: String, text: String },
    SetAutoPair(bool),
    StartDaemon,
    StopDaemon,
}

pub fn fp_short(fp: &str) -> String {
    fp.chars().take(FP_SHOW).collect()
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn age_string(now: u64, last_seen: u64) -> String {
    if last_seen == 0 {
        return "—".to_string();
    }
    let dt = now.saturating_sub(last_seen);
    if dt < 60 {
        format!("{dt}s")
    } else if dt < 3600 {
        format!("{}m", dt / 60)
    } else if dt < 86400 {
        format!("{}h", dt / 3600)
    } else {
        format!("{}d", dt / 86400)
    }
}

pub fn device_last_seen(d: &DeviceView) -> u64 {
    d.channels.iter().map(|c| c.last_seen).max().unwrap_or(0)
}

pub fn sort_devices(devices: &mut [DeviceView]) {
    devices.sort_by(|a, b| {
        b.trusted
            .cmp(&a.trusted)
            .then(device_last_seen(b).cmp(&device_last_seen(a)))
            .then(a.alias.to_lowercase().cmp(&b.alias.to_lowercase()))
    });
}

pub fn primary_channel(d: &DeviceView) -> String {
    let mut chs: Vec<&str> = d.channels.iter().map(|c| c.channel.as_str()).collect();
    chs.sort();
    chs.dedup();
    chs.join("+")
}

pub fn dial_label(d: &DeviceView) -> String {
    let mut addrs: Vec<String> = d
        .channels
        .iter()
        .map(|c| format!("{} {}:{}", c.channel, c.host, c.http_port))
        .collect();
    addrs.sort();
    addrs.join(", ")
}

pub fn find_device<'a>(devices: &'a [DeviceView], target: &str) -> Option<&'a DeviceView> {
    let t = target.trim();
    if t.is_empty() {
        return None;
    }
    if let Some(d) = devices.iter().find(|d| d.fingerprint == t) {
        return Some(d);
    }
    let lower = t.to_lowercase();
    if t.len() >= 6 && t.bytes().all(|b| b.is_ascii_hexdigit()) {
        let hits: Vec<&DeviceView> = devices
            .iter()
            .filter(|d| d.fingerprint.to_lowercase().starts_with(&lower))
            .collect();
        if hits.len() == 1 {
            return Some(hits[0]);
        }
        if hits.len() > 1 {
            return None;
        }
    }
    let hits: Vec<&DeviceView> = devices
        .iter()
        .filter(|d| d.alias.to_lowercase() == lower)
        .collect();
    if hits.len() == 1 {
        Some(hits[0])
    } else {
        None
    }
}

pub struct LogBuf {
    lines: VecDeque<String>,
    cap: usize,
}

impl LogBuf {
    pub fn new(cap: usize) -> Self {
        Self {
            lines: VecDeque::new(),
            cap,
        }
    }

    pub fn push(&mut self, line: String) {
        for part in line.split('\n') {
            if self.lines.len() >= self.cap {
                self.lines.pop_front();
            }
            self.lines.push_back(part.to_string());
        }
    }

    pub fn clear(&mut self) {
        self.lines.clear();
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &String> {
        self.lines.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(alias: &str, fp: &str, trusted: bool, seen: u64) -> DeviceView {
        DeviceView {
            fingerprint: fp.to_string(),
            alias: alias.to_string(),
            trusted,
            channels: vec![ChannelView {
                channel: "lan".to_string(),
                host: "192.168.1.5".to_string(),
                http_port: 53317,
                last_seen: seen,
            }],
            paired_via: None,
        }
    }

    #[test]
    fn trusted_first_then_recent_then_alias() {
        let mut v = vec![
            dev("zebra", "AA11", false, 300),
            dev("apple", "BB22", false, 100),
            dev("middle", "CC33", true, 50),
        ];
        sort_devices(&mut v);
        assert_eq!(v[0].alias, "middle");
        assert_eq!(v[1].alias, "zebra");
        assert_eq!(v[2].alias, "apple");
    }

    #[test]
    fn alias_tiebreak_is_case_insensitive() {
        let mut v = vec![dev("Beta", "AA11", false, 100), dev("alpha", "BB22", false, 100)];
        sort_devices(&mut v);
        assert_eq!(v[0].alias, "alpha");
        assert_eq!(v[1].alias, "Beta");
    }

    #[test]
    fn fp_short_truncates_to_prefix() {
        assert_eq!(fp_short("AABBCCDDEEFF00112233"), "AABBCCDDEEFF");
        assert_eq!(fp_short("ABC"), "ABC");
    }

    #[test]
    fn age_buckets() {
        assert_eq!(age_string(1000, 990), "10s");
        assert_eq!(age_string(1000, 880), "2m");
        assert_eq!(age_string(10000, 2800), "2h");
        assert_eq!(age_string(200000, 2000), "2d");
        assert_eq!(age_string(100, 900), "0s");
        assert_eq!(age_string(1000, 0), "—");
    }

    #[test]
    fn log_buf_caps_old_lines() {
        let mut b = LogBuf::new(3);
        for i in 0..5 {
            b.push(format!("line {i}"));
        }
        assert_eq!(b.len(), 3);
        let v: Vec<&String> = b.iter().collect();
        assert_eq!(v[0], "line 2");
        assert_eq!(v[2], "line 4");
        b.clear();
        assert!(b.is_empty());
    }

    #[test]
    fn log_buf_splits_newlines() {
        let mut b = LogBuf::new(10);
        b.push("a\nb".to_string());
        assert_eq!(b.len(), 2);
    }

    #[test]
    fn flow_transitions() {
        let mut f = Flow::Idle;
        assert!(!f.is_working());
        assert!(f.summary().is_empty());
        f.start("phone");
        assert!(f.is_working());
        assert!(f.summary().contains("phone"));
        f.finish("phone", true, "via lan".to_string());
        assert!(!f.is_working());
        assert!(f.summary().contains("OK"));
        f.finish("phone", false, String::new());
        assert!(f.summary().contains("FAIL"));
        f.reset();
        assert!(matches!(f, Flow::Idle));
    }

    #[test]
    fn find_by_exact_prefix_and_alias() {
        let v = vec![
            dev("phone", "AABBCCDDEEFF0011", false, 10),
            dev("laptop", "1122334455667788", true, 20),
        ];
        assert_eq!(find_device(&v, "AABBCCDDEEFF0011").unwrap().alias, "phone");
        assert_eq!(find_device(&v, "aabbcc").unwrap().alias, "phone");
        assert_eq!(find_device(&v, "LAPTOP").unwrap().alias, "laptop");
        assert!(find_device(&v, "nope").is_none());
        assert!(find_device(&v, "").is_none());
    }

    #[test]
    fn ambiguous_prefix_resolves_to_none() {
        let v = vec![
            dev("a", "AABB0011", false, 10),
            dev("b", "AABB0022", false, 10),
        ];
        assert!(find_device(&v, "aabb").is_none());
    }

    #[test]
    fn dial_label_lists_addresses() {
        let mut d = dev("phone", "AA", true, 10);
        d.channels.push(ChannelView {
            channel: "tailscale".to_string(),
            host: "100.64.0.9".to_string(),
            http_port: 53317,
            last_seen: 9,
        });
        let label = dial_label(&d);
        assert!(label.contains("lan 192.168.1.5:53317"));
        assert!(label.contains("tailscale 100.64.0.9:53317"));
    }

    #[test]
    fn op_labels_are_uppercase() {
        for op in [OpKind::Pair, OpKind::Ping, OpKind::Send, OpKind::Clipboard] {
            let l = op.label();
            assert_eq!(l, l.to_uppercase());
        }
    }
}
