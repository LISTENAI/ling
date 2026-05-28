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
