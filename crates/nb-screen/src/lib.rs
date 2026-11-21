mod capture;
mod control;
mod frame;
mod queue;
mod server;

pub use capture::Strategy;
pub use control::{
    CAP, DEFAULT_FPS, DEFAULT_MAX_W, FRAME_OPEN_KIND, handle_control, handle_control_from,
};
pub use frame::{
    CODEC_JPEG, HEADER_LEN, MAGIC, VERSION, FrameMeta, build_header, parse_header,
};
pub use queue::{FrameQueue, QueuedFrame};
pub use server::{JPEG_QUALITY, QUEUE_CAP, ScreenServer};
