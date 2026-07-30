use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

const CLI_DEVICE_ID_PREFIX: &str = "ling_";
const CLI_DEVICE_ID_RANDOM_CHARS: usize = 8;
const CLI_DEVICE_ID_ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";

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
        Self::show_device_id()
    }

    pub fn show_device_id() -> Result<String> {
        let mut config = Self::load()?;
        if let Some(device_id) = config.cli_device_id.clone() {
            return Ok(device_id);
        }
        let device_id = generate_cli_device_id()?;
        config.cli_device_id = Some(device_id.clone());
        config.save()?;
        Ok(device_id)
    }

    pub fn reset_local_device_id() -> Result<String> {
        let mut config = Self::load()?;
        let device_id = generate_cli_device_id()?;
        config.cli_device_id = Some(device_id.clone());
        config.save()?;
        Ok(device_id)
    }
}

fn generate_cli_device_id() -> Result<String> {
    let mut bytes = [0_u8; CLI_DEVICE_ID_RANDOM_CHARS];
    getrandom::fill(&mut bytes)
        .map_err(|error| anyhow::anyhow!("生成 CLI Device ID 失败：{error}"))?;
    let random = bytes
        .iter()
        .map(|byte| {
            CLI_DEVICE_ID_ALPHABET[usize::from(*byte) % CLI_DEVICE_ID_ALPHABET.len()] as char
        })
        .collect::<String>();
    Ok(format!("{CLI_DEVICE_ID_PREFIX}{random}"))
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
            cli_device_id: Some("ling_cli_existing".to_owned()),
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
            Some("ling_cli_existing")
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
        assert_eq!(
            first.len(),
            CLI_DEVICE_ID_PREFIX.len() + CLI_DEVICE_ID_RANDOM_CHARS
        );
        assert!(first
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_'));
        assert!(!first.contains('-'));
        let encoded = first
            .strip_prefix(CLI_DEVICE_ID_PREFIX)
            .expect("recognizable CLI prefix");
        assert_eq!(encoded.len(), CLI_DEVICE_ID_RANDOM_CHARS);
        assert!(encoded
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte.is_ascii_lowercase()));
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
    fn saved_device_ids_are_reused_verbatim() {
        let guard = EnvGuard::new(&["LING_CONFIG"]);
        let dir = temp_path("ling-config-existing-device-id-test");
        let path = dir.join("config.json");
        guard.set_var("LING_CONFIG", &path);

        for device_id in [
            "123e4567e89b42d3a456426614174000",
            "ling-cli-123e4567-e89b-42d3-a456-426614174000",
            "ling_cli_123e4567e89b42d3",
            "ling_ab-cdefg",
            "ling_ABCDEFGH",
        ] {
            LingConfig {
                api_key: Some("saved-key".to_owned()),
                cli_device_id: Some(device_id.to_owned()),
            }
            .save()
            .expect("save config");

            assert_eq!(
                LingConfig::show_device_id().expect("show persisted ID"),
                device_id
            );
            assert_eq!(
                LingConfig::load_or_create_device_id().expect("reuse persisted ID"),
                device_id
            );
            let persisted = LingConfig::load().expect("load unchanged config");
            assert_eq!(persisted.cli_device_id.as_deref(), Some(device_id));
            assert_eq!(persisted.api_key.as_deref(), Some("saved-key"));
        }
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn reset_local_device_id_replaces_only_the_device_id() {
        let guard = EnvGuard::new(&["LING_CONFIG"]);
        let dir = temp_path("ling-config-device-id-reset-test");
        let path = dir.join("config.json");
        guard.set_var("LING_CONFIG", &path);

        LingConfig {
            api_key: Some("saved-key".to_owned()),
            cli_device_id: Some("invalid-device-id".to_owned()),
        }
        .save()
        .expect("save config");

        let device_id = LingConfig::reset_local_device_id().expect("reset device id");

        assert_ne!(device_id, "invalid-device-id");
        let persisted = LingConfig::load().expect("load reset config");
        assert_eq!(persisted.cli_device_id.as_deref(), Some(device_id.as_str()));
        assert_eq!(persisted.api_key.as_deref(), Some("saved-key"));
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
