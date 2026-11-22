use anyhow::{Context, Result};
use nb_input::Button;
use x11rb::connection::Connection;
use x11rb::protocol::xfixes::ConnectionExt as _;
use x11rb::protocol::xproto::{ConnectionExt as _, Window};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;

const KEY_PRESS: u8 = 2;
const KEY_RELEASE: u8 = 3;
const BUTTON_PRESS: u8 = 4;
const BUTTON_RELEASE: u8 = 5;

const KEYBOARD_DEVICE: u8 = 1;
const POINTER_DEVICE: u8 = 2;

const XK_RETURN: u32 = 0xff0d;
const XK_TAB: u32 = 0xff09;
const XK_SHIFT_L: u32 = 0xffe1;

pub struct Injector {
    conn: RustConnection,
    root: Window,
    w: i32,
    h: i32,
    max_keycode: u8,
    kpk: u8,
    keysyms: Vec<u32>,
    xfixes: bool,
    hidden: bool,
    last: std::cell::Cell<(i32, i32)>,
    seeded: std::cell::Cell<bool>,
}

impl Injector {
    pub fn connect() -> Result<Injector> {
        let (conn, screen_num) = RustConnection::connect(None).context("X display (DISPLAY set?)")?;
        let screen = &conn.setup().roots[screen_num];
        let root = screen.root;
        let w = screen.width_in_pixels as i32;
        let h = screen.height_in_pixels as i32;

        conn.xtest_get_version(1, 2)
            .context("XTEST extension missing")?
            .reply()
            .context("XTEST extension query")?;

        let max_keycode = conn.setup().max_keycode;
        let (kpk, keysyms) = load_keymap(&conn, max_keycode)?;
        let xfixes = match conn.xfixes_query_version(5, 0) {
            Ok(cookie) => cookie.reply().is_ok(),
            Err(_) => false,
        };

        Ok(Injector {
            conn,
            root,
            w,
            h,
            max_keycode,
            kpk,
            keysyms,
            xfixes,
            hidden: false,
            last: std::cell::Cell::new((0, 0)),
            seeded: std::cell::Cell::new(false),
        })
    }

    pub fn screen(&self) -> (i32, i32) {
        (self.w, self.h)
    }

    pub fn pointer(&self) -> Result<(i32, i32)> {
        if self.seeded.get() {
            return Ok(self.last.get());
        }
        let r = self.conn.query_pointer(self.root)?.reply().context("query pointer")?;
        let p = (r.root_x as i32, r.root_y as i32);
        self.last.set(p);
        self.seeded.set(true);
        Ok(p)
    }

    pub fn warp(&self, x: i32, y: i32) -> Result<()> {
        let x = x.clamp(0, self.w - 1);
        let y = y.clamp(0, self.h - 1);
        self.last.set((x, y));
        self.seeded.set(true);
        self.conn
            .warp_pointer(x11rb::NONE, self.root, 0, 0, 0, 0, x as i16, y as i16)?
            .check()
            .context("warp pointer")
    }

    pub fn button(&self, b: Button, down: bool) -> Result<()> {
        let code = match b {
            Button::Left => 1,
            Button::Middle => 2,
            Button::Right => 3,
        };
        self.fake(if down { BUTTON_PRESS } else { BUTTON_RELEASE }, code, POINTER_DEVICE)
    }

    pub fn tap(&self, b: Button, count: u8) -> Result<()> {
        let code = match b {
            Button::Left => 1,
            Button::Middle => 2,
            Button::Right => 3,
        };
        for i in 0..count.clamp(1, 3) {
            if i > 0 {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            self.fake(BUTTON_PRESS, code, POINTER_DEVICE)?;
            self.fake(BUTTON_RELEASE, code, POINTER_DEVICE)?;
        }
        Ok(())
    }

    pub fn scroll_steps(&self, vertical: i32, horizontal: i32) -> Result<()> {
        let button = |up: bool, horizontal: bool| match (horizontal, up) {
            (false, false) => 5,
            (false, true) => 4,
            (true, false) => 7,
            (true, true) => 6,
        };
        for _ in 0..vertical.abs().min(10) {
            let code = button(vertical < 0, false);
            self.fake(BUTTON_PRESS, code, POINTER_DEVICE)?;
            self.fake(BUTTON_RELEASE, code, POINTER_DEVICE)?;
        }
        for _ in 0..horizontal.abs().min(10) {
            let code = button(horizontal < 0, true);
            self.fake(BUTTON_PRESS, code, POINTER_DEVICE)?;
            self.fake(BUTTON_RELEASE, code, POINTER_DEVICE)?;
        }
        Ok(())
    }

    pub fn key_evdev(&self, code: u16, down: bool) -> Result<()> {
        let keycode = code + 8;
        if keycode > self.max_keycode as u16 {
            return Ok(());
        }
        self.fake(
            if down { KEY_PRESS } else { KEY_RELEASE },
            keycode as u8,
            KEYBOARD_DEVICE,
        )
    }

    pub fn type_text(&mut self, text: &str) -> Result<()> {
        for ch in text.chars() {
            let keysym = match ch {
                '\n' => XK_RETURN,
                '\t' => XK_TAB,
                c if (c as u32) < 0x100 => c as u32,
                c => 0x0100_0000 | (c as u32),
            };
            match self.find_key(keysym) {
                Some((kc, shift)) => self.tap_key(kc, shift)?,
                None => {
                    let Some(kc) = self.free_keycode() else {
                        eprintln!("[input] no free keycode for U+{:04X}", ch as u32);
                        continue;
                    };
                    self.set_key(kc, keysym)?;
                    self.tap_key(kc, false)?;
                    self.set_key(kc, 0)?;
                }
            }
        }
        Ok(())
    }

    pub fn hide_cursor(&mut self) {
        if self.xfixes && !self.hidden {
            let hidden = self
                .conn
                .xfixes_hide_cursor(self.root)
                .ok()
                .and_then(|c| c.check().ok())
                .is_some();
            if hidden {
                self.hidden = true;
            }
        }
    }

    pub fn show_cursor(&mut self) {
        if self.hidden {
            if let Ok(cookie) = self.conn.xfixes_show_cursor(self.root) {
                let _ = cookie.check();
            }
            self.hidden = false;
        }
    }

    fn tap_key(&self, keycode: u8, shift: bool) -> Result<()> {
        let shift_keycode = if shift { self.find_key(XK_SHIFT_L).map(|(kc, _)| kc) } else { None };
        if let Some(sk) = shift_keycode {
            self.fake(KEY_PRESS, sk, KEYBOARD_DEVICE)?;
        }
        self.fake(KEY_PRESS, keycode, KEYBOARD_DEVICE)?;
        self.fake(KEY_RELEASE, keycode, KEYBOARD_DEVICE)?;
        if let Some(sk) = shift_keycode {
            self.fake(KEY_RELEASE, sk, KEYBOARD_DEVICE)?;
        }
        Ok(())
    }

    fn find_key(&self, keysym: u32) -> Option<(u8, bool)> {
        if keysym == 0 {
            return None;
        }
        for kc in 8..=self.max_keycode {
            let base = (kc - 8) as usize * self.kpk as usize;
            for level in 0..(self.kpk as usize).min(2) {
                if self.keysyms.get(base + level).copied() == Some(keysym) {
                    return Some((kc, level == 1));
                }
            }
        }
        None
    }

    fn free_keycode(&self) -> Option<u8> {
        (9..=self.max_keycode).rev().find(|&kc| {
            let base = (kc - 8) as usize * self.kpk as usize;
            (0..self.kpk as usize).all(|l| self.keysyms.get(base + l).copied().unwrap_or(0) == 0)
        })
    }

    fn set_key(&mut self, keycode: u8, keysym: u32) -> Result<()> {
        let mut syms = vec![0u32; self.kpk as usize];
        syms[0] = keysym;
        self.conn
            .change_keyboard_mapping(1, keycode, self.kpk, &syms)?
            .check()
            .context("change keyboard mapping")?;
        let base = (keycode - 8) as usize * self.kpk as usize;
        if base < self.keysyms.len() {
            self.keysyms[base] = keysym;
        }
        Ok(())
    }

    fn fake(&self, event: u8, detail: u8, device: u8) -> Result<()> {
        self.conn
            .xtest_fake_input(event, detail, 0, x11rb::NONE, 0, 0, device)?
            .check()
            .context("XTEST fake input")
    }
}

fn load_keymap(conn: &RustConnection, max_keycode: u8) -> Result<(u8, Vec<u32>)> {
    let count = max_keycode.saturating_sub(7);
    let reply = conn
        .get_keyboard_mapping(8, count)?
        .reply()
        .context("get keyboard mapping")?;
    Ok((reply.keysyms_per_keycode, reply.keysyms))
}
