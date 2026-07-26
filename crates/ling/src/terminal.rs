#[cfg(windows)]
pub fn init() -> TerminalEncoding {
    TerminalEncoding::set_utf8()
}

#[cfg(not(windows))]
pub fn init() -> TerminalEncoding {
    TerminalEncoding
}

#[cfg(windows)]
pub struct TerminalEncoding {
    input_code_page: u32,
    output_code_page: u32,
}

#[cfg(not(windows))]
pub struct TerminalEncoding;

#[cfg(unix)]
pub fn width() -> Option<usize> {
    let mut size = std::mem::MaybeUninit::<libc::winsize>::zeroed();
    // SAFETY: TIOCGWINSZ only writes a winsize value to the valid pointer supplied here.
    let result = unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, size.as_mut_ptr()) };
    if result != 0 {
        return None;
    }
    // SAFETY: a successful TIOCGWINSZ call initialized the winsize value.
    let columns = unsafe { size.assume_init() }.ws_col;
    (columns > 0).then_some(columns as usize)
}

#[cfg(windows)]
pub fn width() -> Option<usize> {
    use windows_sys::Win32::System::Console::{
        GetConsoleScreenBufferInfo, GetStdHandle, CONSOLE_SCREEN_BUFFER_INFO, STD_OUTPUT_HANDLE,
    };

    unsafe {
        let handle = GetStdHandle(STD_OUTPUT_HANDLE);
        let mut info = std::mem::zeroed::<CONSOLE_SCREEN_BUFFER_INFO>();
        if GetConsoleScreenBufferInfo(handle, &mut info) == 0 {
            return None;
        }
        let columns = info.srWindow.Right - info.srWindow.Left + 1;
        (columns > 0).then_some(columns as usize)
    }
}

#[cfg(not(any(unix, windows)))]
pub fn width() -> Option<usize> {
    None
}

#[cfg(windows)]
impl TerminalEncoding {
    fn set_utf8() -> Self {
        use windows_sys::Win32::System::Console::{
            GetConsoleCP, GetConsoleOutputCP, SetConsoleCP, SetConsoleOutputCP,
        };

        const CP_UTF8: u32 = 65001;

        // The Windows console defaults to an OEM code page in legacy PowerShell.
        // Switch to UTF-8 before any Chinese prompt/error text is written.
        unsafe {
            let input_code_page = GetConsoleCP();
            let output_code_page = GetConsoleOutputCP();

            if input_code_page != 0 && input_code_page != CP_UTF8 {
                let _ = SetConsoleCP(CP_UTF8);
            }
            if output_code_page != 0 && output_code_page != CP_UTF8 {
                let _ = SetConsoleOutputCP(CP_UTF8);
            }

            Self {
                input_code_page,
                output_code_page,
            }
        }
    }
}

#[cfg(windows)]
impl Drop for TerminalEncoding {
    fn drop(&mut self) {
        use windows_sys::Win32::System::Console::{SetConsoleCP, SetConsoleOutputCP};

        const CP_UTF8: u32 = 65001;

        unsafe {
            if self.input_code_page != 0 && self.input_code_page != CP_UTF8 {
                let _ = SetConsoleCP(self.input_code_page);
            }
            if self.output_code_page != 0 && self.output_code_page != CP_UTF8 {
                let _ = SetConsoleOutputCP(self.output_code_page);
            }
        }
    }
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;

    #[test]
    fn init_is_noop_guard_on_non_windows() {
        let _guard = init();
    }

    #[test]
    fn reported_terminal_width_is_positive() {
        assert!(width().is_none_or(|width| width > 0));
    }
}
