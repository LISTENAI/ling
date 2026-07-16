//! 端云链路模拟请求。
//!
//! 流程（参考文档「大模型端云交互链路协议」）：
//! 1. POST /v1/auth/tokens {productId, deviceId, curtime, checksum=md5(secret+curtime)} → 设备 token
//! 2. wss /v1/dispatch?param=base64({auth_id, llm_app?})，Authorization: Bearer {token}
//! 3. start → 上传数据 → end → 打印所有下行帧直至 finish

use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use md5::{Digest, Md5};
use serde_json::{json, Value};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, http::header, Message},
};

const AUDIO_CHUNK_BYTES: usize = 1280 * 4; // 160ms of 16k 16bit mono PCM
const AUDIO_CHUNK_PACE_MS: u64 = 40; // 4x realtime; blasting breaks server-side session init

#[derive(Debug, Clone)]
pub enum RequestInput {
    Text(String),
    /// 16k 16bit LE 单声道 PCM
    Audio(Vec<u8>),
}

#[derive(Debug, Clone)]
pub struct RequestOptions {
    /// 设备唯一标识（auth_id / deviceId）
    pub device_id: String,
    /// 多应用场景下指定应用 id（llm_app）
    pub llm_app: Option<String>,
}

#[derive(Debug, Clone)]
pub enum RequestEvent {
    /// 云端下发的 TEXT 帧（原样 JSON 字符串）
    Frame(String),
    /// 云端下发的 BINARY 帧（字节数）
    Binary(usize),
}

/// 用产品密钥换取设备接入 token（POST /v1/auth/tokens）。
pub async fn device_auth_token(
    api_base_url: &str,
    product_id: &str,
    product_secret: &str,
    device_id: &str,
) -> Result<String> {
    let url = ling_core::http_url(api_base_url, "/v1/auth/tokens")?;
    let curtime = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .context("系统时钟异常")?
        .as_secs();
    let checksum = format!(
        "{:x}",
        Md5::digest(format!("{product_secret}{curtime}").as_bytes())
    );

    let response = ling_core::client()?
        .post(url)
        .json(&json!({
            "productId": product_id,
            "deviceId": device_id,
            "curtime": curtime,
            "checksum": checksum,
        }))
        .send()
        .await
        .context("请求设备授权失败")?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        bail!(
            "设备授权失败：HTTP {status} {body}\n提示：若应用开启了强制白名单，请使用已导入的设备 ID（--device-id）"
        );
    }
    let value: Value = serde_json::from_str(&body).context("设备授权响应不是合法 JSON")?;
    value
        .get("token")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .with_context(|| format!("设备授权响应缺少 token：{body}"))
}

/// 对云端发起一次模拟请求，把所有链路返回帧交给 on_event。
pub async fn interaction_request(
    api_base_url: &str,
    product_id: &str,
    product_secret: &str,
    input: &RequestInput,
    opts: &RequestOptions,
    mut on_event: impl FnMut(RequestEvent),
) -> Result<()> {
    let token =
        device_auth_token(api_base_url, product_id, product_secret, &opts.device_id).await?;

    let mut param = json!({"auth_id": opts.device_id});
    if let Some(llm_app) = &opts.llm_app {
        param["llm_app"] = json!(llm_app);
    }
    let param = base64::engine::general_purpose::STANDARD.encode(param.to_string());

    let mut url = ling_core::ws_url(api_base_url, "/v1/dispatch")?;
    // 服务端不对 query 做 urldecode，base64 需原样拼接（不能百分号转义 = / +）
    url.set_query(Some(&format!("param={param}")));

    let mut request = url
        .as_str()
        .into_client_request()
        .context("构造链路 WebSocket 请求失败")?;
    request.headers_mut().insert(
        header::AUTHORIZATION,
        format!("Bearer {token}")
            .parse()
            .context("设备 token 含有非法字符")?,
    );

    let (mut ws, _) = connect_async(request)
        .await
        .context("端云链路 WebSocket 连接失败")?;

    // 等待 connected
    loop {
        match ws.next().await {
            Some(Ok(Message::Text(body))) => {
                let frame: Value = serde_json::from_str(&body).context("链路响应不是合法 JSON")?;
                let action = frame.get("action").and_then(Value::as_str);
                let failed = action == Some("error");
                let connected = action == Some("connected");
                on_event(RequestEvent::Frame(body));
                if connected {
                    break;
                }
                if failed {
                    bail!(
                        "链路连接失败：code={} {}",
                        frame.get("code").and_then(Value::as_str).unwrap_or("-"),
                        frame.get("desc").and_then(Value::as_str).unwrap_or("-")
                    );
                }
            }
            Some(Ok(Message::Close(frame))) => bail!("链路提前关闭：{frame:?}"),
            Some(Ok(_)) => continue,
            Some(Err(err)) => return Err(anyhow!(err).context("读取链路连接响应失败")),
            None => bail!("链路未返回连接响应"),
        }
    }

    // 创建会话
    let start = match input {
        RequestInput::Text(_) => json!({
            "action": "start",
            "params": {
                "data_type": "text",
                "features": ["nlu", "tts"],
            }
        }),
        RequestInput::Audio(_) => json!({
            "action": "start",
            "params": {
                "data_type": "audio",
                "aue": "raw",
                "features": ["nlu", "tts"],
            }
        }),
    };
    ws.send(Message::Text(start.to_string()))
        .await
        .context("发送会话创建指令失败")?;

    // 上传数据
    match input {
        RequestInput::Text(text) => {
            ws.send(Message::Binary(text.as_bytes().to_vec()))
                .await
                .context("上传文本数据失败")?;
        }
        RequestInput::Audio(audio) => {
            for chunk in audio.chunks(AUDIO_CHUNK_BYTES) {
                ws.send(Message::Binary(chunk.to_vec()))
                    .await
                    .context("上传音频数据失败")?;
                tokio::time::sleep(std::time::Duration::from_millis(AUDIO_CHUNK_PACE_MS)).await;
            }
        }
    }
    ws.send(Message::Text(json!({"action": "end"}).to_string()))
        .await
        .context("发送上传结束指令失败")?;

    // 接收所有帧直至 finish / 关闭
    while let Some(message) = ws.next().await {
        match message {
            Ok(Message::Text(body)) => {
                let frame = serde_json::from_str::<Value>(&body).ok();
                let action = frame
                    .as_ref()
                    .and_then(|frame| frame.get("action"))
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                let error_code = frame
                    .as_ref()
                    .and_then(|frame| frame.get("code"))
                    .map(|code| {
                        code.as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| code.to_string())
                    });
                let error_desc = frame
                    .as_ref()
                    .and_then(|frame| frame.get("desc"))
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                on_event(RequestEvent::Frame(body));
                match action.as_deref() {
                    Some("finish") => break,
                    Some("error") => {
                        bail!(
                            "链路请求失败：code={} {}",
                            error_code.unwrap_or_else(|| "-".into()),
                            error_desc.unwrap_or_else(|| "-".into())
                        );
                    }
                    _ => {}
                }
            }
            Ok(Message::Binary(bytes)) => on_event(RequestEvent::Binary(bytes.len())),
            Ok(Message::Close(_)) | Err(_) => break,
            Ok(_) => continue,
        }
    }

    let _ = ws.close(None).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_ws_url_uses_core_helper() {
        let url = ling_core::ws_url("https://api.listenai.com", "/v1/dispatch").unwrap();
        assert_eq!(url.as_str(), "wss://api.listenai.com/v1/dispatch");
    }

    #[test]
    fn checksum_matches_md5_of_secret_and_curtime() {
        let digest = format!("{:x}", Md5::digest("secret1717691503".as_bytes()));
        assert_eq!(digest.len(), 32);
    }
}
