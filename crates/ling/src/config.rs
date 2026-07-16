use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct LingConfig {
    pub api_key: Option<String>,
}

impl LingConfig {
    pub fn load() -> Result<Self> {
        let path = config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = fs::read_to_string(&path)
            .with_context(|| format!("读取配置失败：{}", path.display()))?;
        serde_json::from_str(&content).with_context(|| format!("解析配置失败：{}", path.display()))
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("创建配置目录失败：{}", parent.display()))?;
        }
        let content = serde_json::to_string_pretty(self)?;
        fs::write(&path, content).with_context(|| format!("写入配置失败：{}", path.display()))
    }
}

fn config_path() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("LING_CONFIG") {
        return Ok(PathBuf::from(path));
    }
    let dir = dirs::config_dir().context("无法确定用户配置目录，请设置 LING_CONFIG")?;
    Ok(dir.join("listenai").join("ling").join("config.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{temp_path, EnvGuard};

    #[test]
    fn load_returns_default_when_config_file_is_missing() {
        let guard = EnvGuard::new(&["LING_CONFIG"]);
        let path = temp_path("ling-config-missing-test").join("config.json");
        guard.set_var("LING_CONFIG", &path);

        let cfg = LingConfig::load().expect("load missing config");

        assert!(cfg.api_key.is_none());
    }

    #[test]
    fn save_creates_parent_directories_and_load_reads_config() {
        let guard = EnvGuard::new(&["LING_CONFIG"]);
        let dir = temp_path("ling-config-save-test");
        let path = dir.join("listenai").join("ling").join("config.json");
        guard.set_var("LING_CONFIG", &path);

        LingConfig {
            api_key: Some("saved-key".to_owned()),
        }
        .save()
        .expect("save config");

        let content = fs::read_to_string(&path).expect("read config file");
        assert!(content.contains("\"api_key\""));
        assert_eq!(
            LingConfig::load().expect("load config").api_key.as_deref(),
            Some("saved-key")
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn load_reports_invalid_json_with_path_context() {
        let guard = EnvGuard::new(&["LING_CONFIG"]);
        let dir = temp_path("ling-config-invalid-test");
        let path = dir.join("config.json");
        fs::create_dir_all(&dir).expect("create config dir");
        fs::write(&path, "{not-json").expect("write invalid config");
        guard.set_var("LING_CONFIG", &path);

        let err = LingConfig::load().expect_err("invalid JSON should fail");

        let rendered = format!("{err:?}");
        assert!(rendered.contains("解析配置失败"));
        assert!(rendered.contains(path.to_string_lossy().as_ref()));
        let _ = fs::remove_dir_all(dir);
    }
}
