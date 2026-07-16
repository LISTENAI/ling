use anyhow::{Context, Result};
use reqwest::StatusCode;
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
    let models = value
        .get("data")
        .and_then(Value::as_array)
        .context("/v1/models 响应缺少 data 数组")?;
    Ok(models.len())
}

pub use ling_core::{bearer, strip_bearer};

pub fn render_login_success(output: &LoginOutput, api_base_url: &str) -> String {
    format!(
        "登录成功。\nAPI Key: {api_key}\nAPI Base URL: {api_base_url}\n可用模型: {model_count} 个\n\n下一步：\n- 查看账号：ling account\n- 查看模型：ling ai models\n- 查看应用：ling app list\n- 初始化 Agent：ling app init <agent_name> --product-id <product_id>\n- 切换设备 PID/SID：ling app inspect <product_id> 后执行 adb shell device set_pid/set_sid\n\n使用 `ling login --json` 输出原始 JSON。",
        api_key = output.api_key_preview,
        model_count = output.model_count
    )
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
    fn builds_authorization_bearer_header() {
        assert_eq!(bearer("abc"), "Bearer abc");
        assert_eq!(bearer("  Bearer abc  "), "Bearer abc");
    }

    #[test]
    fn previews_api_keys_without_leaking_full_value() {
        assert_eq!(
            preview_key("12345678-abcdefg-87654321"),
            "12345678...87654321"
        );
        assert_eq!(preview_key("short"), "****");
    }

    #[test]
    fn renders_login_success_with_next_steps() {
        let output = LoginOutput {
            auth_type: "api_key",
            api_key_preview: "12345678...87654321".to_owned(),
            model_count: 3,
        };

        let rendered = render_login_success(&output, "https://api.listenai.com");

        assert!(rendered.contains("登录成功"));
        assert!(rendered.contains("API Key: 12345678...87654321"));
        assert!(rendered.contains("API Base URL: https://api.listenai.com"));
        assert!(rendered.contains("可用模型: 3 个"));
        assert!(rendered.contains("ling account"));
        assert!(rendered.contains("ling app inspect <product_id>"));
        assert!(!rendered.contains("abcdefg"));
    }
}
