use anyhow::{Context, Result};
use std::collections::HashMap;
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{
    AtomEnum, ConfigureWindowAux, ConnectionExt, CreateGCAux, CreateWindowAux, EventMask,
    ImageFormat, PropMode, WindowClass,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;
use x11rb::{COPY_DEPTH_FROM_PARENT, COPY_FROM_PARENT};

pub const TITLE: &[u8] = b"Northbridge \xe2\x80\x94 mirror";

pub struct MirrorWindow {
    conn: RustConnection,
    screen: usize,
    window: u32,
    gc: u32,
    width: u16,
    height: u16,
    keysym_cache: HashMap<u8, Vec<u32>>,
}

impl MirrorWindow {
    pub fn connect() -> Result<Self> {
        let (conn, screen) = RustConnection::connect(None).context("X display")?;
        let setup = conn.setup();
        let root = setup.roots[screen].root;
        let black = setup.roots[screen].black_pixel;
        let window: u32 = conn.generate_id().context("window id")?;
        let mask = EventMask::EXPOSURE
            | EventMask::BUTTON_PRESS
            | EventMask::BUTTON_RELEASE
            | EventMask::POINTER_MOTION
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
        conn.create_gc(window, gc, &CreateGCAux::new())
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
        if let Ok(reply) = conn
            .intern_atom(false, b"_NET_WM_NAME")
            .context("intern atom")
            .and_then(|c| c.reply().context("atom reply"))
        {
            if let Ok(utf8) = conn
                .intern_atom(false, b"UTF8_STRING")
                .context("intern utf8")
                .and_then(|c| c.reply().context("utf8 reply"))
            {
                conn.change_property8(
                    PropMode::REPLACE,
                    window,
                    reply.atom,
                    utf8.atom,
                    TITLE,
                )
                .context("net title")
                .and_then(|c| c.check().context("net title check"))
                .ok();
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
        })
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
