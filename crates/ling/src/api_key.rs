use anyhow::{Context, Result};
use reqwest::StatusCode;
use serde_json::Value;

pub async fn login_with_api_key(api_base_url: &str, api_key: &str) -> Result<()> {
    let api_key = strip_bearer(api_key);
    validate_api_key(api_base_url, &api_key).await?;
    Ok(())
}

async fn validate_api_key(api_base_url: &str, api_key: &str) -> Result<()> {
    let url = ling_core::http_url(api_base_url, "/v1/models")?;
    let response = ling_core::client()?
        .get(url)
        .header("authorization", bearer(api_key))
        .send()
        .await?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if status == StatusCode::UNAUTHORIZED {
        anyhow::bail!(
            "API Key 校验失败：HTTP 401，请确认使用的是 https://platform.listenai.com/keys 页面里的完整 key"
        );
    }
    if !status.is_success() {
        anyhow::bail!("API Key 校验失败：HTTP {status} {body}");
    }

    let value: Value = serde_json::from_str(&body).context("/v1/models 响应不是合法 JSON")?;
    value
        .get("data")
        .and_then(Value::as_array)
        .context("/v1/models 响应缺少 data 数组")?;
    Ok(())
}

pub use ling_core::{bearer, strip_bearer};

pub fn render_login_success() -> &'static str {
    "登录成功。"
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
    fn builds_authorization_bearer_header() {
        assert_eq!(bearer("abc"), "Bearer abc");
        assert_eq!(bearer("  Bearer abc  "), "Bearer abc");
    }

    #[test]
    fn renders_concise_login_success() {
        assert_eq!(render_login_success(), "登录成功。");
    }
}
