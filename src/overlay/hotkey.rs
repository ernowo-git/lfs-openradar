//! Observe the configured key without consuming keyboard input meant for LFS.
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
}

impl OverlayHotkey {
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
