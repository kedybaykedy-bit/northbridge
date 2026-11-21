mod app;
mod backend;
mod model;
mod theme;

fn main() -> eframe::Result<()> {
    let (event_tx, event_rx) = std::sync::mpsc::channel();
    let backend = backend::Backend::spawn(event_tx);
    app::run_app(backend, event_rx)
}
