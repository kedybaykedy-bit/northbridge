pub fn normalize_coord(px: f64, extent: f64) -> f64 {
    if extent <= 0.0 {
        return 0.0;
    }
    (px / extent).clamp(0.0, 1.0)
}

pub fn xkey_to_evdev(xkey: u16) -> u16 {
    xkey.saturating_sub(8)
}

pub fn keysym_to_char(keysym: u32) -> Option<char> {
    match keysym {
        0x20..=0x7e => char::from_u32(keysym),
        0xa0..=0xff => char::from_u32(keysym),
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

pub fn hover_packet(x: f64, y: f64) -> serde_json::Value {
    serde_json::json!({"t": "hover", "x": x, "y": y})
}

pub fn button_down_packet(name: &str, x: f64, y: f64) -> serde_json::Value {
    serde_json::json!({"t": "button", "b": name, "down": true, "x": x, "y": y})
}

pub fn button_up_packet(name: &str, coords: Option<(f64, f64)>) -> serde_json::Value {
    match coords {
        Some((x, y)) => serde_json::json!({"t": "button", "b": name, "down": false, "x": x, "y": y}),
        None => serde_json::json!({"t": "button", "b": name, "down": false}),
    }
}

pub fn button_cancel_packet(name: &str) -> serde_json::Value {
    serde_json::json!({"t": "button", "b": name, "down": false, "cancel": true})
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
        let pkt = button_down_packet("left", 0.5, 0.5);
        assert_eq!(pkt["t"], "button");
        assert_eq!(pkt["down"], true);
        assert_eq!(pkt["x"], 0.5);
        let pkt = button_up_packet("left", Some((0.6, 0.4)));
        assert_eq!(pkt["down"], false);
        assert_eq!(pkt["x"], 0.6);
        assert_eq!(pkt["y"], 0.4);
        let pkt = button_up_packet("right", None);
        assert_eq!(pkt["down"], false);
        assert!(pkt.get("x").is_none());
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
}
