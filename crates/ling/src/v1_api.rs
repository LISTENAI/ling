use crate::api_key;
use anyhow::{Context, Result};
use reqwest::{Client, StatusCode, Url};
use serde_json::{json, Map, Value};
use std::io::{self, Write};

#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub model: String,
    pub prompt: String,
    pub system: Option<String>,
    pub stream: bool,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub max_tokens: Option<u32>,
}

pub async fn account(api_base_url: &str, api_key: &str) -> Result<Value> {
    get_json(api_base_url, api_key, "/v1/account").await
}

pub async fn models(api_base_url: &str, api_key: &str) -> Result<Value> {
    get_json(api_base_url, api_key, "/v1/models").await
}

pub async fn chat_completion(
    api_base_url: &str,
    api_key: &str,
    request: &ChatRequest,
) -> Result<Value> {
    post_chat(api_base_url, api_key, request).await
}

pub async fn chat_completion_stream(
    api_base_url: &str,
    api_key: &str,
    request: &ChatRequest,
) -> Result<()> {
    let url = api_url(api_base_url, "/v1/chat/completions")?;
    let client = Client::builder()
        .user_agent(concat!("ling/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let mut response = client
        .post(url)
        .header("authorization", api_key::bearer(api_key))
        .json(&chat_body(request, true))
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(api_error(status, "chat 接口请求失败", &body));
    }

    let mut buffer = String::new();
    while let Some(chunk) = response.chunk().await? {
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(line_end) = buffer.find('\n') {
            let line = buffer[..line_end].trim_end_matches('\r').to_owned();
            buffer.replace_range(..=line_end, "");
            if handle_sse_line(&line)? {
                println!();
                return Ok(());
            }
        }
    }

    if !buffer.is_empty() {
        handle_sse_line(buffer.trim_end_matches('\r'))?;
    }
    println!();
    Ok(())
}

pub fn render_account(value: &Value) -> Result<String> {
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .unwrap_or("-");
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .unwrap_or("-");
    let account_type = value
        .get("type")
        .and_then(Value::as_str)
        .filter(|account_type| !account_type.is_empty())
        .unwrap_or("-");

    Ok(format!(
        "账号信息：\nID: {id}\n名称: {name}\n类型: {account_type}\n\n使用 --json 输出原始 JSON。"
    ))
}

pub fn render_models(value: &Value) -> Result<String> {
    let models = value
        .get("data")
        .and_then(Value::as_array)
        .context("models 响应缺少 data 数组")?;

    if models.is_empty() {
        return Ok("暂无可用模型。使用 --json 输出原始 JSON。".to_owned());
    }

    let mut output = format!("可用模型 {} 个：", models.len());
    for (index, model) in models.iter().enumerate() {
        let id = model
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .unwrap_or("-");
        let description = model
            .get("description")
            .and_then(Value::as_str)
            .filter(|description| !description.is_empty())
            .unwrap_or("-");
        if description == "-" {
            output.push_str(&format!("\n{}. {id}", index + 1));
        } else {
            output.push_str(&format!("\n{}. {id}\n   {description}", index + 1));
        }
    }
    output.push_str("\n\n使用 --json 输出原始 JSON。");
    Ok(output)
}

pub fn render_chat_completion(value: &Value) -> Result<String> {
    let choices = value
        .get("choices")
        .and_then(Value::as_array)
        .context("chat 响应缺少 choices 数组")?;
    let first = choices.first().context("chat 响应 choices 为空")?;

    if let Some(content) = first
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(content_value_to_text)
    {
        return Ok(content);
    }

    if let Some(text) = first.get("text").and_then(Value::as_str) {
        return Ok(text.to_owned());
    }

    anyhow::bail!("chat 响应缺少 choices[0].message.content");
}

fn handle_sse_line(line: &str) -> Result<bool> {
    let Some(data) = line.strip_prefix("data:") else {
        return Ok(false);
    };
    let data = data.trim();
    if data.is_empty() {
        return Ok(false);
    }
    if data == "[DONE]" {
        return Ok(true);
    }

    let value: Value = serde_json::from_str(data).context("chat 流式响应不是合法 JSON")?;
    if let Some(text) = stream_delta_text(&value) {
        print!("{text}");
        io::stdout().flush()?;
    }
    Ok(false)
}

fn stream_delta_text(value: &Value) -> Option<String> {
    value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("delta").or_else(|| choice.get("message")))
        .and_then(|delta| delta.get("content"))
        .and_then(content_value_to_text)
}

fn content_value_to_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.to_owned()),
        Value::Array(items) => {
            let text = items
                .iter()
                .filter_map(|item| {
                    item.get("text")
                        .and_then(Value::as_str)
                        .or_else(|| item.get("content").and_then(Value::as_str))
                })
                .collect::<Vec<_>>()
                .join("");
            (!text.is_empty()).then_some(text)
        }
        _ => None,
    }
}

async fn get_json(api_base_url: &str, api_key: &str, path: &str) -> Result<Value> {
    let url = api_url(api_base_url, path)?;
    let response = Client::builder()
        .user_agent(concat!("ling/", env!("CARGO_PKG_VERSION")))
        .build()?
        .get(url)
        .header("authorization", api_key::bearer(api_key))
        .send()
        .await?;

    parse_json_response(response, "v1 接口请求失败").await
}

async fn post_chat(api_base_url: &str, api_key: &str, request: &ChatRequest) -> Result<Value> {
    let url = api_url(api_base_url, "/v1/chat/completions")?;
    let response = Client::builder()
        .user_agent(concat!("ling/", env!("CARGO_PKG_VERSION")))
        .build()?
        .post(url)
        .header("authorization", api_key::bearer(api_key))
        .json(&chat_body(request, false))
        .send()
        .await?;

    parse_json_response(response, "chat 接口请求失败").await
}

async fn parse_json_response(response: reqwest::Response, message: &str) -> Result<Value> {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(api_error(status, message, &body));
    }

    serde_json::from_str(&body).with_context(|| format!("{message}：响应不是合法 JSON"))
}

fn api_error(status: StatusCode, message: &str, body: &str) -> anyhow::Error {
    if status == StatusCode::UNAUTHORIZED {
        return anyhow::anyhow!(
            "{message}：HTTP 401，请先确认 `ling login` 使用的是 /keys 页面 API Key"
        );
    }
    anyhow::anyhow!("{message}：HTTP {status} {body}")
}

fn chat_body(request: &ChatRequest, force_stream: bool) -> Value {
    let mut body = Map::new();
    body.insert("model".to_owned(), json!(request.model));
    body.insert("messages".to_owned(), json!(chat_messages(request)));
    body.insert("stream".to_owned(), json!(force_stream || request.stream));

    if let Some(temperature) = request.temperature {
        body.insert("temperature".to_owned(), json!(temperature));
    }
    if let Some(top_p) = request.top_p {
        body.insert("top_p".to_owned(), json!(top_p));
    }
    if let Some(max_tokens) = request.max_tokens {
        body.insert("max_tokens".to_owned(), json!(max_tokens));
    }

    Value::Object(body)
}

fn chat_messages(request: &ChatRequest) -> Vec<Value> {
    let mut messages = Vec::new();
    if let Some(system) = request
        .system
        .as_deref()
        .filter(|system| !system.is_empty())
    {
        messages.push(json!({
            "role": "system",
            "content": system
        }));
    }
    messages.push(json!({
        "role": "user",
        "content": request.prompt
    }));
    messages
}

fn api_url(api_base_url: &str, path: &str) -> Result<Url> {
    let base_url = Url::parse(api_base_url).context("LING_API_BASE_URL 不是合法 URL")?;
    base_url
        .join(path.trim_start_matches('/'))
        .context("接口 URL 拼接失败")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_account_summary() {
        let value = json!({
            "id": "123",
            "name": "ListenAI",
            "type": "developer"
        });

        let output = render_account(&value).unwrap();
        assert!(output.contains("账号信息"));
        assert!(output.contains("ID: 123"));
        assert!(output.contains("名称: ListenAI"));
        assert!(output.contains("类型: developer"));
    }

    #[test]
    fn renders_models_summary() {
        let value = json!({
            "object": "list",
            "data": [
                {"id": "qwen3-next-80b-a3b-instruct", "description": "通用模型"},
                {"id": "deepseek-v3", "description": ""}
            ]
        });

        let output = render_models(&value).unwrap();
        assert!(output.contains("可用模型 2 个"));
        assert!(output.contains("1. qwen3-next-80b-a3b-instruct"));
        assert!(output.contains("通用模型"));
        assert!(output.contains("2. deepseek-v3"));
    }

    #[test]
    fn renders_chat_content() {
        let value = json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "你好"
                }
            }]
        });

        assert_eq!(render_chat_completion(&value).unwrap(), "你好");
    }

    #[test]
    fn extracts_stream_delta_text() {
        let value = json!({
            "choices": [{
                "delta": {
                    "content": "你好"
                }
            }]
        });

        assert_eq!(stream_delta_text(&value).unwrap(), "你好");
    }

    #[test]
    fn builds_chat_body_with_options() {
        let request = ChatRequest {
            model: "qwen3-next-80b-a3b-instruct".to_owned(),
            prompt: "你好".to_owned(),
            system: Some("你是助手".to_owned()),
            stream: false,
            temperature: Some(0.2),
            top_p: Some(0.9),
            max_tokens: Some(128),
        };

        let body = chat_body(&request, false);
        assert_eq!(body["model"], "qwen3-next-80b-a3b-instruct");
        assert_eq!(body["stream"], false);
        assert!((body["temperature"].as_f64().unwrap() - 0.2).abs() < 0.0001);
        assert!((body["top_p"].as_f64().unwrap() - 0.9).abs() < 0.0001);
        assert_eq!(body["max_tokens"], 128);
        assert_eq!(body["messages"].as_array().unwrap().len(), 2);
    }
}
