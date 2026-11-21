mod control;
mod decode;
mod display;
mod events;
mod frame;
mod server;

pub use control::{
    CAP, DEFAULT_FPS, DEFAULT_MAX_W, FRAME_OPEN_KIND, handle_control_from, normalize_fps,
    normalize_max_w,
};
pub use decode::{DecodedFrame, H264Decoder, rgba_to_bgra_in_place};
pub use display::{MirrorWindow, TITLE};
pub use events::{
    KeyAction, button_name, key_action, keysym_to_char, normalize_coord, scroll_delta,
    xkey_to_evdev,
};
pub use frame::{CODEC_H264, HEADER_LEN, MAGIC, MAX_PAYLOAD, VERSION, FrameMeta, build_header, parse_header};
pub use server::{FRAME_QUEUE_CAP, MAX_LINE, MOTION_THROTTLE, MirrorServer};
