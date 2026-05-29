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
