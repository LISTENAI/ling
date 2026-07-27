use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct LingConfig {
    pub api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cli_device_id: Option<String>,
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

    pub fn load_or_create_device_id() -> Result<String> {
        let mut config = Self::load()?;
        if let Some(device_id) = config
            .cli_device_id
            .as_deref()
            .map(str::trim)
            .filter(|device_id| !device_id.is_empty())
        {
            return Ok(device_id.to_owned());
        }

        let device_id = generate_cli_device_id()?;
        config.cli_device_id = Some(device_id.clone());
        config.save()?;
        Ok(device_id)
    }
}

fn generate_cli_device_id() -> Result<String> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|error| anyhow::anyhow!("生成 CLI Device ID 失败：{error}"))?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Ok(format!(
        "ling-cli-{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-\
         {:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    ))
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
        assert!(cfg.cli_device_id.is_none());
    }

    #[test]
    fn save_creates_parent_directories_and_load_reads_config() {
        let guard = EnvGuard::new(&["LING_CONFIG"]);
        let dir = temp_path("ling-config-save-test");
        let path = dir.join("listenai").join("ling").join("config.json");
        guard.set_var("LING_CONFIG", &path);

        LingConfig {
            api_key: Some("saved-key".to_owned()),
            cli_device_id: Some("ling-cli-existing".to_owned()),
        }
        .save()
        .expect("save config");

        let content = fs::read_to_string(&path).expect("read config file");
        assert!(content.contains("\"api_key\""));
        assert_eq!(
            LingConfig::load().expect("load config").api_key.as_deref(),
            Some("saved-key")
        );
        assert_eq!(
            LingConfig::load()
                .expect("load config")
                .cli_device_id
                .as_deref(),
            Some("ling-cli-existing")
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn device_id_is_generated_once_and_persisted() {
        let guard = EnvGuard::new(&["LING_CONFIG"]);
        let dir = temp_path("ling-config-device-id-test");
        let path = dir.join("config.json");
        guard.set_var("LING_CONFIG", &path);

        let first = LingConfig::load_or_create_device_id().expect("generate device id");
        let second = LingConfig::load_or_create_device_id().expect("reuse device id");

        assert_eq!(first, second);
        assert!(first.starts_with("ling-cli-"));
        assert_eq!(first.len(), 45);
        assert_eq!(first.as_bytes()[23], b'4');
        assert!(matches!(first.as_bytes()[28], b'8' | b'9' | b'a' | b'b'));
        assert_eq!(
            LingConfig::load()
                .expect("load persisted config")
                .cli_device_id
                .as_deref(),
            Some(first.as_str())
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
