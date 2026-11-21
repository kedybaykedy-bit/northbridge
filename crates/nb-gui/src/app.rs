use crate::backend::Backend;
use crate::model::{
    DeviceView, Flow, GuiCmd, InboundPair, LogBuf, OpKind, SelfInfo, UiEvent, age_string, dial_label,
    fp_short, now_secs, primary_channel, LOG_CAP,
};
use crate::theme;
use tokio::sync::mpsc::UnboundedSender;

pub struct GuiApp {
    cmd_tx: UnboundedSender<GuiCmd>,
    events: std::sync::mpsc::Receiver<UiEvent>,
    devices: Vec<DeviceView>,
    selected: Option<String>,
    self_info: Option<SelfInfo>,
    log: LogBuf,
    auto_pair: bool,
    daemon_running: bool,
    discovering: bool,
    pair_flow: Flow,
    ping_flow: Flow,
    send_flow: Flow,
    clip_flow: Flow,
    send_paths: String,
    clip_text: String,
    inbound: Option<InboundPair>,
    started: bool,
}

impl GuiApp {
    pub fn new(cmd_tx: UnboundedSender<GuiCmd>, events: std::sync::mpsc::Receiver<UiEvent>) -> Self {
        Self {
            cmd_tx,
            events,
            devices: Vec::new(),
            selected: None,
            self_info: None,
            log: LogBuf::new(LOG_CAP),
            auto_pair: true,
            daemon_running: false,
            discovering: false,
            pair_flow: Flow::Idle,
            ping_flow: Flow::Idle,
            send_flow: Flow::Idle,
            clip_flow: Flow::Idle,
            send_paths: String::new(),
            clip_text: String::new(),
            inbound: None,
            started: false,
        }
    }

    fn send(&self, cmd: GuiCmd) {
        let _ = self.cmd_tx.send(cmd);
    }

    fn selected_device(&self) -> Option<&DeviceView> {
        let fp = self.selected.as_ref()?;
        crate::model::find_device(&self.devices, fp)
    }

    fn drain_events(&mut self) {
        for _ in 0..200 {
            match self.events.try_recv() {
                Ok(UiEvent::Log(line)) => self.log.push(line),
                Ok(UiEvent::Peers(devices)) => {
                    self.devices = devices;
                    if let Some(fp) = self.selected.clone() {
                        if !self.devices.iter().any(|d| d.fingerprint == fp) {
                            self.selected = None;
                        }
                    }
                }
                Ok(UiEvent::OpResult { op, target, ok, detail }) => {
                    let flow = match op {
                        OpKind::Pair => &mut self.pair_flow,
                        OpKind::Ping => &mut self.ping_flow,
                        OpKind::Send => &mut self.send_flow,
                        OpKind::Clipboard => &mut self.clip_flow,
                    };
                    flow.finish(&target, ok, detail.clone());
                    if !ok {
                        self.log.push(format!("[{}] {target} — {detail}", op.label()));
                    }
                }
                Ok(UiEvent::InboundPair(p)) => self.inbound = Some(p),
                Ok(UiEvent::DaemonStatus { running }) => {
                    self.daemon_running = running;
                    if !running {
                        self.pair_flow.reset();
                        self.ping_flow.reset();
                        self.send_flow.reset();
                        self.clip_flow.reset();
                    }
                }
                Ok(UiEvent::SelfInfo(info)) => self.self_info = Some(info),
                Ok(UiEvent::Discovering { active }) => self.discovering = active,
                Err(_) => break,
            }
        }
    }

    fn masthead(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("masthead").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(theme::title("Northbridge"));
                ui.label(theme::kicker("monokrom manset · sayi 01"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = if self.daemon_running { "STOP" } else { "START" };
                    if ui.button(theme::chip(label)).clicked() {
                        if self.daemon_running {
                            self.send(GuiCmd::StopDaemon);
                        } else {
                            self.send(GuiCmd::StartDaemon);
                        }
                    }
                    let dot = if self.daemon_running { "● LIVE" } else { "○ DOWN" };
                    ui.label(theme::meta(dot));
                    if let Some(info) = self.self_info.clone() {
                        ui.label(theme::meta(&format!(
                            "{} · {} · http :{} pkt :{}",
                            info.alias,
                            fp_short(&info.fingerprint),
                            info.http_port,
                            info.packet_port
                        )));
                    }
                });
            });
            ui.add_space(2.0);
            egui::Frame::new()
                .fill(theme::W)
                .inner_margin(egui::Margin::same(0))
                .show(ui, |ui| {
                    ui.allocate_space(egui::vec2(ui.available_width(), 3.0));
                });
            ui.add_space(2.0);
        });
    }

    fn device_pane(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("devices")
            .resizable(true)
            .default_size(340.0)
            .size_range(egui::Rangef::new(260.0, 520.0))
            .show(ui, |ui| {
                ui.label(theme::kicker("cihazlar · devices"));
                ui.label(theme::title("Cihazlar"));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let label = if self.discovering { "SCANNING…" } else { "DISCOVER NOW" };
                    ui.add_enabled(!self.discovering && self.daemon_running, egui::Button::new(theme::chip(label)))
                        .clicked()
                        .then(|| self.send(GuiCmd::DiscoverNow));
                    if ui.checkbox(&mut self.auto_pair, theme::body("auto-pair")).changed() {
                        let v = self.auto_pair;
                        self.send(GuiCmd::SetAutoPair(v));
                    }
                });
                ui.add_space(6.0);
                if self.devices.is_empty() {
                    theme::band_frame().show(ui, |ui| {
                        ui.label(theme::body("No devices yet."));
                        ui.label(theme::meta("Peers appear here while discovery runs. Start `northbridge listen` on the other side."));
                    });
                }
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut pick: Option<String> = None;
                    for d in self.devices.clone() {
                        let active = self.selected.as_ref() == Some(&d.fingerprint);
                        let row = theme::mini_frame().show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(theme::body(&d.alias));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let tag = if d.trusted { "TRUSTED" } else { "NEW" };
                                    ui.label(theme::chip(tag));
                                });
                            });
                            ui.label(theme::mono(&format!(
                                "{} · {} · {}",
                                fp_short(&d.fingerprint),
                                primary_channel(&d),
                                age_string(now_secs(), crate::model::device_last_seen(&d))
                            )));
                            ui.label(theme::meta(&dial_label(&d)));
                            if d.trusted {
                                if let Some(via) = d.paired_via.clone() {
                                    ui.label(theme::meta(&format!("paired via {via}")));
                                }
                            }
                        });
                        if ui.interact(
                            row.response.rect,
                            egui::Id::unique(("nbdev", &d.fingerprint)),
                            egui::Sense::click(),
                        ).clicked() {
                            pick = Some(d.fingerprint.clone());
                        }
                        if active {
                            ui.add_space(2.0);
                        }
                    }
                    if let Some(fp) = pick {
                        self.selected = Some(fp);
                    }
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let enabled = self.selected.is_some() && self.daemon_running && !self.pair_flow.is_working();
                    if ui.add_enabled(enabled, egui::Button::new(theme::chip("PAIR"))).clicked() {
                        if let Some(fp) = self.selected.clone() {
                            self.pair_flow.start(&fp_short(&fp));
                            self.send(GuiCmd::Pair { fingerprint: fp });
                        }
                    }
                    ui.label(theme::meta(&self.pair_flow.summary()));
                });
            });
    }

    fn action_pane(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.label(theme::kicker("eylemler · actions"));
            ui.label(theme::title("Eylemler"));
            ui.add_space(4.0);
            if let Some(sel) = self.selected_device().cloned() {
                theme::paper_frame().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(theme::body(&sel.alias));
                        ui.label(theme::meta(&format!(
                            "{} · {}",
                            fp_short(&sel.fingerprint),
                            if sel.trusted { "trusted" } else { "not paired" }
                        )));
                    });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let ping_on = self.daemon_running && !self.ping_flow.is_working();
                        if ui.add_enabled(ping_on, egui::Button::new(theme::chip("PING"))).clicked() {
                            self.ping_flow.start(&sel.alias);
                            self.send(GuiCmd::Ping { fingerprint: sel.fingerprint.clone() });
                        }
                        ui.label(theme::meta(&self.ping_flow.summary()));
                    });
                    ui.add_space(4.0);
                    ui.label(theme::kicker("send files"));
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut self.send_paths);
                    });
                    ui.label(theme::meta("Absolute paths, separated by spaces."));
                    ui.horizontal(|ui| {
                        let send_on = self.daemon_running && !self.send_flow.is_working() && !self.send_paths.trim().is_empty();
                        if ui.add_enabled(send_on, egui::Button::new(theme::chip("SEND FILES"))).clicked() {
                            let paths: Vec<String> =
                                self.send_paths.split_whitespace().map(String::from).collect();
                            self.send_flow.start(&sel.alias);
                            self.send(GuiCmd::SendFiles { fingerprint: sel.fingerprint.clone(), paths });
                        }
                        ui.label(theme::meta(&self.send_flow.summary()));
                    });
                    ui.add_space(4.0);
                    ui.label(theme::kicker("push clipboard"));
                    ui.text_edit_multiline(&mut self.clip_text);
                    ui.horizontal(|ui| {
                        let clip_on = self.daemon_running && !self.clip_flow.is_working() && !self.clip_text.is_empty();
                        if ui.add_enabled(clip_on, egui::Button::new(theme::chip("PUSH CLIPBOARD"))).clicked() {
                            self.clip_flow.start(&sel.alias);
                            self.send(GuiCmd::PushClipboard {
                                fingerprint: sel.fingerprint.clone(),
                                text: self.clip_text.clone(),
                            });
                        }
                        ui.label(theme::meta(&self.clip_flow.summary()));
                    });
                });
            } else {
                theme::band_frame().show(ui, |ui| {
                    ui.label(theme::body("Select a device on the left to act on it."));
                });
            }
            ui.add_space(8.0);
            if let Some(p) = self.inbound.clone() {
                theme::mini_frame().show(ui, |ui| {
                    ui.label(theme::kicker("last pairing request"));
                    let verdict = if p.accepted { "ACCEPTED" } else { "REJECTED" };
                    ui.label(theme::body(&format!("{} ({}) — {}", p.alias, fp_short(&p.fingerprint), verdict)));
                    if let Some(pin) = p.pin {
                        ui.label(theme::mono(&format!("PIN {pin} — confirm it matches the phone screen")));
                    }
                    ui.label(theme::meta(&format!("via {}", p.channel)));
                });
            }
            ui.add_space(8.0);
            theme::band_frame().show(ui, |ui| {
                ui.label(theme::kicker("services"));
                ui.label(theme::body("Embedded in this window: packet server · file receive · clipboard sync · discovery."));
                ui.label(theme::meta("Screen / audio / mirror streaming stay in `northbridge listen` (CLI) for now."));
            });
        });
    }

    fn log_pane(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("log")
            .resizable(true)
            .default_size(190.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(theme::kicker(&format!("kayit · log · {}", self.log.len())));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_enabled(!self.log.is_empty(), egui::Button::new(theme::chip("CLEAR"))).clicked() {
                            self.log.clear();
                        }
                    });
                });
                egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                    for line in self.log.iter() {
                        ui.label(theme::mono(line));
                    }
                });
            });
    }
}

impl eframe::App for GuiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if !self.started {
            self.started = true;
            self.log.push("northbridge gui starting…".to_string());
        }
        self.drain_events();
        self.masthead(ui);
        self.device_pane(ui);
        self.log_pane(ui);
        self.action_pane(ui);
        ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
    }
}

pub fn run_app(backend: Backend, events: std::sync::mpsc::Receiver<UiEvent>) -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 780.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Northbridge",
        options,
        Box::new(move |cc| {
            theme::apply(&cc.egui_ctx);
            Ok(Box::new(GuiApp::new(backend.cmd_tx, events)))
        }),
    )
}
