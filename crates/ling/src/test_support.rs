use std::{
    env,
    ffi::OsStr,
    path::PathBuf,
    process,
    sync::{Mutex, MutexGuard, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub(crate) struct EnvGuard {
    saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
    _guard: MutexGuard<'static, ()>,
}

impl EnvGuard {
    pub(crate) fn new(vars: &[&'static str]) -> Self {
        let guard = ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("env lock");
        let saved = vars
            .iter()
            .map(|var| (*var, env::var_os(var)))
            .collect::<Vec<_>>();
        for var in vars {
            env::remove_var(var);
        }
        Self {
            saved,
            _guard: guard,
        }
    }

    pub(crate) fn set_var<K, V>(&self, key: K, value: V)
    where
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        env::set_var(key, value);
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (var, value) in &self.saved {
            match value {
                Some(value) => env::set_var(*var, value),
                None => env::remove_var(*var),
            }
        }
    }
}

pub(crate) fn temp_path(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    env::temp_dir().join(format!("{prefix}-{}-{nanos}", process::id()))
}
