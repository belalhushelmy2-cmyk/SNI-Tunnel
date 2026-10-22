//! Console-window helpers so the GUI can run without a lingering console when
//! it is launched by Explorer / the Run key (double-click, autostart) while
//! still keeping the console when launched from an existing terminal.

/// Hide this process's console window *only* when we are its sole owner.
///
/// `GetConsoleProcessList` returns the number of processes attached to the
/// console. When Explorer or the Run key launches a console-subsystem app,
/// that count is 1 (just us) and the freshly created console can be hidden;
/// when a user runs us from an existing terminal the count is >= 2 and the
/// shared console is left untouched.
#[cfg(windows)]
pub fn hide_if_sole_owner() {
    use windows_sys::Win32::System::Console::{GetConsoleProcessList, GetConsoleWindow};
    use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
    unsafe {
        let mut pids = [0u32; 2];
        let count = GetConsoleProcessList(pids.as_mut_ptr(), pids.len() as u32);
        if count == 1 {
            let hwnd = GetConsoleWindow();
            if !hwnd.is_null() {
                ShowWindow(hwnd, SW_HIDE);
            }
        }
    }
}

#[cfg(not(windows))]
pub fn hide_if_sole_owner() {}
