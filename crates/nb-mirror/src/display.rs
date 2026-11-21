use anyhow::{Context, Result};
use std::collections::HashMap;
use std::io::ErrorKind;
use x11rb::connection::Connection;
use x11rb::errors::{ConnectionError, ParseError, ReplyError, ReplyOrIdError};
use x11rb::protocol::Event;
use x11rb::protocol::xfixes::ConnectionExt as _;
use x11rb::protocol::xproto::{
    AtomEnum, ConfigureWindowAux, ConnectionExt, CreateGCAux, CreateWindowAux, EventMask, GrabMode,
    GrabStatus, ImageFormat, InputFocus, PropMode, Window, WindowClass,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;
use x11rb::{COPY_DEPTH_FROM_PARENT, COPY_FROM_PARENT};

pub const TITLE: &[u8] = b"Northbridge \xe2\x80\x94 mirror";

pub struct KvmGrab {
    conn: RustConnection,
    root: Window,
    width: u16,
    height: u16,
    prev_focus: Window,
    prev_revert: InputFocus,
    keysym_cache: HashMap<u8, Vec<u32>>,
    done: bool,
}

impl KvmGrab {
    pub fn acquire() -> Result<Self> {
        let (conn, screen) = RustConnection::connect(None).context("X display")?;
        let setup = conn.setup();
        let root = setup.roots[screen].root;
        let width = setup.roots[screen].width_in_pixels;
        let height = setup.roots[screen].height_in_pixels;
        let prev = conn.get_input_focus()?.reply().context("get focus")?;
        let _ = conn.xfixes_query_version(5, 0)?.reply();
        if conn
            .xfixes_hide_cursor(root)
            .ok()
            .and_then(|c| c.check().ok())
            .is_none()
        {
            anyhow::bail!("hide cursor failed");
        }
        let mut grabbed = Self {
            conn,
            root,
            width,
            height,
            prev_focus: prev.focus,
            prev_revert: prev.revert_to,
            keysym_cache: HashMap::new(),
            done: false,
        };
        let mut last_status = GrabStatus::SUCCESS;
        for _ in 0..20 {
            let status = grabbed
                .conn
                .grab_pointer(
                    false,
                    root,
                    EventMask::BUTTON_PRESS
                        | EventMask::BUTTON_RELEASE
                        | EventMask::POINTER_MOTION
                        | EventMask::BUTTON_MOTION,
                    GrabMode::ASYNC,
                    GrabMode::ASYNC,
                    x11rb::NONE,
                    x11rb::NONE,
                    x11rb::CURRENT_TIME,
                )?
                .reply()
                .context("grab pointer")?
                .status;
            if status == GrabStatus::SUCCESS {
                last_status = status;
                break;
            }
            last_status = status;
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        if last_status != GrabStatus::SUCCESS {
            grabbed.shutdown();
            anyhow::bail!("grab pointer failed: status {}", u8::from(last_status));
        }
        let kstatus = grabbed
            .conn
            .grab_keyboard(
                false,
                root,
                x11rb::CURRENT_TIME,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
            )?
            .reply()
            .context("grab keyboard")?
            .status;
        if kstatus != GrabStatus::SUCCESS {
            grabbed.shutdown();
            anyhow::bail!("grab keyboard failed: status {}", u8::from(kstatus));
        }
        grabbed
            .conn
            .set_input_focus(InputFocus::POINTER_ROOT, root, x11rb::CURRENT_TIME)?
            .check()
            .ok();
        grabbed.conn.flush().ok();
        Ok(grabbed)
    }

    pub fn screen_size(&self) -> (u16, u16) {
        (self.width, self.height)
    }

    pub fn pointer(&self) -> Result<(i32, i32)> {
        let r = self.conn.query_pointer(self.root)?.reply().context("query pointer")?;
        Ok((r.root_x as i32, r.root_y as i32))
    }

    pub fn poll_event(&self) -> Result<Option<Event>> {
        self.conn.poll_for_event().context("poll event")
    }

    pub fn keysyms(&mut self, keycode: u8) -> Vec<u32> {
        if let Some(cached) = self.keysym_cache.get(&keycode) {
            return cached.clone();
        }
        let mut fetched = Vec::new();
        if let Ok(cookie) = self.conn.get_keyboard_mapping(keycode, 1) {
            if let Ok(reply) = cookie.reply() {
                fetched = reply.keysyms.to_vec();
            }
        }
        self.keysym_cache.insert(keycode, fetched.clone());
        fetched
    }

    pub fn release(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        if self.done {
            return;
        }
        self.done = true;
        self.conn.ungrab_keyboard(x11rb::CURRENT_TIME).ok();
        self.conn.ungrab_pointer(x11rb::CURRENT_TIME).ok();
        self.conn.xfixes_show_cursor(self.root).ok();
        self.conn
            .set_input_focus(self.prev_revert, self.prev_focus, x11rb::CURRENT_TIME)
            .ok();
        self.conn.flush().ok();
    }
}

impl Drop for KvmGrab {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn io_kind_dead(kind: ErrorKind) -> bool {
    matches!(
        kind,
        ErrorKind::BrokenPipe
            | ErrorKind::ConnectionReset
            | ErrorKind::ConnectionAborted
            | ErrorKind::NotConnected
            | ErrorKind::UnexpectedEof
    )
}

fn connection_error_dead(err: &ConnectionError) -> bool {
    match err {
        ConnectionError::IoError(io) => io_kind_dead(io.kind()),
        ConnectionError::ParseError(_) => true,
        _ => false,
    }
}

pub fn is_connection_dead(err: &anyhow::Error) -> bool {
    for cause in err.chain() {
        if let Some(conn) = cause.downcast_ref::<ConnectionError>() {
            if connection_error_dead(conn) {
                return true;
            }
            continue;
        }
        if let Some(reply) = cause.downcast_ref::<ReplyError>() {
            match reply {
                ReplyError::ConnectionError(conn) => {
                    if connection_error_dead(conn) {
                        return true;
                    }
                }
                ReplyError::X11Error(_) => {}
            }
            continue;
        }
        if let Some(reply) = cause.downcast_ref::<ReplyOrIdError>() {
            match reply {
                ReplyOrIdError::ConnectionError(conn) => {
                    if connection_error_dead(conn) {
                        return true;
                    }
                }
                ReplyOrIdError::IdsExhausted | ReplyOrIdError::X11Error(_) => {}
            }
            continue;
        }
        if let Some(parse) = cause.downcast_ref::<ParseError>() {
            let _ = parse;
            return true;
        }
        if let Some(io) = cause.downcast_ref::<std::io::Error>() {
            if io_kind_dead(io.kind()) {
                return true;
            }
            continue;
        }
        let msg = cause.to_string().to_lowercase();
        if msg.contains("closed the connection")
            || msg.contains("broken pipe")
            || msg.contains("connection reset")
            || msg.contains("connection aborted")
        {
            return true;
        }
    }
    false
}

pub struct MirrorWindow {
    conn: RustConnection,
    screen: usize,
    window: u32,
    gc: u32,
    width: u16,
    height: u16,
    keysym_cache: HashMap<u8, Vec<u32>>,
    net_wm_name: u32,
    utf8_string: u32,
}

impl MirrorWindow {
    pub fn connect() -> Result<Self> {
        let (conn, screen) = RustConnection::connect(None).context("X display")?;
        let setup = conn.setup();
        let root = setup.roots[screen].root;
        let black = setup.roots[screen].black_pixel;
        let window: u32 = conn.generate_id().context("window id")?;
        let mask = EventMask::EXPOSURE
            | EventMask::ENTER_WINDOW
            | EventMask::BUTTON_PRESS
            | EventMask::BUTTON_RELEASE
            | EventMask::POINTER_MOTION
            | EventMask::BUTTON_MOTION
            | EventMask::KEY_PRESS
            | EventMask::KEY_RELEASE
            | EventMask::FOCUS_CHANGE
            | EventMask::STRUCTURE_NOTIFY;
        conn.create_window(
            COPY_DEPTH_FROM_PARENT,
            window,
            root,
            0,
            0,
            320,
            240,
            0,
            WindowClass::INPUT_OUTPUT,
            COPY_FROM_PARENT,
            &CreateWindowAux::new()
                .background_pixel(black)
                .event_mask(mask),
        )
        .context("create window")?
        .check()
        .context("create window check")?;
        let gc: u32 = conn.generate_id().context("gc id")?;
        conn.create_gc(gc, window, &CreateGCAux::new())
            .context("create gc")?
            .check()
            .context("create gc check")?;
        conn.change_property8(
            PropMode::REPLACE,
            window,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            TITLE,
        )
        .context("window title")?
        .check()
        .ok();
        let net_wm_name = conn
            .intern_atom(false, b"_NET_WM_NAME")
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|r| r.atom)
            .unwrap_or(0);
        let utf8_string = conn
            .intern_atom(false, b"UTF8_STRING")
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|r| r.atom)
            .unwrap_or(0);
        if net_wm_name != 0 && utf8_string != 0 {
            if let Ok(cookie) = conn.change_property8(
                PropMode::REPLACE,
                window,
                net_wm_name,
                utf8_string,
                TITLE,
            ) {
                let _ = cookie.check();
            }
        }
        conn.map_window(window)
            .context("map window")?
            .check()
            .context("map window check")?;
        conn.flush().context("flush")?;
        Ok(Self {
            conn,
            screen,
            window,
            gc,
            width: 320,
            height: 240,
            keysym_cache: HashMap::new(),
            net_wm_name,
            utf8_string,
        })
    }

    pub fn set_title(&mut self, text: &[u8]) -> bool {
        if self.net_wm_name == 0 || self.utf8_string == 0 {
            return true;
        }
        let cookie = match self.conn.change_property8(
            PropMode::REPLACE,
            self.window,
            self.net_wm_name,
            self.utf8_string,
            text,
        ) {
            Ok(cookie) => cookie,
            Err(e) => return !connection_error_dead(&e),
        };
        if let Err(ReplyError::ConnectionError(conn)) = cookie.check() {
            return !connection_error_dead(&conn);
        }
        match self.conn.flush() {
            Ok(()) => true,
            Err(e) => !connection_error_dead(&e),
        }
    }

    pub fn size(&self) -> (u16, u16) {
        (self.width, self.height)
    }

    pub fn show_frame(&mut self, bgra: &[u8], width: u16, height: u16) -> Result<()> {
        let need = width as usize * height as usize * 4;
        if bgra.len() < need {
            anyhow::bail!("short bgra: {} < {need}", bgra.len());
        }
        if width != self.width || height != self.height {
            self.conn
                .configure_window(
                    self.window,
                    &ConfigureWindowAux::new()
                        .width(width as u32)
                        .height(height as u32),
                )
                .context("resize window")?
                .check()
                .context("resize check")?;
            self.width = width;
            self.height = height;
        }
        self.conn
            .put_image(
                ImageFormat::Z_PIXMAP,
                self.window,
                self.gc,
                width,
                height,
                0,
                0,
                0,
                24,
                &bgra[..need],
            )
            .context("put image")?
            .check()
            .context("put image check")?;
        self.conn.flush().context("flush")?;
        Ok(())
    }

    pub fn poll_event(&self) -> Result<Option<Event>> {
        self.conn.poll_for_event().context("poll event")
    }

    pub fn keysyms(&mut self, keycode: u8) -> Vec<u32> {
        if let Some(cached) = self.keysym_cache.get(&keycode) {
            return cached.clone();
        }
        let mut fetched = Vec::new();
        if let Ok(cookie) = self.conn.get_keyboard_mapping(keycode, 1) {
            if let Ok(reply) = cookie.reply() {
                fetched = reply.keysyms.to_vec();
            }
        }
        self.keysym_cache.insert(keycode, fetched.clone());
        fetched
    }

    pub fn destroy(self) {
        self.conn.destroy_window(self.window).ok();
        self.conn.flush().ok();
    }

    pub fn screen_size(&self) -> (u16, u16) {
        let setup = self.conn.setup();
        let s = &setup.roots[self.screen];
        (s.width_in_pixels, s.height_in_pixels)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Error as IoError;

    fn broken_pipe() -> anyhow::Error {
        let io = IoError::new(ErrorKind::BrokenPipe, "busted");
        anyhow::Error::from(ConnectionError::from(io)).context("put image check")
    }

    #[test]
    fn broken_pipe_is_dead() {
        assert!(is_connection_dead(&broken_pipe()));
    }

    #[test]
    fn closed_connection_eof_is_dead() {
        let io = IoError::new(ErrorKind::UnexpectedEof, "The X11 server closed the connection");
        let err = anyhow::Error::from(ConnectionError::from(io)).context("poll event");
        assert!(is_connection_dead(&err));
    }

    #[test]
    fn reply_connection_error_is_dead() {
        let io = IoError::new(ErrorKind::ConnectionReset, "reset");
        let conn = ConnectionError::from(io);
        let err = anyhow::Error::from(ReplyError::from(conn)).context("resize check");
        assert!(is_connection_dead(&err));
    }

    #[test]
    fn raw_io_broken_pipe_is_dead() {
        let err: anyhow::Error =
            IoError::new(ErrorKind::BrokenPipe, "write failed").into();
        assert!(is_connection_dead(&err));
    }

    #[test]
    fn x11_parse_error_is_dead() {
        let err = anyhow::Error::from(ConnectionError::from(ParseError::InsufficientData));
        assert!(is_connection_dead(&err));
    }

    #[test]
    fn transient_errors_are_not_dead() {
        let err: anyhow::Error = anyhow::anyhow!("short bgra: 3 < 100");
        assert!(!is_connection_dead(&err));
        let err = anyhow::Error::from(ConnectionError::MaximumRequestLengthExceeded);
        assert!(!is_connection_dead(&err));
        let err: anyhow::Error = IoError::new(ErrorKind::TimedOut, "slow").into();
        assert!(!is_connection_dead(&err));
    }

    #[test]
    fn kvm_grab_roundtrip_releases_display() {
        let grab = match KvmGrab::acquire() {
            Ok(grab) => grab,
            Err(_) => return,
        };
        let (w, h) = grab.screen_size();
        assert!(w > 0 && h > 0);
        assert!(KvmGrab::acquire().is_err());
        grab.release();
        match KvmGrab::acquire() {
            Ok(regrabbed) => regrabbed.release(),
            Err(e) => panic!("regrab after release failed: {e:#}"),
        }
    }
}
