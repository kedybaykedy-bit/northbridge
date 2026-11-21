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
pub use display::{KvmGrab, MirrorWindow, TITLE};
pub use events::{
    DragRelease, DragTracker, KeyAction, KvmHeld, button_cancel_packet, button_down_packet,
    button_name, button_up_packet, enter_packet, key_action, key_down_packet, key_up_packet,
    keysym_to_char, leave_packet, move_packet, normalize_coord, scroll_delta, scroll_packet,
    type_packet, xkey_to_evdev,
};
pub use frame::{CODEC_H264, HEADER_LEN, MAGIC, MAX_PAYLOAD, VERSION, FrameMeta, build_header, parse_header};
pub use server::{FRAME_QUEUE_CAP, KVM_KIND, MAX_LINE, MOTION_THROTTLE, MirrorServer};
