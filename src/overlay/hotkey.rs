//! Toggle shared visibility through Windows polling or KDE global shortcuts.
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::JoinHandle,
};

pub(super) struct OverlayHotkey {
    presses: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    #[cfg(target_os = "linux")]
    error: Arc<std::sync::Mutex<Option<String>>>,
}

impl OverlayHotkey {
    #[cfg(target_os = "linux")]
    pub fn start(key: u8, ctx: eframe::egui::Context) -> Result<Self, String> {
        use std::{thread, time::Duration};
        let registration = kde::Registration::new(key)
            .map_err(|e| format!("could not register KDE overlay shortcut: {e}"))?;
        let presses = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let error = Arc::new(std::sync::Mutex::new(None));
        let queued = presses.clone();
        let stopping = stop.clone();
        let failed = error.clone();
        registration
            .listen(queued, ctx.clone(), error.clone())
            .map_err(|e| format!("could not listen for KDE overlay shortcut: {e}"))?;
        let worker = thread::Builder::new()
            .name("overlay-hotkey".into())
            .spawn(move || {
                let report = |message| {
                    *failed.lock().unwrap() = Some(message);
                    ctx.request_repaint();
                };
                while !stopping.load(Ordering::Relaxed) {
                    if let Err(e) = registration.connection.process(Duration::from_millis(100)) {
                        report(format!("kde overlay shortcut connection failed: {e}"));
                        break;
                    }
                }
            })
            .map_err(|e| format!("could not start overlay hotkey: {e}"))?;
        Ok(Self {
            presses,
            stop,
            worker: Some(worker),
            error,
        })
    }

    #[cfg(target_os = "linux")]
    pub fn error(&self) -> Option<String> {
        self.error.lock().unwrap().clone()
    }
    #[cfg(windows)]
    pub fn start(key: u8, ctx: eframe::egui::Context) -> Result<Self, String> {
        use std::{thread, time::Duration};
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

        let presses = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let queued = presses.clone();
        let stopping = stop.clone();
        let worker = thread::Builder::new()
            .name("overlay-hotkey".into())
            .spawn(move || {
                // Only the high bit is reliable; the low "pressed since last call"
                // bit is shared with other applications. A held key never repeats.
                let down = || unsafe { GetAsyncKeyState(i32::from(key)) < 0 };
                let mut latch = PressLatch::new(down());
                while !stopping.load(Ordering::Relaxed) {
                    let pressed = latch.observe(down());
                    if pressed
                        && ![0x10, 0x11, 0x12, 0x5b, 0x5c]
                            .into_iter()
                            .any(|modifier| unsafe { GetAsyncKeyState(modifier) < 0 })
                    {
                        queued.fetch_add(1, Ordering::Relaxed);
                        // Wake the control panel even when its window is occluded.
                        ctx.request_repaint();
                    }
                    thread::sleep(Duration::from_millis(8));
                }
            })
            .map_err(|e| format!("could not start overlay hotkey: {e}"))?;
        Ok(Self {
            presses,
            stop,
            worker: Some(worker),
        })
    }

    pub fn take_toggle(&self) -> bool {
        !self.presses.swap(0, Ordering::Relaxed).is_multiple_of(2)
    }

    #[cfg(test)]
    pub fn queued(presses: Arc<AtomicUsize>) -> Self {
        Self {
            presses,
            stop: Arc::new(AtomicBool::new(false)),
            worker: None,
            #[cfg(target_os = "linux")]
            error: Arc::new(std::sync::Mutex::new(None)),
        }
    }
}

#[cfg(target_os = "linux")]
mod kde {
    use super::*;
    use dbus::{blocking::Connection, message::MatchRule};
    use std::time::Duration;

    const SERVICE: &str = "org.kde.kglobalaccel";
    const INTERFACE: &str = "org.kde.KGlobalAccel";
    const COMPONENT: &str = "lfs-openradar";
    const ACTION: &str = "toggle-overlays";

    pub(super) struct Registration {
        pub connection: Connection,
        action: Vec<String>,
        path: dbus::Path<'static>,
    }

    impl Registration {
        pub fn new(key: u8) -> Result<Self, String> {
            let connection = Connection::new_session().map_err(|e| e.to_string())?;
            let bus = connection.with_proxy(
                "org.freedesktop.DBus",
                "/org/freedesktop/DBus",
                Duration::from_secs(2),
            );
            let (available,): (bool,) = bus
                .method_call("org.freedesktop.DBus", "NameHasOwner", (SERVICE,))
                .map_err(|e| e.to_string())?;
            if !available {
                return Err(
                    "KDE global shortcut service unavailable; shortcut works in the panel only"
                        .into(),
                );
            }
            let owner = connection
                .request_name("space.ctrlchsr.OpenRadar.Shortcuts", false, false, true)
                .map_err(|e| e.to_string())?;
            if owner
                != dbus::blocking::stdintf::org_freedesktop_dbus::RequestNameReply::PrimaryOwner
            {
                return Err("another OpenRadar instance owns the global shortcut".into());
            }
            let qt_key = match key {
                0x1b => 0x01000000,
                0x09 => 0x01000001,
                0x08 => 0x01000003,
                0x0d => 0x01000004,
                0x2d => 0x01000006,
                0x2e => 0x01000007,
                0x24 => 0x01000010,
                0x23 => 0x01000011,
                0x21 => 0x01000012,
                0x22 => 0x01000013,
                0x70..=0x87 => 0x01000030 + i32::from(key - 0x70),
                _ => i32::from(key),
            };
            let proxy = connection.with_proxy(SERVICE, "/kglobalaccel", Duration::from_secs(2));
            let (available,): (bool,) = proxy
                .method_call(INTERFACE, "isGlobalShortcutAvailable", (qt_key, COMPONENT))
                .map_err(|e| e.to_string())?;
            if !available {
                return Err("key is already assigned to another KDE action; change overlay_toggle_key and restart".into());
            }
            let action: Vec<String> = [COMPONENT, ACTION, "OpenRadar", "Show/hide overlays"]
                .into_iter()
                .map(String::from)
                .collect();
            let (): () = proxy
                .method_call(INTERFACE, "doRegister", (action.clone(),))
                .map_err(|e| e.to_string())?;
            // The guard unregisters even when binding or subscription fails.
            let mut registration = Self {
                connection,
                action,
                path: dbus::Path::from("/"),
            };
            let proxy = registration.connection.with_proxy(
                SERVICE,
                "/kglobalaccel",
                Duration::from_secs(2),
            );
            let (path,): (dbus::Path<'static>,) = proxy
                .method_call(INTERFACE, "getComponent", (COMPONENT,))
                .map_err(|e| e.to_string())?;
            registration.path = path;
            // SetPresent | NoAutoloading: the TOML setting is authoritative.
            let (keys,): (Vec<i32>,) = proxy
                .method_call(
                    INTERFACE,
                    "setShortcut",
                    (registration.action.clone(), vec![qt_key], 6u32),
                )
                .map_err(|e| e.to_string())?;
            if keys != [qt_key] {
                return Err(
                    "KDE could not bind the configured key; change overlay_toggle_key and restart"
                        .into(),
                );
            }
            Ok(registration)
        }

        pub fn listen(
            &self,
            queued: Arc<AtomicUsize>,
            ctx: eframe::egui::Context,
            error: Arc<std::sync::Mutex<Option<String>>>,
        ) -> Result<(), dbus::Error> {
            let repaint = ctx.clone();
            let rule =
                MatchRule::new_signal("org.kde.kglobalaccel.Component", "globalShortcutPressed")
                    .with_sender(SERVICE)
                    .with_path(self.path.clone());
            self.connection.add_match(
                rule,
                move |(component, action, _timestamp): (String, String, i64), _, _| {
                    if component == COMPONENT && action == ACTION {
                        queued.fetch_add(1, Ordering::Relaxed);
                        ctx.request_repaint();
                    }
                    true
                },
            )?;
            // KDE emits held-key repeats separately; only presses are subscribed.
            self.connection.add_match(
                MatchRule::new_signal("org.freedesktop.DBus", "NameOwnerChanged")
                    .with_sender("org.freedesktop.DBus"),
                move |(name, _old, _new): (String, String, String), _, _| {
                    if name == SERVICE {
                        *error.lock().unwrap() = Some("kde global shortcut service restarted; shortcut works in the panel only".into());
                        repaint.request_repaint();
                    }
                    true
                },
            )?;
            Ok(())
        }
    }

    impl Drop for Registration {
        fn drop(&mut self) {
            let proxy =
                self.connection
                    .with_proxy(SERVICE, "/kglobalaccel", Duration::from_secs(1));
            let _: Result<(bool,), _> =
                proxy.method_call(INTERFACE, "unregister", (COMPONENT, ACTION));
        }
    }
}

impl Drop for OverlayHotkey {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(any(windows, test))]
struct PressLatch {
    down: bool,
}
#[cfg(any(windows, test))]
impl PressLatch {
    fn new(down: bool) -> Self {
        Self { down }
    }
    fn observe(&mut self, down: bool) -> bool {
        let pressed = down && !self.down;
        self.down = down;
        pressed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn holding_or_starting_with_the_key_down_does_not_repeat() {
        let mut latch = PressLatch::new(true);
        assert!(!latch.observe(true));
        assert!(!latch.observe(false));
        assert!(latch.observe(true));
        for _ in 0..20 {
            assert!(!latch.observe(true));
        }
        assert!(!latch.observe(false));
        assert!(latch.observe(true));
    }
    #[test]
    fn queued_presses_are_consumed_once_and_keep_toggle_parity() {
        let presses = Arc::new(AtomicUsize::new(1));
        let listener = OverlayHotkey::queued(presses.clone());
        assert!(listener.take_toggle());
        assert!(!listener.take_toggle());
        presses.store(2, Ordering::Relaxed);
        assert!(!listener.take_toggle());
        presses.store(3, Ordering::Relaxed);
        assert!(listener.take_toggle());
    }
}
