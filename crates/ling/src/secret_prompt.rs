use anyhow::Result;

const API_KEY_PROMPT: &str =
    "请打开 https://platform.listenai.com/keys 获取 API Key。\r\nAPI Key: ";

pub fn prompt_api_key() -> Result<String> {
    platform::prompt_api_key()
}

#[cfg(unix)]
mod platform {
    use super::*;
    use anyhow::Context;
    use std::io::{self, Write};
    use std::time::Duration;

    const BRACKETED_PASTE_ENABLE: &[u8] = b"\x1b[?2004h";
    const BRACKETED_PASTE_DISABLE: &[u8] = b"\x1b[?2004l";
    const PASTE_START_AFTER_ESC: &[u8] = b"[200~";
    const PASTE_END: &[u8] = b"\x1b[201~";

    pub fn prompt_api_key() -> Result<String> {
        if !stdin_is_tty() {
            return prompt_from_stdin();
        }

        let _guard = TerminalGuard::enter().context("终端隐藏输入初始化失败")?;
        let mut prompt = HiddenPrompt::new();
        prompt.run()
    }

    struct HiddenPrompt {
        api_key: String,
    }

    impl HiddenPrompt {
        fn new() -> Self {
            Self {
                api_key: String::new(),
            }
        }

        fn run(&mut self) -> Result<String> {
            write_stderr(API_KEY_PROMPT.as_bytes())?;

            loop {
                match read_byte()? {
                    b'\r' | b'\n' => {
                        write_stderr(b"\r\n")?;
                        return Ok(std::mem::take(&mut self.api_key));
                    }
                    3 => {
                        write_stderr(b"\r\n")?;
                        anyhow::bail!("已取消");
                    }
                    4 if self.api_key.is_empty() => {
                        write_stderr(b"\r\n")?;
                        anyhow::bail!("未读取到 API Key");
                    }
                    8 | 127 => {
                        self.pop_char()?;
                    }
                    21 => {
                        let displayed = self.api_key.chars().count();
                        self.api_key.clear();
                        erase_mask(displayed)?;
                    }
                    b'\x1b' => {
                        if let Some(paste) = read_bracketed_paste()? {
                            self.push_text(&paste)?;
                        }
                    }
                    byte if !byte.is_ascii_control() => {
                        self.api_key.push(byte as char);
                        write_stderr(b"*")?;
                    }
                    _ => {}
                }
            }
        }

        fn push_text(&mut self, text: &str) -> Result<()> {
            let mut added = 0;
            for character in text.chars().filter(|character| !character.is_control()) {
                self.api_key.push(character);
                added += 1;
            }
            if added > 0 {
                write_stderr("*".repeat(added).as_bytes())?;
            }
            Ok(())
        }

        fn pop_char(&mut self) -> Result<()> {
            if self.api_key.pop().is_some() {
                write_stderr(b"\x08 \x08")?;
            }
            Ok(())
        }
    }

    fn erase_mask(count: usize) -> io::Result<()> {
        if count > 0 {
            write_stderr("\x08 \x08".repeat(count).as_bytes())?;
        }
        Ok(())
    }

    struct TerminalGuard {
        original: libc::termios,
    }

    impl TerminalGuard {
        fn enter() -> io::Result<Self> {
            let mut original = std::mem::MaybeUninit::<libc::termios>::uninit();
            if unsafe { libc::tcgetattr(libc::STDIN_FILENO, original.as_mut_ptr()) } != 0 {
                return Err(io::Error::last_os_error());
            }
            let original = unsafe { original.assume_init() };

            let mut raw = original;
            raw.c_lflag &= !(libc::ECHO | libc::ICANON | libc::ISIG);
            raw.c_cc[libc::VMIN] = 1;
            raw.c_cc[libc::VTIME] = 0;

            if unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &raw) } != 0 {
                return Err(io::Error::last_os_error());
            }

            write_stderr(BRACKETED_PASTE_ENABLE)?;
            Ok(Self { original })
        }
    }

    impl Drop for TerminalGuard {
        fn drop(&mut self) {
            let _ = write_stderr(BRACKETED_PASTE_DISABLE);
            unsafe {
                libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &self.original);
            }
        }
    }

    fn read_bracketed_paste() -> io::Result<Option<String>> {
        for expected in PASTE_START_AFTER_ESC {
            match read_byte_timeout(Duration::from_millis(30))? {
                Some(byte) if byte == *expected => {}
                _ => return Ok(None),
            }
        }

        let mut paste = Vec::new();
        loop {
            paste.push(read_byte()?);
            if paste.ends_with(PASTE_END) {
                paste.truncate(paste.len() - PASTE_END.len());
                return Ok(Some(String::from_utf8_lossy(&paste).into_owned()));
            }
        }
    }

    fn prompt_from_stdin() -> Result<String> {
        write_stderr(API_KEY_PROMPT.as_bytes())?;
        let mut api_key = String::new();
        io::stdin()
            .read_line(&mut api_key)
            .context("读取 API Key 失败")?;
        Ok(api_key.trim_end_matches(['\r', '\n']).to_owned())
    }

    fn stdin_is_tty() -> bool {
        unsafe { libc::isatty(libc::STDIN_FILENO) == 1 }
    }

    fn read_byte() -> io::Result<u8> {
        let mut byte = 0;
        loop {
            let read = unsafe {
                libc::read(
                    libc::STDIN_FILENO,
                    &mut byte as *mut u8 as *mut libc::c_void,
                    1,
                )
            };
            if read == 1 {
                return Ok(byte);
            }
            if read == 0 {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "stdin closed"));
            }

            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
    }

    fn read_byte_timeout(timeout: Duration) -> io::Result<Option<u8>> {
        let mut read_fds = unsafe { std::mem::zeroed::<libc::fd_set>() };
        unsafe {
            libc::FD_ZERO(&mut read_fds);
            libc::FD_SET(libc::STDIN_FILENO, &mut read_fds);
        }

        let mut timeout = libc::timeval {
            tv_sec: timeout
                .as_secs()
                .try_into()
                .expect("timeout seconds fit in timeval"),
            // `tv_usec` is i64 on Linux and i32 on macOS. Subsecond
            // microseconds are always at most 999,999, so both fit.
            tv_usec: timeout.subsec_micros() as _,
        };

        loop {
            let ready = unsafe {
                libc::select(
                    libc::STDIN_FILENO + 1,
                    &mut read_fds,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    &mut timeout,
                )
            };
            if ready > 0 {
                return read_byte().map(Some);
            }
            if ready == 0 {
                return Ok(None);
            }

            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
    }

    fn write_stderr(bytes: &[u8]) -> io::Result<()> {
        let mut stderr = io::stderr().lock();
        stderr.write_all(bytes)?;
        stderr.flush()
    }
}

#[cfg(not(unix))]
mod platform {
    use super::*;

    pub fn prompt_api_key() -> Result<String> {
        let config = rpassword::ConfigBuilder::new()
            .password_feedback_mask('*')
            .build();
        Ok(rpassword::prompt_password_with_config(
            API_KEY_PROMPT,
            config,
        )?)
    }
}
