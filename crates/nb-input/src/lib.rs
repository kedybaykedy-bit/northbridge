use serde::{Deserialize, Serialize};

pub const MAX_MESSAGE_LEN: u32 = 1 << 20;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum InputMsg {
    Enter { x: f64, y: f64 },
    Leave,
    Move { x: f64, y: f64 },
    Button { b: Button, down: bool },
    Scroll { dx: f64, dy: f64 },
    Key { code: u16, down: bool },
    RelMove { dx: f64, dy: f64 },
    Tap { b: Button, count: u8 },
    ClickAbs { x: f64, y: f64 },
    Type { s: String },
    Capture,
    Release,
    Ping,
    Pong,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Button {
    Left,
    Right,
    Middle,
}

pub fn encode(msg: &InputMsg) -> Vec<u8> {
    let payload = serde_json::to_vec(msg).expect("InputMsg must serialize to JSON");
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(&payload);
    frame
}

#[derive(Debug)]
pub enum DecodeError {
    Oversize { len: u32 },
    InvalidUtf8,
    InvalidJson(serde_json::Error),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::Oversize { len } => {
                write!(f, "declared message length {len} exceeds {MAX_MESSAGE_LEN}")
            }
            DecodeError::InvalidUtf8 => write!(f, "message payload is not valid UTF-8"),
            DecodeError::InvalidJson(err) => {
                write!(f, "message payload is not a valid InputMsg: {err}")
            }
        }
    }
}

impl std::error::Error for DecodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DecodeError::InvalidJson(err) => Some(err),
            DecodeError::Oversize { .. } | DecodeError::InvalidUtf8 => None,
        }
    }
}

pub struct Decoder {
    buffer: Vec<u8>,
    deferred: Vec<InputMsg>,
    oversize: Option<u32>,
}

impl Decoder {
    pub fn new() -> Decoder {
        Decoder {
            buffer: Vec::new(),
            deferred: Vec::new(),
            oversize: None,
        }
    }

    pub fn feed(&mut self, data: &[u8]) -> Result<Vec<InputMsg>, DecodeError> {
        if let Some(len) = self.oversize {
            return Err(DecodeError::Oversize { len });
        }
        self.buffer.extend_from_slice(data);
        let mut msgs = Vec::new();
        while self.buffer.len() >= 4 {
            let len = u32::from_be_bytes([
                self.buffer[0],
                self.buffer[1],
                self.buffer[2],
                self.buffer[3],
            ]);
            if len > MAX_MESSAGE_LEN {
                self.buffer.clear();
                self.deferred.clear();
                self.oversize = Some(len);
                return Err(DecodeError::Oversize { len });
            }
            let total = 4 + len as usize;
            if self.buffer.len() < total {
                break;
            }
            let frame: Vec<u8> = self.buffer.drain(..total).collect();
            match decode_payload(&frame[4..]) {
                Ok(msg) => msgs.push(msg),
                Err(err) => {
                    self.deferred.extend(msgs);
                    return Err(err);
                }
            }
        }
        if self.deferred.is_empty() {
            Ok(msgs)
        } else {
            let mut all = std::mem::take(&mut self.deferred);
            all.append(&mut msgs);
            Ok(all)
        }
    }

    pub fn pending(&self) -> usize {
        self.buffer.len()
    }
}

fn decode_payload(payload: &[u8]) -> Result<InputMsg, DecodeError> {
    let text = std::str::from_utf8(payload).map_err(|_| DecodeError::InvalidUtf8)?;
    serde_json::from_str(text).map_err(DecodeError::InvalidJson)
}

pub mod zone {
    pub const DEFAULT_MARGIN: i32 = 1;

    pub fn in_jump_zone(x: i32, y: i32, w: i32, h: i32, margin: i32) -> Option<(f64, f64)> {
        if distance_to_edge(x, w) < margin || distance_to_edge(y, h) < margin {
            Some((
                (x as f64 / w as f64).clamp(0.0, 1.0),
                (y as f64 / h as f64).clamp(0.0, 1.0),
            ))
        } else {
            None
        }
    }

    fn distance_to_edge(value: i32, size: i32) -> i32 {
        if value < 0 {
            -value
        } else if value >= size {
            value - size + 1
        } else {
            value.min(size - 1 - value)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(msg: InputMsg) {
        let bytes = encode(&msg);
        let mut decoder = Decoder::new();
        assert_eq!(decoder.feed(&bytes).unwrap(), vec![msg]);
        assert_eq!(decoder.pending(), 0);
    }

    #[test]
    fn roundtrip_each_variant() {
        roundtrip(InputMsg::Enter { x: 0.25, y: 0.75 });
        roundtrip(InputMsg::Leave);
        roundtrip(InputMsg::Move { x: 0.5, y: 0.5 });
        roundtrip(InputMsg::Button {
            b: Button::Left,
            down: true,
        });
        roundtrip(InputMsg::Scroll { dx: -1.0, dy: 3.5 });
        roundtrip(InputMsg::Key {
            code: 30,
            down: false,
        });
        roundtrip(InputMsg::RelMove { dx: -12.5, dy: 7.25 });
        roundtrip(InputMsg::Tap {
            b: Button::Right,
            count: 2,
        });
        roundtrip(InputMsg::ClickAbs { x: 0.9, y: 0.1 });
        roundtrip(InputMsg::Type {
            s: "Merhaba Dünya — çğıöşü".into(),
        });
        roundtrip(InputMsg::Capture);
        roundtrip(InputMsg::Release);
        roundtrip(InputMsg::Ping);
        roundtrip(InputMsg::Pong);
    }

    #[test]
    fn serde_names_for_new_variants() {
        assert_eq!(
            serde_json::to_value(InputMsg::Type { s: "a".into() }).unwrap()["t"],
            "type"
        );
        assert_eq!(
            serde_json::to_value(InputMsg::Capture).unwrap()["t"],
            "capture"
        );
        assert_eq!(
            serde_json::to_value(InputMsg::Release).unwrap()["t"],
            "release"
        );
    }

    #[test]
    fn roundtrip_all_buttons() {
        for b in [Button::Left, Button::Right, Button::Middle] {
            roundtrip(InputMsg::Button { b, down: true });
            roundtrip(InputMsg::Button { b, down: false });
            roundtrip(InputMsg::Tap { b, count: 3 });
        }
    }

    #[test]
    fn wire_format_is_length_prefixed_json() {
        let bytes = encode(&InputMsg::Ping);
        let len = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        assert_eq!(len as usize, bytes.len() - 4);
        assert_eq!(&bytes[4..], br#"{"t":"ping"}"#);
    }

    #[test]
    fn serde_names() {
        let value = serde_json::to_value(InputMsg::RelMove { dx: 1.0, dy: 2.0 }).unwrap();
        assert_eq!(value["t"], "rel_move");
        let value = serde_json::to_value(InputMsg::ClickAbs { x: 0.0, y: 1.0 }).unwrap();
        assert_eq!(value["t"], "click_abs");
        assert_eq!(serde_json::to_value(Button::Left).unwrap(), "left");
        assert_eq!(serde_json::to_value(Button::Right).unwrap(), "right");
        assert_eq!(serde_json::to_value(Button::Middle).unwrap(), "middle");
    }

    #[test]
    fn feed_byte_by_byte() {
        let msgs = vec![
            InputMsg::Enter { x: 0.1, y: 0.2 },
            InputMsg::Key {
                code: 28,
                down: true,
            },
            InputMsg::Leave,
            InputMsg::RelMove { dx: -3.5, dy: 4.5 },
        ];
        let stream: Vec<u8> = msgs.iter().flat_map(encode).collect();
        let mut decoder = Decoder::new();
        let mut received = Vec::new();
        for byte in stream {
            received.extend(decoder.feed(&[byte]).unwrap());
        }
        assert_eq!(received, msgs);
        assert_eq!(decoder.pending(), 0);
    }

    #[test]
    fn feed_two_messages_in_one_buffer() {
        let a = InputMsg::Ping;
        let b = InputMsg::Move { x: 0.3, y: 0.4 };
        let mut buffer = encode(&a);
        buffer.extend(encode(&b));
        let mut decoder = Decoder::new();
        assert_eq!(decoder.feed(&buffer).unwrap(), vec![a, b]);
        assert_eq!(decoder.pending(), 0);
    }

    #[test]
    fn feed_partial_frames_report_pending() {
        let frame = encode(&InputMsg::Pong);
        let mut decoder = Decoder::new();
        assert!(decoder.feed(&[]).unwrap().is_empty());
        assert!(decoder.feed(&frame[..3]).unwrap().is_empty());
        assert_eq!(decoder.pending(), 3);
        assert!(decoder.feed(&frame[3..5]).unwrap().is_empty());
        assert_eq!(decoder.pending(), 5);
        assert_eq!(decoder.feed(&frame[5..]).unwrap(), vec![InputMsg::Pong]);
        assert_eq!(decoder.pending(), 0);
    }

    #[test]
    fn oversize_prefix_poisons_decoder() {
        let mut frame = (MAX_MESSAGE_LEN + 1).to_be_bytes().to_vec();
        frame.extend_from_slice(&[0u8; 16]);
        let mut decoder = Decoder::new();
        match decoder.feed(&frame) {
            Err(DecodeError::Oversize { len }) => assert_eq!(len, MAX_MESSAGE_LEN + 1),
            other => panic!("expected oversize error, got {other:?}"),
        }
        assert_eq!(decoder.pending(), 0);
        match decoder.feed(&encode(&InputMsg::Ping)) {
            Err(DecodeError::Oversize { len }) => assert_eq!(len, MAX_MESSAGE_LEN + 1),
            other => panic!("expected oversize error, got {other:?}"),
        }

        let mut decoder = Decoder::new();
        let prefix = (MAX_MESSAGE_LEN + 1).to_be_bytes();
        for (i, byte) in prefix.iter().enumerate() {
            let result = decoder.feed(&[*byte]);
            if i < 3 {
                assert!(result.unwrap().is_empty());
            } else {
                assert!(matches!(result, Err(DecodeError::Oversize { .. })));
            }
        }
        assert_eq!(decoder.pending(), 0);
    }

    #[test]
    fn invalid_json_is_rejected() {
        let payload = b"definitely not json";
        let mut frame = (payload.len() as u32).to_be_bytes().to_vec();
        frame.extend_from_slice(payload);
        let mut decoder = Decoder::new();
        match decoder.feed(&frame) {
            Err(DecodeError::InvalidJson(_)) => {}
            other => panic!("expected invalid json error, got {other:?}"),
        }
        assert_eq!(decoder.pending(), 0);

        let payload = br#"{"t":"unknown_variant"}"#;
        let mut frame = (payload.len() as u32).to_be_bytes().to_vec();
        frame.extend_from_slice(payload);
        let mut decoder = Decoder::new();
        assert!(matches!(
            decoder.feed(&frame),
            Err(DecodeError::InvalidJson(_))
        ));
    }

    #[test]
    fn invalid_json_keeps_subsequent_message() {
        let payload = b"nope";
        let mut buffer = (payload.len() as u32).to_be_bytes().to_vec();
        buffer.extend_from_slice(payload);
        buffer.extend(encode(&InputMsg::Ping));
        let mut decoder = Decoder::new();
        assert!(matches!(
            decoder.feed(&buffer),
            Err(DecodeError::InvalidJson(_))
        ));
        assert_eq!(decoder.feed(&[]).unwrap(), vec![InputMsg::Ping]);
        assert_eq!(decoder.pending(), 0);
    }

    #[test]
    fn valid_messages_before_invalid_one_are_preserved() {
        let payload = b"nope";
        let mut buffer = encode(&InputMsg::Pong);
        buffer.extend((payload.len() as u32).to_be_bytes());
        buffer.extend_from_slice(payload);
        buffer.extend(encode(&InputMsg::Leave));
        let mut decoder = Decoder::new();
        assert!(matches!(
            decoder.feed(&buffer),
            Err(DecodeError::InvalidJson(_))
        ));
        assert_eq!(
            decoder.feed(&[]).unwrap(),
            vec![InputMsg::Pong, InputMsg::Leave]
        );
        assert_eq!(decoder.pending(), 0);
    }

    #[test]
    fn invalid_utf8_is_rejected() {
        let mut frame = 2u32.to_be_bytes().to_vec();
        frame.extend_from_slice(&[0xff, 0xfe]);
        let mut decoder = Decoder::new();
        assert!(matches!(decoder.feed(&frame), Err(DecodeError::InvalidUtf8)));
    }

    #[test]
    fn decode_error_display_and_source() {
        let err = DecodeError::Oversize { len: u32::MAX };
        assert!(!err.to_string().is_empty());
        assert!(std::error::Error::source(&err).is_none());
        let err = DecodeError::InvalidUtf8;
        assert!(!err.to_string().is_empty());
        assert!(std::error::Error::source(&err).is_none());
        let json_err = serde_json::from_str::<InputMsg>("}").unwrap_err();
        let err = DecodeError::InvalidJson(json_err);
        assert!(!err.to_string().is_empty());
        assert!(std::error::Error::source(&err).is_some());
    }

    #[test]
    fn jump_zone_corner() {
        assert_eq!(
            zone::in_jump_zone(0, 0, 100, 80, zone::DEFAULT_MARGIN),
            Some((0.0, 0.0))
        );
        assert_eq!(
            zone::in_jump_zone(99, 79, 100, 80, zone::DEFAULT_MARGIN),
            Some((0.99, 0.9875))
        );
        assert_eq!(zone::DEFAULT_MARGIN, 1);
    }

    #[test]
    fn jump_zone_inside_and_far_from_edge_are_none() {
        assert_eq!(zone::in_jump_zone(50, 40, 100, 80, 1), None);
        assert_eq!(zone::in_jump_zone(10, 10, 100, 80, 5), None);
        assert_eq!(zone::in_jump_zone(90, 70, 100, 80, 5), None);
    }

    #[test]
    fn jump_zone_edge_margin_boundary() {
        assert_eq!(zone::in_jump_zone(0, 40, 100, 80, 1), Some((0.0, 0.5)));
        assert_eq!(zone::in_jump_zone(1, 40, 100, 80, 1), None);
        assert_eq!(zone::in_jump_zone(1, 40, 100, 80, 2), Some((0.01, 0.5)));
        assert_eq!(zone::in_jump_zone(10, 10, 100, 80, 11), Some((0.1, 0.125)));
        assert_eq!(zone::in_jump_zone(99, 40, 100, 80, 1), Some((0.99, 0.5)));
        assert_eq!(zone::in_jump_zone(98, 40, 100, 80, 1), None);
        assert_eq!(zone::in_jump_zone(50, 0, 100, 80, 1), Some((0.5, 0.0)));
        assert_eq!(zone::in_jump_zone(50, 79, 100, 80, 1), Some((0.5, 0.9875)));
        assert_eq!(zone::in_jump_zone(50, 78, 100, 80, 1), None);
    }

    #[test]
    fn jump_zone_margin_zero_is_never_active() {
        assert_eq!(zone::in_jump_zone(0, 0, 100, 80, 0), None);
        assert_eq!(zone::in_jump_zone(0, 40, 100, 80, 0), None);
        assert_eq!(zone::in_jump_zone(99, 79, 100, 80, 0), None);
        assert_eq!(zone::in_jump_zone(50, 40, 100, 80, 0), None);
    }

    #[test]
    fn jump_zone_outside_screen() {
        assert_eq!(zone::in_jump_zone(-5, 40, 100, 80, 1), None);
        assert_eq!(zone::in_jump_zone(105, 40, 100, 80, 1), None);
        assert_eq!(zone::in_jump_zone(-1, 40, 100, 80, 2), Some((0.0, 0.5)));
        assert_eq!(zone::in_jump_zone(101, 40, 100, 80, 2), None);
        assert_eq!(zone::in_jump_zone(100, 40, 100, 80, 2), Some((1.0, 0.5)));
        assert_eq!(
            zone::in_jump_zone(101, 40, 100, 80, 3),
            Some((1.0, 0.5))
        );
    }
}
