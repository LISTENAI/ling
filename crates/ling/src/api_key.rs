use anyhow::{Context, Result};
use reqwest::{Client, StatusCode, Url};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Serialize)]
pub struct LoginOutput {
    pub auth_type: &'static str,
    pub api_key_preview: String,
    pub model_count: usize,
}

pub async fn login_with_api_key(api_base_url: &str, api_key: &str) -> Result<LoginOutput> {
    let api_key = strip_bearer(api_key);
    let model_count = validate_api_key(api_base_url, &api_key).await?;

    Ok(LoginOutput {
        auth_type: "api_key",
        api_key_preview: preview_key(&api_key),
        model_count,
    })
}

async fn validate_api_key(api_base_url: &str, api_key: &str) -> Result<usize> {
    let url = api_url(api_base_url, "/v1/models")?;
    let response = Client::builder()
        .user_agent(concat!("ling/", env!("CARGO_PKG_VERSION")))
        .build()?
        .get(url)
        .header("authorization", bearer(api_key))
        .send()
        .await?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if status == StatusCode::UNAUTHORIZED {
        anyhow::bail!("API Key 校验失败：HTTP 401，请确认使用的是 platform.listenai.com/keys 页面里的完整 key");
    }
    if !status.is_success() {
        anyhow::bail!("API Key 校验失败：HTTP {status} {body}");
    }

    let value: Value = serde_json::from_str(&body).context("/v1/models 响应不是合法 JSON")?;
    let models = value
        .get("data")
        .and_then(Value::as_array)
        .context("/v1/models 响应缺少 data 数组")?;
    Ok(models.len())
}

fn api_url(api_base_url: &str, path: &str) -> Result<Url> {
    let base_url = Url::parse(api_base_url).context("LING_API_BASE_URL 不是合法 URL")?;
    base_url
        .join(path.trim_start_matches('/'))
        .context("接口 URL 拼接失败")
}

pub fn strip_bearer(api_key: &str) -> String {
    let api_key = api_key.trim();
    if api_key.to_ascii_lowercase().starts_with("bearer ") {
        api_key[7..].trim().to_owned()
    } else {
        api_key.to_owned()
    }
}

pub fn bearer(api_key: &str) -> String {
    format!("Bearer {}", strip_bearer(api_key))
}

pub fn preview_key(api_key: &str) -> String {
    let api_key = strip_bearer(api_key);
    let chars = api_key.chars().collect::<Vec<_>>();
    if chars.len() <= 16 {
        return "****".to_owned();
    }

    let prefix = chars.iter().take(8).collect::<String>();
    let suffix = chars
        .iter()
        .rev()
        .take(8)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    format!("{prefix}...{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_bearer_prefix() {
        assert_eq!(strip_bearer("abc"), "abc");
        assert_eq!(strip_bearer("Bearer abc"), "abc");
        assert_eq!(strip_bearer("bearer abc"), "abc");
    }

    #[test]
    fn previews_api_keys_without_leaking_full_value() {
        assert_eq!(
            preview_key("12345678-abcdefg-87654321"),
            "12345678...87654321"
        );
        assert_eq!(preview_key("short"), "****");
    }
}
