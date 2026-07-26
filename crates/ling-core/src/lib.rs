//! ling 各命令与插件共享的基础能力：API 地址解析、WebSocket 地址转换、
//! 统一的 HTTP 客户端与 Bearer 处理。
//!
//! 约定：默认指向生产环境；调用方（CLI 主程序）通过 `--api-base-url` /
//! `LING_API_BASE_URL` 等参数显式覆盖后，把 base url 传给各插件。插件自身
//! 不保存、不写死任何环境地址。

use anyhow::{anyhow, bail, Context, Result};
use reqwest::Client;
use std::time::Duration;
use url::Url;

/// 默认 API 地址（生产环境）。
pub const DEFAULT_API_BASE_URL: &str = "https://api.listenai.com";

/// 把 base url 与路径拼接为 HTTP(S) URL。
pub fn http_url(base_url: &str, path: &str) -> Result<Url> {
    let mut base =
        Url::parse(base_url).with_context(|| format!("不是合法的 API 地址：{base_url}"))?;
    // 保留 base 自带的路径段（Url::join 对不以 / 结尾的路径会做替换而非追加）
    if !base.path().ends_with('/') {
        let path = format!("{}/", base.path());
        base.set_path(&path);
    }
    base.join(path.trim_start_matches('/'))
        .with_context(|| format!("接口 URL 拼接失败：{base_url} + {path}"))
}

/// 把 base url 与路径拼接为 WebSocket URL（https→wss、http→ws）。
pub fn ws_url(base_url: &str, path: &str) -> Result<Url> {
    let mut url = http_url(base_url, path)?;
    let scheme = match url.scheme() {
        "https" | "wss" => "wss",
        "http" | "ws" => "ws",
        other => bail!("不支持的协议：{other}"),
    };
    url.set_scheme(scheme)
        .map_err(|_| anyhow!("设置 WebSocket 协议失败"))?;
    Ok(url)
}

/// 统一 User-Agent 的 HTTP 客户端。
pub fn client() -> Result<Client> {
    Ok(Client::builder().user_agent(user_agent()).build()?)
}

/// 带整体超时的 HTTP 客户端。
pub fn client_with_timeout(timeout: Duration) -> Result<Client> {
    Ok(Client::builder()
        .user_agent(user_agent())
        .timeout(timeout)
        .build()?)
}

fn user_agent() -> &'static str {
    concat!("ling/", env!("CARGO_PKG_VERSION"))
}

/// 去掉可能存在的 "Bearer " 前缀并修剪空白。
pub fn strip_bearer(api_key: &str) -> String {
    let api_key = api_key.trim();
    if api_key.len() >= 7 && api_key[..7].eq_ignore_ascii_case("bearer ") {
        api_key[7..].trim().to_owned()
    } else {
        api_key.to_owned()
    }
}

/// 规范化为 "Bearer {key}" 头部值。
pub fn bearer(api_key: &str) -> String {
    format!("Bearer {}", strip_bearer(api_key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_http_url() {
        let url = http_url(DEFAULT_API_BASE_URL, "/v1/models").unwrap();
        assert_eq!(url.as_str(), "https://api.listenai.com/v1/models");
    }

    #[test]
    fn preserves_base_path_segments() {
        let url = http_url("https://gw.example.com/listenai", "/v1/models").unwrap();
        assert_eq!(url.as_str(), "https://gw.example.com/listenai/v1/models");
    }

    #[test]
    fn rejects_invalid_base() {
        assert!(http_url("not a url", "/v1/models").is_err());
    }

    #[test]
    fn converts_ws_scheme() {
        let url = ws_url("https://api.listenai.com", "/v1/asr").unwrap();
        assert_eq!(url.as_str(), "wss://api.listenai.com/v1/asr");
        let url = ws_url("http://localhost:8080", "/v1/asr").unwrap();
        assert_eq!(url.as_str(), "ws://localhost:8080/v1/asr");
    }

    #[test]
    fn strips_bearer_prefix_case_insensitively() {
        assert_eq!(strip_bearer("Bearer abc"), "abc");
        assert_eq!(strip_bearer("bearer abc"), "abc");
        assert_eq!(strip_bearer(" abc "), "abc");
        assert_eq!(bearer("abc"), "Bearer abc");
        assert_eq!(bearer("Bearer abc"), "Bearer abc");
    }
}
