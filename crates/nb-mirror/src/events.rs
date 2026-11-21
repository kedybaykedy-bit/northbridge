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
}
