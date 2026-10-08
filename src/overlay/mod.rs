//! Native desktop rendering, isolated from protocol decoding and geometry.
#[cfg(feature = "desktop")]
mod delta_style;
#[cfg(feature = "desktop")]
mod desktop;
#[cfg(feature = "desktop")]
mod folder_picker;
#[cfg(feature = "desktop")]
mod fonts;
#[cfg(feature = "desktop")]
mod fuel_style;
#[cfg(feature = "desktop")]
mod gadget_style;
#[cfg(feature = "desktop")]
mod gap_style;
#[cfg(feature = "desktop")]
mod graphics_log;
#[cfg(feature = "desktop")]
mod gt7_style;
#[cfg(feature = "desktop")]
mod hotkey;
#[cfg(feature = "desktop")]
mod radar_style;
#[cfg(feature = "desktop")]
mod render;
#[cfg(feature = "desktop")]
mod speed_dashboard;
#[cfg(feature = "desktop")]
mod theme;
#[cfg(feature = "desktop")]
mod window;
#[cfg(feature = "desktop")]
pub use desktop::run;

/// None means foreground detection is not implemented on this desktop backend.
pub fn game_foreground() -> Option<bool> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::{
            Foundation::CloseHandle,
            System::Threading::{
                OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
            },
            UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
        };
        // Read-only process identification; no focus or game-window mutations.
        unsafe {
            let window = GetForegroundWindow();
            if window.is_null() {
                return Some(false);
            }
            let mut pid = 0;
            GetWindowThreadProcessId(window, &mut pid);
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                return Some(false);
            }
            let mut path = [0_u16; 1024];
            let mut size = path.len() as u32;
            let ok = QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut size);
            CloseHandle(process);
            if ok == 0 {
                return Some(false);
            }
            let path = String::from_utf16_lossy(&path[..size as usize]);
            Some(
                path.rsplit('\\')
                    .next()
                    .is_some_and(|name| name.eq_ignore_ascii_case("LFS.exe")),
            )
        }
    }
    #[cfg(not(windows))]
    {
        None
    }
}
