use crate::x11::Injector;
use nb_core::packet::NetworkPacket;
use nb_core::plugin::{Plugin, PluginCtx};
use nb_input::{Button, InputMsg};
use nb_transport::InputRegistry;
use serde_json::json;
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const KIND_IN: &str = "nb.touchpad";
pub const KIND_OUT: &str = "nb.kvm";

const WATCH_INTERVAL: Duration = Duration::from_millis(12);

pub struct Capture {
    fp: String,
    anchor: (i32, i32),
    vpos: (f64, f64),
}

pub struct InputState {
    injector: Option<Mutex<Injector>>,
    typing: Option<std_mpsc::Sender<String>>,
    capture: Option<Capture>,
    scroll_acc: (f64, f64),
    screen: (i32, i32),
}

impl InputState {
    pub fn new() -> InputState {
        InputState {
            injector: None,
            typing: None,
            capture: None,
            scroll_acc: (0.0, 0.0),
            screen: (0, 0),
        }
    }

    pub fn with_display() -> InputState {
        match Injector::connect() {
            Ok(inj) => {
                let screen = inj.screen();
                let (typing, rx) = std_mpsc::channel::<String>();
                let typer_conn = Injector::connect();
                std::thread::spawn(move || {
                    let Ok(mut typer) = typer_conn else {
                        eprintln!("[input] typing thread: second X connection failed");
                        return;
                    };
                    while let Ok(text) = rx.recv() {
                        if let Err(e) = typer.type_text(&text) {
                            eprintln!("[input] typing failed: {e:#}");
                        }
                    }
                });
                println!("[input] X11 ready — {}x{}", screen.0, screen.1);
                InputState {
                    injector: Some(Mutex::new(inj)),
                    typing: Some(typing),
                    capture: None,
                    scroll_acc: (0.0, 0.0),
                    screen,
                }
            }
            Err(e) => {
                eprintln!("[input] touchpad disabled: {e:#}");
                InputState::new()
            }
        }
    }
}

pub struct TouchpadPlugin {
    state: Arc<Mutex<InputState>>,
    registry: InputRegistry,
}

impl TouchpadPlugin {
    pub fn new(state: Arc<Mutex<InputState>>, registry: InputRegistry) -> Self {
        TouchpadPlugin { state, registry }
    }
}

impl Plugin for TouchpadPlugin {
    fn id(&self) -> &str {
        "nb.touchpad"
    }

    fn incoming(&self) -> &[&str] {
        &[KIND_IN]
    }

    fn outgoing(&self) -> &[&str] {
        &[KIND_OUT]
    }

    fn on_packet(&mut self, ctx: &PluginCtx, pkt: &NetworkPacket) -> Option<NetworkPacket> {
        let msg: InputMsg = serde_json::from_value(pkt.body.clone()).ok()?;
        handle(&self.state, &self.registry, &ctx.peer_fingerprint, msg)
    }
}

fn handle(
    state: &Arc<Mutex<InputState>>,
    registry: &InputRegistry,
    peer_fp: &str,
    msg: InputMsg,
) -> Option<NetworkPacket> {
    match msg {
        InputMsg::Ping => Some(NetworkPacket::new(KIND_IN, json!({"t": "pong"}))),
        InputMsg::Pong => None,
        InputMsg::Capture => {
            start_capture(state, registry, peer_fp);
            None
        }
        InputMsg::Release => {
            stop_capture(state, registry, peer_fp);
            None
        }
        InputMsg::Type { s } => {
            let st = state.lock().unwrap();
            if let Some(tx) = &st.typing {
                let _ = tx.send(s);
            } else {
                eprintln!("[input] type ignored — no X display");
            }
            None
        }
        InputMsg::Key { code, down } => {
            let st = state.lock().unwrap();
            if let Some(inj) = &st.injector {
                if let Err(e) = inj.lock().unwrap().key_evdev(code, down) {
                    eprintln!("[input] key {code} failed: {e:#}");
                }
            }
            None
        }
        InputMsg::RelMove { .. } | InputMsg::ClickAbs { .. } => {
            let mut st = state.lock().unwrap();
            if st.capture.is_some() {
                return None;
            }
            apply(&mut st, msg);
            None
        }
        InputMsg::Tap { .. } | InputMsg::Button { .. } | InputMsg::Scroll { .. } => {
            let mut st = state.lock().unwrap();
            apply(&mut st, msg);
            None
        }
        InputMsg::Enter { .. } | InputMsg::Leave | InputMsg::Move { .. } => None,
    }
}

fn apply(st: &mut InputState, msg: InputMsg) {
    let InputState {
        injector,
        scroll_acc,
        ..
    } = st;
    let Some(injector) = injector else { return };
    let Ok(inj) = injector.lock() else { return };
    let outcome = match msg {
        InputMsg::RelMove { dx, dy } => inj
            .pointer()
            .and_then(|(x, y)| inj.warp(x + dx as i32, y + dy as i32)),
        InputMsg::Tap { b, count } => inj.tap(b, count),
        InputMsg::Button { b, down } => inj.button(b, down),
        InputMsg::Scroll { dx, dy } => {
            scroll_acc.0 += dx;
            scroll_acc.1 += dy;
            let steps_x = scroll_acc.0.trunc() as i32;
            let steps_y = scroll_acc.1.trunc() as i32;
            scroll_acc.0 -= steps_x as f64;
            scroll_acc.1 -= steps_y as f64;
            inj.scroll_steps(steps_y, steps_x)
        }
        InputMsg::ClickAbs { x, y } => {
            let (w, h) = inj.screen();
            let px = (x.clamp(0.0, 1.0) * (w - 1) as f64) as i32;
            let py = (y.clamp(0.0, 1.0) * (h - 1) as f64) as i32;
            inj.warp(px, py).and_then(|_| inj.tap(Button::Left, 1))
        }
        _ => Ok(()),
    };
    if let Err(e) = outcome {
        eprintln!("[input] injection failed: {e:#}");
    }
}

fn start_capture(state: &Arc<Mutex<InputState>>, registry: &InputRegistry, peer_fp: &str) {
    let mut st = state.lock().unwrap();
    if st.capture.is_some() || st.injector.is_none() {
        if st.injector.is_none() {
            eprintln!("[input] capture ignored — no X display");
        }
        return;
    }
    let (inj, screen) = match (&st.injector, st.screen) {
        (Some(inj), (w, h)) if w > 0 && h > 0 => (inj, (w, h)),
        _ => {
            eprintln!("[input] capture ignored — no X display");
            return;
        }
    };
    let anchor = {
        let Ok(inj) = inj.lock() else { return };
        match inj.pointer() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("[input] pointer query failed: {e:#}");
                return;
            }
        }
    };
    let ny = anchor.1 as f64 / screen.1 as f64;
    let enter = NetworkPacket::new(
        KIND_OUT,
        serde_json::to_value(&InputMsg::Enter { x: 0.0, y: ny }).expect("serializable"),
    );
    if !registry.send(peer_fp, enter) {
        eprintln!("[input] capture: peer has no input stream");
        return;
    }
    if let Ok(mut i) = inj.lock() {
        i.hide_cursor();
    }
    st.capture = Some(Capture {
        fp: peer_fp.to_string(),
        anchor,
        vpos: (0.0, ny),
    });
    drop(st);
    spawn_watcher(Arc::clone(state), registry.clone());
}

fn stop_capture(state: &Arc<Mutex<InputState>>, registry: &InputRegistry, peer_fp: &str) {
    let mut st = state.lock().unwrap();
    let Some(cap) = st.capture.take() else { return };
    if cap.fp != peer_fp {
        st.capture = Some(cap);
        return;
    }
    let _ = registry.send(peer_fp, NetworkPacket::new(KIND_OUT, json!({"t": "leave"})));
    if let Some(inj) = &st.injector {
        if let Ok(mut i) = inj.lock() {
            i.show_cursor();
        }
    }
}

fn spawn_watcher(state: Arc<Mutex<InputState>>, registry: InputRegistry) {
    enum Tick {
        Idle,
        Moved,
        Lost(String),
    }

    std::thread::spawn(move || loop {
        std::thread::sleep(WATCH_INTERVAL);
        let mut st = state.lock().unwrap();
        let Some(cap) = st.capture.as_ref() else { break };
        let fp = cap.fp.clone();
        let anchor = cap.anchor;
        let mut vpos = cap.vpos;
        let screen = st.screen;

        let tick = {
            let Some(inj) = &st.injector else { break };
            let Ok(inj) = inj.lock() else { break };
            match inj.pointer() {
                Ok(p) if p != anchor => {
                    let dx = (p.0 - anchor.0) as f64;
                    let dy = (p.1 - anchor.1) as f64;
                    vpos.0 = (vpos.0 + dx / screen.0 as f64).clamp(0.0, 1.0);
                    vpos.1 = (vpos.1 + dy / screen.1 as f64).clamp(0.0, 1.0);
                    let _ = inj.warp(anchor.0, anchor.1);
                    Tick::Moved
                }
                Ok(_) => Tick::Idle,
                Err(e) => Tick::Lost(format!("{e:#}")),
            }
        };

        match tick {
            Tick::Idle => continue,
            Tick::Lost(err) => {
                eprintln!("[input] capture pointer lost: {err}");
                st.capture = None;
                show_cursor(&st);
                break;
            }
            Tick::Moved => {}
        }

        let pkt = NetworkPacket::new(
            KIND_OUT,
            serde_json::to_value(&InputMsg::Move {
                x: vpos.0,
                y: vpos.1,
            })
            .expect("serializable"),
        );
        if !registry.send(&fp, pkt) {
            st.capture = None;
            show_cursor(&st);
            break;
        }
        if let Some(cap) = st.capture.as_mut() {
            cap.vpos = vpos;
        }
    });
}

fn show_cursor(st: &InputState) {
    if let Some(inj) = &st.injector {
        if let Ok(mut i) = inj.lock() {
            i.show_cursor();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> Arc<Mutex<InputState>> {
        Arc::new(Mutex::new(InputState::new()))
    }

    #[test]
    fn ping_gets_pong() {
        let st = state();
        let reg = InputRegistry::new();
        let resp = handle(&st, &reg, "FP", InputMsg::Ping).expect("pong");
        assert_eq!(resp.kind, KIND_IN);
        assert_eq!(resp.body["t"], "pong");
    }

    #[test]
    fn capture_without_display_is_ignored() {
        let st = state();
        let reg = InputRegistry::new();
        assert!(handle(&st, &reg, "FP", InputMsg::Capture).is_none());
        assert!(st.lock().unwrap().capture.is_none());
    }

    #[test]
    fn input_kinds_declare_capabilities() {
        let st = state();
        let p = TouchpadPlugin::new(st, InputRegistry::new());
        assert_eq!(p.incoming(), &[KIND_IN]);
        assert_eq!(p.outgoing(), &[KIND_OUT]);
        assert_eq!(p.id(), "nb.touchpad");
    }

    #[test]
    fn entry_direction_messages_from_phone_are_ignored() {
        let st = state();
        let reg = InputRegistry::new();
        for msg in [
            InputMsg::Leave,
            InputMsg::Enter { x: 0.5, y: 0.5 },
            InputMsg::Move { x: 0.5, y: 0.5 },
            InputMsg::Pong,
        ] {
            assert!(handle(&st, &reg, "FP", msg).is_none());
        }
    }

    #[test]
    fn malformed_body_yields_no_response() {
        let st = state();
        let mut plugin = TouchpadPlugin::new(st, InputRegistry::new());
        let ctx = PluginCtx {
            own_alias: "me".into(),
            own_fingerprint: "F".into(),
            peer_fingerprint: "P".into(),
            peer_alias: "peer".into(),
            channel: nb_core::channel::Channel::Lan,
        };
        let pkt = NetworkPacket::new(KIND_IN, json!({"t": "nope"}));
        assert!(plugin.on_packet(&ctx, &pkt).is_none());
    }
}
