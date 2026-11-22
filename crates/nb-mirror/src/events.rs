pub fn normalize_coord(px: f64, extent: f64) -> f64 {
    if extent <= 0.0 {
        return 0.0;
    }
    (px / extent).clamp(0.0, 1.0)
}

pub fn xkey_to_evdev(xkey: u16) -> u16 {
    xkey.saturating_sub(8)
}

const LEGACY_KEYSYMS: [(u32, u32); 79] = [
    (0x1a1,0x0104), (0x1a2,0x02d8), (0x1a3,0x0141), (0x1a5,0x013d), (0x1a6,0x015a), (0x1a9,0x0160),
    (0x1aa,0x015e), (0x1ab,0x0164), (0x1ac,0x0179), (0x1ae,0x017d), (0x1af,0x017b), (0x1b1,0x0105),
    (0x1b2,0x02db), (0x1b3,0x0142), (0x1b5,0x013e), (0x1b6,0x015b), (0x1b7,0x02c7), (0x1b9,0x0161),
    (0x1ba,0x015f), (0x1bb,0x0165), (0x1bc,0x017a), (0x1bd,0x02dd), (0x1be,0x017e), (0x1bf,0x017c),
    (0x1c0,0x0154), (0x1c3,0x0102), (0x1c5,0x0139), (0x1c6,0x0106), (0x1c8,0x010c), (0x1ca,0x0118),
    (0x1cc,0x011a), (0x1cf,0x010e), (0x1d0,0x0110), (0x1d1,0x0143), (0x1d2,0x0147), (0x1d5,0x0150),
    (0x1d8,0x0158), (0x1d9,0x016e), (0x1db,0x0170), (0x1de,0x0162), (0x1e0,0x0155), (0x1e3,0x0103),
    (0x1e5,0x013a), (0x1e6,0x0107), (0x1e8,0x010d), (0x1ea,0x0119), (0x1ec,0x011b), (0x1ef,0x010f),
    (0x1f0,0x0111), (0x1f1,0x0144), (0x1f2,0x0148), (0x1f5,0x0151), (0x1f8,0x0159), (0x1f9,0x016f),
    (0x1fb,0x0171), (0x1fe,0x0163), (0x1ff,0x02d9), (0x2a1,0x0126), (0x2a6,0x0124), (0x2a9,0x0130),
    (0x2ab,0x011e), (0x2ac,0x0134), (0x2b1,0x0127), (0x2b6,0x0125), (0x2b9,0x0131), (0x2bb,0x011f),
    (0x2bc,0x0135), (0x2c5,0x010a), (0x2c6,0x0108), (0x2d5,0x0120), (0x2d8,0x011c), (0x2dd,0x016c),
    (0x2de,0x015c), (0x2e5,0x010b), (0x2e6,0x0109), (0x2f5,0x0121), (0x2f8,0x011d), (0x2fd,0x016d),
    (0x2fe,0x015d),
];

fn legacy_char(keysym: u32) -> Option<char> {
    LEGACY_KEYSYMS
        .binary_search_by_key(&keysym, |(k, _)| *k)
        .ok()
        .and_then(|i| char::from_u32(LEGACY_KEYSYMS[i].1))
}

pub fn keysym_to_char(keysym: u32) -> Option<char> {
    match keysym {
        0x20..=0x7e => char::from_u32(keysym),
        0xa0..=0xff => char::from_u32(keysym),
        0x100..=0x2ff => legacy_char(keysym),
        0x01000000..=0x0110ffff => char::from_u32(keysym & 0x00ffffff),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyAction {
    Type(char),
    Key,
}

pub fn key_action(keysym: u32) -> KeyAction {
    match keysym_to_char(keysym) {
        Some(ch) => KeyAction::Type(ch),
        None => KeyAction::Key,
    }
}

pub fn button_name(detail: u8) -> Option<&'static str> {
    match detail {
        1 => Some("left"),
        2 => Some("middle"),
        3 => Some("right"),
        _ => None,
    }
}

pub fn scroll_delta(detail: u8) -> Option<(f64, f64)> {
    match detail {
        4 => Some((0.0, -1.0)),
        5 => Some((0.0, 1.0)),
        6 => Some((-1.0, 0.0)),
        7 => Some((1.0, 0.0)),
        _ => None,
    }
}

pub fn move_packet(x: f64, y: f64) -> serde_json::Value {
    serde_json::json!({"t": "move", "b": "left", "x": x, "y": y})
}

pub fn move_delta_packet(dx: f64, dy: f64) -> serde_json::Value {
    serde_json::json!({"t": "md", "dx": dx, "dy": dy})
}

pub fn hover_packet(x: f64, y: f64) -> serde_json::Value {
    serde_json::json!({"t": "hover", "x": x, "y": y})
}

pub fn button_down_packet(name: &str) -> serde_json::Value {
    serde_json::json!({"t": "button", "b": name, "down": true})
}

pub fn button_up_packet(name: &str) -> serde_json::Value {
    serde_json::json!({"t": "button", "b": name, "down": false})
}

pub fn button_cancel_packet(name: &str) -> serde_json::Value {
    serde_json::json!({"t": "button", "b": name, "down": false, "cancel": true})
}

pub fn scroll_packet(dx: f64, dy: f64) -> serde_json::Value {
    serde_json::json!({"t": "scroll", "dx": dx, "dy": dy})
}

pub fn key_down_packet(code: u16) -> serde_json::Value {
    serde_json::json!({"t": "key", "code": code, "down": true})
}

pub fn key_up_packet(code: u16) -> serde_json::Value {
    serde_json::json!({"t": "key", "code": code, "down": false})
}

pub fn type_packet(ch: char) -> serde_json::Value {
    serde_json::json!({"t": "type", "s": ch.to_string()})
}

pub fn enter_packet(x: f64, y: f64) -> serde_json::Value {
    serde_json::json!({"t": "enter", "x": x, "y": y})
}

pub fn leave_packet() -> serde_json::Value {
    serde_json::json!({"t": "leave"})
}

pub struct KvmHeld {
    buttons: Vec<u8>,
    keys: Vec<u16>,
}

impl KvmHeld {
    pub fn new() -> Self {
        Self {
            buttons: Vec::new(),
            keys: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.buttons.is_empty() && self.keys.is_empty()
    }

    pub fn press_button(&mut self, detail: u8) {
        if button_name(detail).is_some() && !self.buttons.contains(&detail) {
            self.buttons.push(detail);
        }
    }

    pub fn release_button(&mut self, detail: u8) {
        self.buttons.retain(|&d| d != detail);
    }

    pub fn press_key(&mut self, code: u16) {
        if !self.keys.contains(&code) {
            self.keys.push(code);
        }
    }

    pub fn release_key(&mut self, code: u16) {
        self.keys.retain(|&c| c != code);
    }

    pub fn cancel_all(&mut self) -> Vec<serde_json::Value> {
        let mut out = Vec::new();
        for detail in std::mem::take(&mut self.buttons) {
            if let Some(name) = button_name(detail) {
                out.push(button_cancel_packet(name));
            }
        }
        for code in std::mem::take(&mut self.keys) {
            out.push(key_up_packet(code));
        }
        out
    }
}

impl Default for KvmHeld {
    fn default() -> Self {
        Self::new()
    }
}

pub const DRAG_THRESHOLD_NORM: f64 = 0.012;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DragRelease {
    Tap { x: f64, y: f64 },
    Swipe { x: f64, y: f64 },
    Nothing,
}

pub struct DragTracker {
    held: bool,
    moved: bool,
    start: (f64, f64),
    last: (f64, f64),
}

impl DragTracker {
    pub fn new() -> Self {
        Self {
            held: false,
            moved: false,
            start: (0.0, 0.0),
            last: (0.0, 0.0),
        }
    }

    pub fn is_held(&self) -> bool {
        self.held
    }

    pub fn press(&mut self, x: f64, y: f64) {
        self.held = true;
        self.moved = false;
        self.start = (x, y);
        self.last = (x, y);
    }

    pub fn clear(&mut self) {
        self.held = false;
        self.moved = false;
    }

    pub fn motion(&mut self, x: f64, y: f64) -> bool {
        if !self.held {
            return false;
        }
        self.last = (x, y);
        let dx = x - self.start.0;
        let dy = y - self.start.1;
        if dx * dx + dy * dy >= DRAG_THRESHOLD_NORM * DRAG_THRESHOLD_NORM {
            self.moved = true;
        }
        true
    }

    pub fn release(&mut self, x: f64, y: f64) -> DragRelease {
        if !self.held {
            return DragRelease::Nothing;
        }
        self.held = false;
        let end = if self.moved { (x, y) } else { self.last };
        if self.moved {
            self.moved = false;
            DragRelease::Swipe { x: end.0, y: end.1 }
        } else {
            DragRelease::Tap {
                x: self.last.0,
                y: self.last.1,
            }
        }
    }
}

impl Default for DragTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coords_normalize_and_clamp() {
        assert_eq!(normalize_coord(540.0, 1080.0), 0.5);
        assert_eq!(normalize_coord(-5.0, 1080.0), 0.0);
        assert_eq!(normalize_coord(9999.0, 1080.0), 1.0);
        assert_eq!(normalize_coord(10.0, 0.0), 0.0);
    }

    #[test]
    fn keycode_maps_to_evdev() {
        assert_eq!(xkey_to_evdev(36), 28);
        assert_eq!(xkey_to_evdev(8), 0);
        assert_eq!(xkey_to_evdev(0), 0);
    }

    #[test]
    fn keysym_ascii_prefers_type() {
        assert_eq!(key_action(0x61), KeyAction::Type('a'));
        assert_eq!(key_action(0x20), KeyAction::Type(' '));
        assert_eq!(key_action(0xe4), KeyAction::Type('\u{e4}'));
        assert_eq!(key_action(0xff08), KeyAction::Key);
        assert_eq!(key_action(0xff1b), KeyAction::Key);
        assert_eq!(key_action(0), KeyAction::Key);
    }

    #[test]
    fn legacy_keysyms_cover_turkish() {
        assert_eq!(keysym_to_char(0x02bb), Some('\u{11f}'));
        assert_eq!(keysym_to_char(0x02ab), Some('\u{11e}'));
        assert_eq!(keysym_to_char(0x02b9), Some('\u{131}'));
        assert_eq!(keysym_to_char(0x02a9), Some('\u{130}'));
        assert_eq!(keysym_to_char(0x01aa), Some('\u{15e}'));
        assert_eq!(keysym_to_char(0x01ba), Some('\u{15f}'));
        assert_eq!(keysym_to_char(0x01a9), Some('\u{160}'));
        assert_eq!(keysym_to_char(0x1a4), None);
        assert_eq!(key_action(0x02bb), KeyAction::Type('\u{11f}'));
    }

    #[test]
    fn buttons_and_scroll_map() {
        assert_eq!(button_name(1), Some("left"));
        assert_eq!(button_name(2), Some("middle"));
        assert_eq!(button_name(3), Some("right"));
        assert_eq!(button_name(4), None);
        assert_eq!(scroll_delta(4), Some((0.0, -1.0)));
        assert_eq!(scroll_delta(5), Some((0.0, 1.0)));
        assert_eq!(scroll_delta(6), Some((-1.0, 0.0)));
        assert_eq!(scroll_delta(7), Some((1.0, 0.0)));
        assert_eq!(scroll_delta(1), None);
    }

    #[test]
    fn hover_packet_carries_position_only() {
        let pkt = hover_packet(0.25, 0.75);
        assert_eq!(pkt["t"], "hover");
        assert_eq!(pkt["x"], 0.25);
        assert_eq!(pkt["y"], 0.75);
        assert!(pkt.get("b").is_none());
        assert!(pkt.get("down").is_none());
    }

    #[test]
    fn hover_motion_never_arms_drag() {
        let mut drag = DragTracker::new();
        assert!(!drag.is_held());
        assert!(!drag.motion(0.5, 0.5));
        assert_eq!(drag.release(0.5, 0.5), DragRelease::Nothing);
    }

    #[test]
    fn press_release_without_move_is_tap() {
        let mut drag = DragTracker::new();
        drag.press(0.3, 0.4);
        assert!(drag.is_held());
        assert!(drag.motion(0.301, 0.401));
        assert_eq!(
            drag.release(0.301, 0.401),
            DragRelease::Tap { x: 0.301, y: 0.401 }
        );
        assert!(!drag.is_held());
    }

    #[test]
    fn press_move_release_is_swipe_to_release_point() {
        let mut drag = DragTracker::new();
        drag.press(0.2, 0.2);
        assert!(drag.motion(0.5, 0.6));
        assert!(drag.motion(0.55, 0.65));
        assert_eq!(
            drag.release(0.6, 0.7),
            DragRelease::Swipe { x: 0.6, y: 0.7 }
        );
    }

    #[test]
    fn double_click_produces_two_taps() {
        let mut drag = DragTracker::new();
        drag.press(0.5, 0.5);
        let first = drag.release(0.5, 0.5);
        drag.press(0.5, 0.5);
        let second = drag.release(0.5, 0.5);
        assert!(matches!(first, DragRelease::Tap { .. }));
        assert!(matches!(second, DragRelease::Tap { .. }));
    }

    #[test]
    fn wire_packets_carry_drag_shapes() {
        let pkt = move_packet(0.25, 0.75);
        assert_eq!(pkt["t"], "move");
        assert_eq!(pkt["b"], "left");
        assert_eq!(pkt["x"], 0.25);
        assert_eq!(pkt["y"], 0.75);
        let pkt = button_down_packet("left");
        assert_eq!(pkt["t"], "button");
        assert_eq!(pkt["down"], true);
        assert!(pkt.get("x").is_none());
        let pkt = button_up_packet("left");
        assert_eq!(pkt["down"], false);
        assert!(pkt.get("x").is_none());
        assert!(pkt.get("y").is_none());
        assert!(pkt.get("cancel").is_none());
        let pkt = button_cancel_packet("left");
        assert_eq!(pkt["down"], false);
        assert_eq!(pkt["cancel"], true);
    }

    #[test]
    fn clear_cancels_pending_press() {
        let mut drag = DragTracker::new();
        drag.press(0.4, 0.4);
        drag.clear();
        assert!(!drag.is_held());
        assert_eq!(drag.release(0.4, 0.4), DragRelease::Nothing);
        assert!(!drag.motion(0.5, 0.5));
    }

    #[test]
    fn kvm_packets_carry_forward_shapes() {
        let pkt = scroll_packet(0.0, -1.0);
        assert_eq!(pkt["t"], "scroll");
        assert_eq!(pkt["dx"], 0.0);
        assert_eq!(pkt["dy"], -1.0);
        let pkt = key_down_packet(30);
        assert_eq!(pkt["t"], "key");
        assert_eq!(pkt["code"], 30);
        assert_eq!(pkt["down"], true);
        let pkt = key_up_packet(30);
        assert_eq!(pkt["down"], false);
        let pkt = type_packet('a');
        assert_eq!(pkt["t"], "type");
        assert_eq!(pkt["s"], "a");
        let pkt = enter_packet(0.25, 0.75);
        assert_eq!(pkt["t"], "enter");
        assert_eq!(pkt["x"], 0.25);
        assert_eq!(pkt["y"], 0.75);
        assert_eq!(leave_packet()["t"], "leave");
    }

    #[test]
    fn kvm_held_tracks_and_cancels() {
        let mut held = KvmHeld::new();
        assert!(held.is_empty());
        held.press_button(1);
        held.press_button(1);
        held.press_button(4);
        held.press_key(30);
        held.press_key(30);
        assert!(!held.is_empty());
        held.release_button(3);
        held.release_key(31);
        let pkts = held.cancel_all();
        assert_eq!(pkts.len(), 2);
        assert_eq!(pkts[0]["t"], "button");
        assert_eq!(pkts[0]["b"], "left");
        assert_eq!(pkts[0]["cancel"], true);
        assert_eq!(pkts[1]["t"], "key");
        assert_eq!(pkts[1]["code"], 30);
        assert_eq!(pkts[1]["down"], false);
        assert!(held.is_empty());
        assert!(held.cancel_all().is_empty());
    }

    #[test]
    fn kvm_held_release_removes() {
        let mut held = KvmHeld::new();
        held.press_button(1);
        held.press_button(3);
        held.press_key(30);
        held.release_button(1);
        held.release_key(30);
        let pkts = held.cancel_all();
        assert_eq!(pkts.len(), 1);
        assert_eq!(pkts[0]["b"], "right");
    }
}
