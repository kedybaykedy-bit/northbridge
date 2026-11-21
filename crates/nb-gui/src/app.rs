use crate::backend::Backend;
use crate::model::{
    AppEntry, DeviceView, Flow, GuiCmd, InboundPair, LogBuf, OpKind, SelfInfo, UiEvent, age_string,
    dial_label, filter_apps, fp_short, now_secs, primary_channel, LOG_CAP,
};
use crate::theme;
use tokio::sync::mpsc::UnboundedSender;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Tab {
    Pair,
    Ping,
    Files,
    Clip,
    Remote,
    Apps,
}

struct TileDef {
    tab: Tab,
    kicker: String,
    title: String,
    hint: String,
    no: String,
    enabled: bool,
}

fn tile_pressed(ui: &mut egui::Ui, resp: &egui::Response) -> bool {
    if resp.clicked() {
        return true;
    }
    if resp.has_focus() {
        return ui.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Space));
    }
    false
}

fn nonzero(summary: String, fallback: &str) -> String {
    if summary.trim().is_empty() {
        fallback.to_string()
    } else {
        summary
    }
}

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
    wake_flow: Flow,
    open_flow: Flow,
    apps_flow: Flow,
    sleep_flow: Flow,
    keep_flow: Flow,
    keep_on: bool,
    send_paths: String,
    clip_text: String,
    apps: Vec<AppEntry>,
    apps_total: u64,
    apps_truncated: bool,
    apps_target: Option<String>,
    apps_filter: String,
    selected_app: Option<String>,
    inbound: Option<InboundPair>,
    started: bool,
    active_tab: Tab,
    last_transfer: Option<String>,
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
            wake_flow: Flow::Idle,
            open_flow: Flow::Idle,
            apps_flow: Flow::Idle,
            sleep_flow: Flow::Idle,
            keep_flow: Flow::Idle,
            keep_on: false,
            send_paths: String::new(),
            clip_text: String::new(),
            apps: Vec::new(),
            apps_total: 0,
            apps_truncated: false,
            apps_target: None,
            apps_filter: String::new(),
            selected_app: None,
            inbound: None,
            started: false,
            active_tab: Tab::Ping,
            last_transfer: None,
        }
    }

    fn send(&self, cmd: GuiCmd) {
        let _ = self.cmd_tx.send(cmd);
    }

    fn selected_device(&self) -> Option<&DeviceView> {
        let fp = self.selected.as_ref()?;
        crate::model::find_device(&self.devices, fp)
    }

    fn clear_device_state(&mut self) {
        self.apps.clear();
        self.apps_total = 0;
        self.apps_truncated = false;
        self.apps_target = None;
        self.selected_app = None;
        self.apps_filter.clear();
        self.wake_flow.reset();
        self.open_flow.reset();
        self.apps_flow.reset();
        self.sleep_flow.reset();
        self.keep_flow.reset();
        self.keep_on = false;
    }

    fn drain_events(&mut self) {
        for _ in 0..300 {
            match self.events.try_recv() {
                Ok(UiEvent::Log(line)) => {
                    if line.contains("[transfer]") {
                        self.last_transfer = Some(line.clone());
                    }
                    self.log.push(line);
                }
                Ok(UiEvent::Peers(devices)) => {
                    self.devices = devices;
                    if let Some(fp) = self.selected.clone() {
                        if !self.devices.iter().any(|d| d.fingerprint == fp) {
                            self.selected = None;
                            self.clear_device_state();
                        }
                    }
                }
                Ok(UiEvent::OpResult { op, target, ok, detail }) => {
                    let flow = match op {
                        OpKind::Pair => &mut self.pair_flow,
                        OpKind::Ping => &mut self.ping_flow,
                        OpKind::Send => &mut self.send_flow,
                        OpKind::Clipboard => &mut self.clip_flow,
                        OpKind::Wake => &mut self.wake_flow,
                        OpKind::Open => &mut self.open_flow,
                        OpKind::Apps => &mut self.apps_flow,
                        OpKind::Sleep => &mut self.sleep_flow,
                        OpKind::KeepAwake => &mut self.keep_flow,
                    };
                    flow.finish(&target, ok, detail.clone());
                    if !ok {
                        self.log.push(format!("[{}] {target} — {detail}", op.label()));
                    }
                    if op == OpKind::KeepAwake && !ok {
                        self.keep_on = false;
                    }
                }
                Ok(UiEvent::AppsList {
                    target,
                    ok,
                    detail,
                    apps,
                    total,
                    truncated,
                }) => {
                    self.apps = apps;
                    self.apps_total = total;
                    self.apps_truncated = truncated;
                    self.apps_target = Some(target);
                    self.selected_app = None;
                    if !ok {
                        self.log.push(format!("[APPS] {detail}"));
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
                        self.wake_flow.reset();
                        self.open_flow.reset();
                        self.apps_flow.reset();
                        self.sleep_flow.reset();
                        self.keep_flow.reset();
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
                ui.vertical(|ui| {
                    ui.label(theme::kicker("monokrom manset · sayi 01"));
                    ui.label(
                        egui::RichText::new("NORTHBRIDGE")
                            .italics()
                            .size(30.0)
                            .color(theme::W)
                            .strong(),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = if self.daemon_running { "STOP" } else { "START" };
                    if ui.button(theme::chip(label)).clicked() {
                        if self.daemon_running {
                            self.send(GuiCmd::StopDaemon);
                        } else {
                            self.send(GuiCmd::StartDaemon);
                        }
                    }
                    let dot = if self.daemon_running { "CANLI" } else { "DURDU" };
                    ui.label(theme::kicker(dot));
                    if self.discovering {
                        ui.label(theme::meta("TARIYOR…"));
                    }
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
            ui.add_space(4.0);
            theme::double_rule(ui);
            ui.add_space(2.0);
            theme::tram_band(ui, 14.0);
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
                    let label = if self.discovering { "TARIYOR…" } else { "KESFET" };
                    ui.add_enabled(
                        !self.discovering && self.daemon_running,
                        egui::Button::new(theme::chip(label)),
                    )
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
                        ui.label(theme::body("Henuz cihaz yok."));
                        ui.label(theme::meta("Kesif calisirken esler burada listelenir."));
                    });
                }
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut pick: Option<String> = None;
                    for d in self.devices.clone() {
                        let active = self.selected.as_ref() == Some(&d.fingerprint);
                        let row = theme::mini_frame().show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(theme::body(&d.alias));
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let tag = if d.trusted { "TRUSTED" } else { "NEW" };
                                        ui.label(theme::chip(tag));
                                    },
                                );
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
                            if active {
                                ui.label(theme::kicker("SECILI"));
                            }
                        });
                        if ui
                            .interact(
                                row.response.rect,
                                egui::Id::new(("nbdev", d.fingerprint.clone())),
                                egui::Sense::click(),
                            )
                            .clicked()
                        {
                            pick = Some(d.fingerprint.clone());
                        }
                    }
                    if let Some(fp) = pick {
                        if self.selected.as_ref() != Some(&fp) {
                            self.selected = Some(fp);
                            self.clear_device_state();
                        }
                    }
                });
                ui.add_space(6.0);
                if let Some(sel) = self.selected_device().cloned() {
                    theme::band_frame().show(ui, |ui| {
                        ui.label(theme::kicker("oturum · packet"));
                        for c in &sel.channels {
                            let age = if c.last_seen == 0 {
                                "—".to_string()
                            } else {
                                age_string(now_secs(), c.last_seen)
                            };
                            ui.label(theme::mono(&format!(
                                "{} {}:{} · {}",
                                c.channel, c.host, c.http_port, age
                            )));
                        }
                        ui.label(theme::meta(&format!("kanal: {}", primary_channel(&sel))));
                    });
                } else {
                    ui.label(theme::meta("Oturum icin soldan bir cihaz sec."));
                }
            });
    }

    fn status_strip(&self, ui: &mut egui::Ui) {
        let avail = ui.available_width();
        let n = if avail > 620.0 { 4 } else { 2 };
        let daemon = if self.daemon_running { "CANLI" } else { "DURDU" };
        let daemon_note = if self.discovering {
            "kesif TARIYOR"
        } else {
            "gomulu sunucu"
        };
        let (pkt, http) = match &self.self_info {
            Some(i) => (format!(":{}", i.packet_port), format!(":{}", i.http_port)),
            None => ("—".to_string(), "—".to_string()),
        };
        let trusted = self.devices.iter().filter(|d| d.trusted).count();
        let cells: [(String, String, String); 4] = [
            ("DAGITIM".to_string(), daemon.to_string(), daemon_note.to_string()),
            ("PAKET".to_string(), pkt, "nb.packet · tcp".to_string()),
            ("HTTP".to_string(), http, "localsend · alici".to_string()),
            (
                "AG".to_string(),
                format!("{} CIHAZ", self.devices.len()),
                format!("{trusted} guvenilir"),
            ),
        ];
        egui::Grid::new("nb-stats")
            .num_columns(n)
            .spacing(egui::vec2(10.0, 10.0))
            .show(ui, |ui| {
                for (i, (k, v, m)) in cells.iter().enumerate() {
                    theme::mini_frame().show(ui, |ui| {
                        ui.set_min_width(120.0);
                        ui.label(theme::kicker(k));
                        ui.label(
                            egui::RichText::new(v).size(18.0).color(theme::W).strong(),
                        );
                        ui.label(theme::meta(m));
                    });
                    if (i + 1) % n == 0 {
                        ui.end_row();
                    }
                }
            });
    }

    fn tile_specs(&self) -> Vec<TileDef> {
        let sel = self.selected_device();
        let dev_ok = self.daemon_running && sel.is_some();
        let trusted = sel.map(|d| d.trusted).unwrap_or(false);
        vec![
            TileDef {
                tab: Tab::Pair,
                kicker: "ESLESTIRME".to_string(),
                title: "ESLE".to_string(),
                hint: nonzero(self.pair_flow.summary(), "guven · pair"),
                no: "01".to_string(),
                enabled: dev_ok,
            },
            TileDef {
                tab: Tab::Ping,
                kicker: "BAGLANTI".to_string(),
                title: "PING".to_string(),
                hint: nonzero(self.ping_flow.summary(), "pong · rtt"),
                no: "02".to_string(),
                enabled: dev_ok,
            },
            TileDef {
                tab: Tab::Files,
                kicker: "AKTARIM".to_string(),
                title: "DOSYA".to_string(),
                hint: nonzero(self.send_flow.summary(), "gonder · al"),
                no: "03".to_string(),
                enabled: dev_ok,
            },
            TileDef {
                tab: Tab::Clip,
                kicker: "PANO".to_string(),
                title: "PANO".to_string(),
                hint: nonzero(self.clip_flow.summary(), "it · al"),
                no: "04".to_string(),
                enabled: dev_ok,
            },
            TileDef {
                tab: Tab::Remote,
                kicker: "UZAKTAN".to_string(),
                title: "UYANDIR".to_string(),
                hint: nonzero(self.wake_flow.summary(), "uyu · uyan · tut"),
                no: "05".to_string(),
                enabled: dev_ok && trusted,
            },
            TileDef {
                tab: Tab::Apps,
                kicker: "UYGULAMA".to_string(),
                title: "UYGULAMA".to_string(),
                hint: nonzero(self.apps_flow.summary(), "ac · liste"),
                no: "06".to_string(),
                enabled: dev_ok && trusted,
            },
        ]
    }

    fn tile_grid(&mut self, ui: &mut egui::Ui) {
        let tiles = self.tile_specs();
        let avail = ui.available_width();
        let cols = theme::tile_columns(avail, theme::TILE_MIN, theme::TILE_GAP);
        let tw = theme::tile_width(avail, cols, theme::TILE_GAP);
        let size = egui::vec2(tw, theme::TILE_H);
        let mut idx = 0;
        while idx < tiles.len() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme::TILE_GAP;
                for t in tiles.iter().skip(idx).take(cols) {
                    let active = self.active_tab == t.tab;
                    let resp = theme::skewed_tile(
                        ui,
                        ("nbtile", t.no.clone()),
                        theme::TileSpec {
                            kicker: &t.kicker,
                            title: &t.title,
                            hint: &t.hint,
                            no: &t.no,
                        },
                        size,
                        active,
                        t.enabled,
                    );
                    if t.enabled && tile_pressed(ui, &resp) {
                        self.active_tab = t.tab;
                    }
                }
            });
            ui.add_space(theme::TILE_GAP);
            idx += cols;
        }
    }

    fn detail_pair(&mut self, ui: &mut egui::Ui, sel: DeviceView) {
        ui.label(theme::kicker("eslestirme · pair"));
        ui.label(theme::body(&format!(
            "{} · {}",
            sel.alias,
            fp_short(&sel.fingerprint)
        )));
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let enabled = self.daemon_running && !self.pair_flow.is_working();
            if ui
                .add_enabled(enabled, egui::Button::new(theme::chip("ESLE")))
                .clicked()
            {
                self.pair_flow.start(&fp_short(&sel.fingerprint));
                self.send(GuiCmd::Pair {
                    fingerprint: sel.fingerprint.clone(),
                });
            }
            ui.label(theme::meta(&self.pair_flow.summary()));
        });
        ui.label(theme::meta("Guven cihaza yazilir; reddedilirse loga duser."));
    }

    fn detail_ping(&mut self, ui: &mut egui::Ui, sel: DeviceView) {
        ui.label(theme::kicker("baglanti · ping"));
        ui.label(theme::body(&format!(
            "{} · {}",
            sel.alias,
            fp_short(&sel.fingerprint)
        )));
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let enabled = self.daemon_running && !self.ping_flow.is_working();
            if ui
                .add_enabled(enabled, egui::Button::new(theme::chip("PING")))
                .clicked()
            {
                self.ping_flow.start(&sel.alias);
                self.send(GuiCmd::Ping {
                    fingerprint: sel.fingerprint.clone(),
                });
            }
            ui.label(theme::meta(&self.ping_flow.summary()));
        });
        ui.label(theme::meta("Paket kanalindan pong bekler; sure loga yazilir."));
    }

    fn detail_files(&mut self, ui: &mut egui::Ui, sel: DeviceView) {
        ui.label(theme::kicker("dosya gonder · send files"));
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.send_paths);
        });
        ui.label(theme::meta("Mutlak yollar, boslukla ayrilir."));
        ui.horizontal(|ui| {
            let enabled = self.daemon_running
                && !self.send_flow.is_working()
                && !self.send_paths.trim().is_empty();
            if ui
                .add_enabled(enabled, egui::Button::new(theme::chip("GONDER")))
                .clicked()
            {
                let paths: Vec<String> =
                    self.send_paths.split_whitespace().map(String::from).collect();
                self.send_flow.start(&sel.alias);
                self.send(GuiCmd::SendFiles {
                    fingerprint: sel.fingerprint.clone(),
                    paths,
                });
            }
            ui.label(theme::meta(&self.send_flow.summary()));
        });
        if self.send_flow.is_working() {
            ui.add_space(4.0);
            theme::paint_transfer_bar(ui, ui.ctx().time());
            ui.label(theme::meta("AKTARIM SURUYOR…"));
        }
        if let Some(t) = self.last_transfer.clone() {
            ui.label(theme::mono(&t));
        } else {
            ui.label(theme::meta("Henuz aktarim kaydi yok."));
        }
        ui.label(theme::meta("Canli yuzde arka uctan gelmiyor; ilerleme cizgisi calisirken doner."));
    }

    fn detail_clip(&mut self, ui: &mut egui::Ui, sel: DeviceView) {
        ui.label(theme::kicker("pano it · push clipboard"));
        ui.text_edit_multiline(&mut self.clip_text);
        ui.horizontal(|ui| {
            let enabled =
                self.daemon_running && !self.clip_flow.is_working() && !self.clip_text.is_empty();
            if ui
                .add_enabled(enabled, egui::Button::new(theme::chip("PANOYA IT")))
                .clicked()
            {
                self.clip_flow.start(&sel.alias);
                self.send(GuiCmd::PushClipboard {
                    fingerprint: sel.fingerprint.clone(),
                    text: self.clip_text.clone(),
                });
            }
            ui.label(theme::meta(&self.clip_flow.summary()));
        });
        ui.label(theme::meta("Yerel degisiklikler esli guvenilir cihazlara otomatik itilir."));
    }

    fn detail_remote(&mut self, ui: &mut egui::Ui, sel: DeviceView) {
        ui.label(theme::kicker("uzaktan kumanda · remote"));
        ui.horizontal(|ui| {
            let wake_on = self.daemon_running && sel.trusted && !self.wake_flow.is_working();
            if ui
                .add_enabled(wake_on, egui::Button::new(theme::chip("UYANDIR")))
                .clicked()
            {
                self.wake_flow.start(&sel.alias);
                self.send(GuiCmd::Wake {
                    fingerprint: sel.fingerprint.clone(),
                });
            }
            let sleep_on = self.daemon_running && sel.trusted && !self.sleep_flow.is_working();
            if ui
                .add_enabled(sleep_on, egui::Button::new(theme::chip("UYUT")))
                .clicked()
            {
                self.sleep_flow.start(&sel.alias);
                self.send(GuiCmd::Sleep {
                    fingerprint: sel.fingerprint.clone(),
                });
            }
        });
        ui.label(theme::meta(&self.wake_flow.summary()));
        ui.label(theme::meta(&self.sleep_flow.summary()));
        ui.horizontal(|ui| {
            let mut next = self.keep_on;
            let keep_enabled = self.daemon_running && sel.trusted && !self.keep_flow.is_working();
            ui.add_enabled(
                keep_enabled,
                egui::Checkbox::new(&mut next, theme::body("EKRANI ACIK TUT")),
            );
            if next != self.keep_on && keep_enabled {
                self.keep_on = next;
                self.keep_flow.start(&sel.alias);
                self.send(GuiCmd::KeepAwake {
                    fingerprint: sel.fingerprint.clone(),
                    on: next,
                });
            }
            ui.label(theme::meta(&self.keep_flow.summary()));
        });
        if !sel.trusted {
            ui.label(theme::meta("Once eslestir — uzaktan kumanda guven ister."));
        }
    }

    fn detail_apps(&mut self, ui: &mut egui::Ui, sel: DeviceView) {
        ui.label(theme::kicker("uygulama · apps"));
        ui.horizontal(|ui| {
            let fetch_on = self.daemon_running && sel.trusted && !self.apps_flow.is_working();
            if ui
                .add_enabled(fetch_on, egui::Button::new(theme::chip("LISTEYI GETIR")))
                .clicked()
            {
                self.apps_flow.start(&sel.alias);
                self.send(GuiCmd::FetchApps {
                    fingerprint: sel.fingerprint.clone(),
                });
            }
            ui.label(theme::meta(&self.apps_flow.summary()));
        });
        if let Some(target) = self.apps_target.clone() {
            let mut head = format!("{} uygulama", self.apps.len());
            if self.apps_truncated {
                head = format!("{} (toplam {} — daraltilmis)", self.apps.len(), self.apps_total);
            } else if self.apps_total as usize != self.apps.len() {
                head = format!("{} (toplam {})", self.apps.len(), self.apps_total);
            }
            ui.label(theme::meta(&format!("{target} · {head}")));
        }
        ui.horizontal(|ui| {
            ui.label(theme::meta("filtre"));
            ui.text_edit_singleline(&mut self.apps_filter);
        });
        let shown: Vec<AppEntry> = filter_apps(&self.apps, &self.apps_filter)
            .into_iter()
            .cloned()
            .collect();
        if self.apps.is_empty() {
            ui.label(theme::meta("Liste bos — LISTEYI GETIR ile doldur."));
        } else if shown.is_empty() {
            ui.label(theme::meta("Filtreye uyan uygulama yok."));
        } else {
            egui::ScrollArea::vertical()
                .max_height(220.0)
                .show(ui, |ui| {
                    let mut pick: Option<String> = None;
                    for entry in shown {
                        let active = self.selected_app.as_ref() == Some(&entry.pkg);
                        let row = theme::mini_frame().show(ui, |ui| {
                            ui.label(theme::body(&entry.label));
                            ui.label(theme::mono(&entry.pkg));
                        });
                        if ui
                            .interact(
                                row.response.rect,
                                egui::Id::new(("nbapp", entry.pkg.clone())),
                                egui::Sense::click(),
                            )
                            .clicked()
                        {
                            pick = Some(entry.pkg.clone());
                        }
                        if active {
                            ui.label(theme::meta("secili"));
                        }
                    }
                    if let Some(pkg) = pick {
                        self.selected_app = Some(pkg);
                    }
                });
            ui.horizontal(|ui| {
                let open_on = self.daemon_running
                    && sel.trusted
                    && self.selected_app.is_some()
                    && !self.open_flow.is_working();
                if ui
                    .add_enabled(open_on, egui::Button::new(theme::chip("AC")))
                    .clicked()
                {
                    if let Some(pkg) = self.selected_app.clone() {
                        self.open_flow.start(&pkg);
                        self.send(GuiCmd::Open {
                            fingerprint: sel.fingerprint.clone(),
                            package: pkg,
                        });
                    }
                }
                if let Some(pkg) = self.selected_app.clone() {
                    ui.label(theme::mono(&pkg));
                }
                ui.label(theme::meta(&self.open_flow.summary()));
            });
        }
    }

    fn detail_card(&mut self, ui: &mut egui::Ui) {
        theme::paper_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            if let Some(sel) = self.selected_device().cloned() {
                ui.horizontal(|ui| {
                    ui.label(theme::body(&sel.alias));
                    ui.label(theme::meta(&format!(
                        "{} · {}",
                        fp_short(&sel.fingerprint),
                        if sel.trusted { "guvenilir" } else { "eslesmedi" }
                    )));
                });
                ui.add_space(4.0);
                match self.active_tab {
                    Tab::Pair => self.detail_pair(ui, sel),
                    Tab::Ping => self.detail_ping(ui, sel),
                    Tab::Files => self.detail_files(ui, sel),
                    Tab::Clip => self.detail_clip(ui, sel),
                    Tab::Remote => self.detail_remote(ui, sel),
                    Tab::Apps => self.detail_apps(ui, sel),
                }
            } else {
                ui.label(theme::body("Once soldan bir cihaz sec, sonra egik tilelardan birine bas."));
                ui.label(theme::meta("Tilelar klavye ile de calisir: Tab ile gez, Enter ile sec."));
            }
        });
    }

    fn services_pane(&self, ui: &mut egui::Ui) {
        theme::band_frame().show(ui, |ui| {
            ui.label(theme::kicker("servisler · dahili"));
            ui.label(theme::body(
                "Bu pencerede gomulu: paket sunucusu · dosya alici · pano esitleme · kesif.",
            ));
            let (pkt, http) = match &self.self_info {
                Some(i) => (i.packet_port.to_string(), i.http_port.to_string()),
                None => ("—".to_string(), "—".to_string()),
            };
            let daemon = if self.daemon_running { "CALISIYOR" } else { "DURDU" };
            let disc = if self.discovering { "TARIYOR" } else { "BEKLEMEDE" };
            ui.label(theme::mono(&format!("paket :{pkt} — {daemon}")));
            ui.label(theme::mono(&format!("http :{http} — alici acik")));
            ui.label(theme::mono(&format!("kesif/duyuru — {disc}")));
            ui.label(theme::meta(
                "Mirror/screen/audio telefon tarafindan baslatilir; masaustu burada salt dinler (ayri CLI).",
            ));
        });
    }

    fn action_pane(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.label(theme::kicker("eylemler · actions"));
                ui.label(theme::title("Eylemler"));
                theme::double_rule(ui);
                ui.add_space(6.0);
                self.status_strip(ui);
                ui.add_space(6.0);
                self.tile_grid(ui);
                self.detail_card(ui);
                ui.add_space(8.0);
                if let Some(p) = self.inbound.clone() {
                    theme::mini_frame().show(ui, |ui| {
                        ui.label(theme::kicker("son eslesme istegi"));
                        let verdict = if p.accepted { "ACCEPTED" } else { "REJECTED" };
                        ui.label(theme::body(&format!(
                            "{} ({}) — {verdict}",
                            p.alias,
                            fp_short(&p.fingerprint)
                        )));
                        if let Some(pin) = p.pin {
                            ui.label(theme::mono(&format!("PIN {pin} — telefon ekraniyla karsilastir")));
                        }
                        ui.label(theme::meta(&format!("via {}", p.channel)));
                    });
                    ui.add_space(8.0);
                }
                self.services_pane(ui);
                ui.add_space(4.0);
                ui.label(theme::meta("MM-01 · TEK MUREKKEP · CIFT CIZGI · EGIK TILE"));
            });
        });
    }

    fn log_pane(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("log")
            .resizable(true)
            .default_size(170.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(theme::kicker(&format!("kayit · log · {}", self.log.len())));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled(!self.log.is_empty(), egui::Button::new(theme::chip("CLEAR")))
                            .clicked()
                        {
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
        let busy = self.discovering
            || self.pair_flow.is_working()
            || self.ping_flow.is_working()
            || self.send_flow.is_working()
            || self.clip_flow.is_working()
            || self.wake_flow.is_working()
            || self.open_flow.is_working()
            || self.apps_flow.is_working()
            || self.sleep_flow.is_working()
            || self.keep_flow.is_working();
        if busy {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(120));
        } else {
            ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
        }
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
