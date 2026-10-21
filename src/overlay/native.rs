//! Native Wayland presentation, owned separately from the XWayland panel.
use super::session::{Gadget, OverlaySession, OverlayStatus, SnapshotSource};
use crate::config::Config;
use smithay_client_toolkit::reexports::calloop::ping::{Ping, make_ping};
use std::collections::BTreeSet;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::JoinHandle;
use wayland_client::Connection;
mod worker;

/// Keep the probed connection for startup, including inherited Wayland sockets.
pub(crate) fn auto_connection() -> Option<Connection> {
    let kde = std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|desktop| {
        desktop
            .split(':')
            .any(|name| name.eq_ignore_ascii_case("KDE"))
    });
    let wayland = std::env::var("XDG_SESSION_TYPE")
        .is_ok_and(|session| session.eq_ignore_ascii_case("wayland"));
    if !kde || !wayland {
        return None;
    }
    let connection = Connection::connect_to_env().ok()?;
    worker::supports_layer_shell(&connection).then_some(connection)
}

pub struct NativeOverlay {
    session: Arc<Mutex<OverlaySession>>,
    source: Arc<Mutex<SnapshotSource>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    ping: Option<Ping>,
    repaint: eframe::egui::Context,
}
impl NativeOverlay {
    pub fn start(
        config: Config,
        show: bool,
        source: SnapshotSource,
        repaint: eframe::egui::Context,
    ) -> Self {
        Self::with_connection(
            config,
            show,
            source,
            repaint,
            Connection::connect_to_env()
                .map_err(|e| format!("cannot connect to native Wayland: {e}")),
        )
    }
    pub fn with_connection(
        config: Config,
        show: bool,
        source: SnapshotSource,
        repaint: eframe::egui::Context,
        connection: Result<Connection, String>,
    ) -> Self {
        let mut overlay = Self {
            session: Arc::new(Mutex::new(OverlaySession::new(config, show))),
            source: Arc::new(Mutex::new(source)),
            stop: Arc::new(AtomicBool::new(false)),
            worker: None,
            ping: None,
            repaint,
        };
        overlay.spawn(connection);
        overlay
    }
    fn spawn(&mut self, connection: Result<Connection, String>) {
        self.stop.store(false, Ordering::Release);
        let (ping, ping_source) = match make_ping() {
            Ok(pair) => pair,
            Err(error) => {
                self.session
                    .lock()
                    .unwrap()
                    .failure(format!("cannot create overlay wakeup: {error}"));
                return;
            }
        };
        self.ping = Some(ping);
        let state = self.session.clone();
        let source = self.source.clone();
        let stop = self.stop.clone();
        let repaint = self.repaint.clone();
        self.worker = Some(std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                worker::run(
                    connection?,
                    state.clone(),
                    source,
                    stop,
                    repaint.clone(),
                    ping_source,
                )
            }))
            .unwrap_or_else(|_| {
                Err("native overlay worker panicked; retry or select the window backend".into())
            });
            if let Err(error) = result {
                log::error!(target: "openradar_graphics", "{error}");
                state.lock().unwrap().failure(error);
                repaint.request_repaint();
            }
        }));
    }
    pub fn update(
        &self,
        config: Config,
        show: bool,
        source: SnapshotSource,
        hidden_gadgets: &BTreeSet<Gadget>,
    ) {
        {
            let mut session = self.session.lock().unwrap();
            session.update(config, show);
            session.hidden_gadgets.clone_from(hidden_gadgets);
        }
        *self.source.lock().unwrap() = source;
        if let Some(ping) = &self.ping {
            ping.ping();
        }
    }
    pub fn status(&self) -> OverlayStatus {
        self.session.lock().unwrap().status()
    }
    pub fn set_radar_editing(&self, editing: bool) {
        self.session.lock().unwrap().radar_editing = editing;
    }
    pub fn config_for_save(&self) -> Config {
        self.session.lock().unwrap().config_for_save()
    }
    pub fn retry(&mut self) {
        let show = self.session.lock().unwrap().show;
        self.shutdown();
        {
            let mut session = self.session.lock().unwrap();
            session.retry();
            session.show = show;
        }
        self.spawn(
            Connection::connect_to_env()
                .map_err(|e| format!("cannot connect to native Wayland: {e}")),
        );
    }
    pub fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(ping) = &self.ping {
            ping.ping();
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        self.session.lock().unwrap().closed();
    }
}
impl Drop for NativeOverlay {
    fn drop(&mut self) {
        self.shutdown();
    }
}
