//! 端云链路模拟请求。
//!
//! 流程（参考文档「大模型端云交互链路协议」）：
//! 1. POST /v1/auth/tokens {productId, deviceId, curtime, checksum=md5(secret+curtime)} → 设备 token
//! 2. wss /v1/interaction?param=base64({auth_id,scene,mcp,type,...,llm_app?})，Authorization: Bearer {token}
//! 3. start → 上传数据 → 打印所有下行帧直至 finish

use crate::device_mcp;
use anyhow::{bail, Context, Result};
use base64::Engine;
use chrono::Local;
use futures_util::{SinkExt, StreamExt};
use md5::{Digest, Md5};
use serde_json::{json, Value};
use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, http::header, Message},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const PCM_BYTES_PER_SECOND: usize = 16_000 * 2;
const AUDIO_CHUNK_BYTES: usize = 2_560; // 80ms of 16k 16bit mono PCM
const LLM_WS_VERSION: &str = "2.0";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const SESSION_START_TIMEOUT: Duration = Duration::from_secs(15);
const SESSION_FINISH_TIMEOUT: Duration = Duration::from_secs(180);
const TEXT_STREAM_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone)]
pub enum RequestInput {
    Text(String),
    /// 16k 16bit LE 单声道 PCM
    Audio(Vec<u8>),
}

#[derive(Debug, Clone)]
pub struct RequestOptions {
    /// 设备鉴权使用的唯一标识（deviceId）
    pub device_id: String,
    /// 定向调试时覆盖应用 id（llm_app）
    pub llm_app: Option<String>,
}

#[derive(Debug, Clone)]
pub enum RequestDirection {
    Upstream,
    Downstream,
}

impl RequestDirection {
    fn marker(&self) -> &'static str {
        match self {
            Self::Upstream => "↑",
            Self::Downstream => "↓",
        }
    }
}

#[derive(Debug, Clone)]
pub enum RequestEvent {
    /// 云端下发的 TEXT 帧（原样 JSON 字符串）
    Frame {
        direction: RequestDirection,
        body: String,
    },
    /// 上行或下行 BINARY 帧。文本请求本身也通过 BINARY 帧上传。
    Binary {
        direction: RequestDirection,
        bytes: usize,
        text: Option<String>,
    },
}

/// 把一条端云交互事件渲染成人类友好的带时间戳事件。
pub fn render_event(event: &RequestEvent) -> String {
    let time = Local::now().format("%H:%M:%S%.3f").to_string();
    match event {
        RequestEvent::Frame { direction, body } => render_frame_at(direction, body, &time),
        RequestEvent::Binary {
            direction,
            bytes,
            text,
        } => render_binary_at(direction, *bytes, text.as_deref(), &time),
    }
}

/// 把一条端云交互事件渲染成带时间戳和方向的协议级明细。
pub fn render_verbose_event(event: &RequestEvent) -> String {
    let time = Local::now().format("%H:%M:%S%.3f");
    match event {
        RequestEvent::Frame { direction, body } => {
            format!("[{time}] {} {body}", direction.marker())
        }
        RequestEvent::Binary {
            direction,
            bytes,
            text,
        } => match text {
            Some(text) => format!(
                "[{time}] {} [binary {bytes} bytes] {}",
                direction.marker(),
                text.replace('\n', "\\n").replace('\r', "\\r")
            ),
            None => format!("[{time}] {} [binary {bytes} bytes]", direction.marker()),
        },
    }
}

/// 渲染 text_streaming 的累计回复文本。
pub fn render_reply_text(text: &str) -> String {
    let prefix = reply_prefix();
    format!("{prefix}{}", normalize_reply_text(text))
}

/// 把累计回复渲染成不超过指定显示宽度的单行预览，内容过长时保留最新尾部。
pub fn render_reply_preview(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    let prefix = reply_prefix();
    let text = normalize_reply_text(text);
    let full = format!("{prefix}{text}");
    if UnicodeWidthStr::width(full.as_str()) <= max_width {
        return full;
    }

    let ellipsis = "…";
    let fixed_width = UnicodeWidthStr::width(prefix.as_str()) + UnicodeWidthStr::width(ellipsis);
    if fixed_width >= max_width {
        return tail_with_width(&full, max_width);
    }
    let tail = tail_with_width(&text, max_width - fixed_width);
    format!("{prefix}{ellipsis}{tail}")
}

/// 渲染 text_streaming 读取失败的非致命警告。
pub fn render_reply_stream_error(error: &str) -> String {
    format!(
        "[{}] ↓ 回复文本读取失败：{error}",
        Local::now().format("%H:%M:%S%.3f")
    )
}

/// 把一条 text_streaming SSE 事件块压缩成单行。
pub fn render_verbose_sse_frame(frame: &str) -> String {
    let frame = frame
        .lines()
        .map(|line| line.trim_end_matches('\r'))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" | ");
    format!("[{}] ↓ SSE {frame}", Local::now().format("%H:%M:%S%.3f"),)
}

fn reply_prefix() -> String {
    format!("[{}] ↓ 回复：", Local::now().format("%H:%M:%S%.3f"))
}

fn normalize_reply_text(text: &str) -> String {
    text.replace('\n', "\\n").replace('\r', "\\r")
}

fn tail_with_width(text: &str, max_width: usize) -> String {
    let mut width = 0;
    let mut reversed = Vec::new();
    for character in text.chars().rev() {
        let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if width + character_width > max_width {
            break;
        }
        width += character_width;
        reversed.push(character);
    }
    reversed.into_iter().rev().collect()
}

/// 从一条 interaction JSON 帧中提取回复文本流 URL。
pub fn text_stream_url(frame: &str) -> Option<String> {
    let value = serde_json::from_str::<Value>(frame).ok()?;
    text_stream_url_from_value(&value).map(str::to_owned)
}

/// 从一条 interaction JSON 帧中提取 TTS URL（兼容 Base64 content）。
pub fn tts_url(frame: &str) -> Option<String> {
    let value = serde_json::from_str::<Value>(frame).ok()?;
    tts_url_from_value(&value)
}

/// 读取 text_streaming SSE，在每帧到达时回调原始事件块，并在文本增长时回调累计全文。
pub async fn stream_reply_text(
    url: &str,
    mut on_update: impl FnMut(&str),
    mut on_frame: impl FnMut(&str),
) -> Result<String> {
    let mut response = ling_core::client_with_timeout(TEXT_STREAM_TIMEOUT)?
        .get(url)
        .send()
        .await
        .context("连接 text_streaming 失败")?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        bail!("text_streaming 返回 HTTP {status} {body}");
    }

    let mut decoder = SseDecoder::default();
    let mut accumulated = String::new();
    let mut done = false;
    while let Some(chunk) = response
        .chunk()
        .await
        .context("读取 text_streaming 数据失败")?
    {
        for event in decoder.push(&chunk) {
            on_frame(&event.raw);
            if event.is_done() {
                done = true;
                break;
            }
            if let Some(text) = decode_stream_text(&event.data) {
                if merge_stream_text(&mut accumulated, &text) {
                    on_update(&accumulated);
                }
            }
        }
        if done {
            break;
        }
    }
    if !done {
        for event in decoder.finish() {
            on_frame(&event.raw);
            if !event.is_done() {
                if let Some(text) = decode_stream_text(&event.data) {
                    if merge_stream_text(&mut accumulated, &text) {
                        on_update(&accumulated);
                    }
                }
            }
        }
    }
    Ok(accumulated)
}

/// 下载 interaction 返回的 TTS URL 到本地文件。
pub async fn download_tts(url: &str, path: &Path) -> Result<u64> {
    let response = ling_core::client_with_timeout(TEXT_STREAM_TIMEOUT)?
        .get(url)
        .send()
        .await
        .context("下载 TTS 音频失败")?;
    let status = response.status();
    if !status.is_success() {
        bail!("下载 TTS 音频失败：HTTP {status}");
    }
    let bytes = response.bytes().await.context("读取 TTS 音频数据失败")?;
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建输出目录失败：{}", parent.display()))?;
    }
    std::fs::write(path, &bytes)
        .with_context(|| format!("写入 TTS 音频失败：{}", path.display()))?;
    Ok(bytes.len() as u64)
}

fn render_frame_at(direction: &RequestDirection, frame: &str, time: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(frame) else {
        return format!("[{time}] {} 文本帧：{frame}", direction.marker());
    };
    let action = value.get("action").and_then(Value::as_str).unwrap_or("-");
    let message = match (direction, action) {
        (RequestDirection::Upstream, "start") => {
            let data_type = string_at(&value, &["/params/data_type"]).unwrap_or("unknown");
            format!("创建会话，数据类型：{data_type}")
        }
        (RequestDirection::Upstream, "end") => "上传结束".to_owned(),
        (_, "connected") => "已连接".to_owned(),
        (_, "started") => match string_at(&value, &["/sid"]) {
            Some(sid) => format!("会话开始，sid: {sid}"),
            None => "会话开始".to_owned(),
        },
        (_, "result") => render_result(&value),
        (_, "mcp") => render_mcp(&value),
        (_, "finish") => "结束".to_owned(),
        (_, "error") => format!(
            "错误：code={} {}",
            scalar_at(&value, &["/code"]).unwrap_or_else(|| "-".to_owned()),
            string_at(&value, &["/desc"]).unwrap_or("-")
        ),
        (_, other) => {
            let detail = value
                .get("data")
                .filter(|data| !data.is_null() && **data != Value::String(String::new()))
                .map(compact)
                .unwrap_or_default();
            if detail.is_empty() {
                format!("事件：{other}")
            } else {
                format!("事件：{other} {detail}")
            }
        }
    };
    format!("[{time}] {} {message}", direction.marker())
}

fn render_binary_at(
    direction: &RequestDirection,
    bytes: usize,
    text: Option<&str>,
    time: &str,
) -> String {
    let message = match (direction, text) {
        (RequestDirection::Upstream, Some(text)) => {
            format!(
                "上传文本：{}",
                text.replace('\n', "\\n").replace('\r', "\\r")
            )
        }
        (RequestDirection::Upstream, None) => format!("上传音频：{bytes} bytes"),
        (RequestDirection::Downstream, _) => format!("音频数据：{bytes} bytes"),
    };
    format!("[{time}] {} {message}", direction.marker())
}

fn render_result(value: &Value) -> String {
    let Some(sub) = string_at(value, &["/data/sub"]) else {
        return format!(
            "结果：{}",
            value.get("data").map(compact).unwrap_or_default()
        );
    };
    match sub {
        "tts" => match tts_url_from_value(value) {
            Some(url) => format!("TTS URL：{url}"),
            None => "TTS 结果".to_owned(),
        },
        "iat" => match string_at(value, &["/data/text"]) {
            Some(text) => format!("识别结果：{text}"),
            None => "识别结果".to_owned(),
        },
        "nlp" => {
            if let Some(url) = text_stream_url_from_value(value) {
                return format!("回复文本 URL：{url}");
            }
            if let Some(text) = string_at(
                value,
                &[
                    "/data/nlp/text",
                    "/data/intent/answer/text",
                    "/data/answer/text",
                    "/data/text",
                ],
            ) {
                return format!("回复：{text}");
            }
            match string_at(value, &["/data/nlp_origin"]) {
                Some(origin) => format!("NLU 结果：{origin}"),
                None => "NLU 结果".to_owned(),
            }
        }
        "vad" => match string_at(value, &["/data/info"]) {
            Some(info) => format!("VAD：{info}"),
            None => "VAD 结果".to_owned(),
        },
        "setting" => "会话配置已下发".to_owned(),
        other => format!(
            "{other} 结果：{}",
            value.get("data").map(compact).unwrap_or_default()
        ),
    }
}

fn render_mcp(value: &Value) -> String {
    if let Some(method) = string_at(value, &["/data/method"]) {
        let name = string_at(value, &["/data/params/name"]);
        let params = value.pointer("/data/params");
        let label = match name {
            Some(name) => format!("{method} {name}"),
            None => method.to_owned(),
        };
        return match params {
            Some(params) if !params.is_null() => {
                format!("MCP 调用：{label} {}", compact_safe(params))
            }
            _ => format!("MCP 调用：{label}"),
        };
    }

    let method = string_at(value, &["/method"]).unwrap_or("unknown");
    match value.get("result") {
        Some(result) => format!("MCP 结果：{method} {}", compact_safe(result)),
        None => format!("MCP 事件：{method}"),
    }
}

fn compact_safe(value: &Value) -> String {
    compact(&redact_credentials(value))
}

fn redact_credentials(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| {
                    let normalized = key
                        .chars()
                        .filter(|character| character.is_ascii_alphanumeric())
                        .flat_map(char::to_lowercase)
                        .collect::<String>();
                    let value = if matches!(
                        normalized.as_str(),
                        "token"
                            | "accesstoken"
                            | "refreshtoken"
                            | "apikey"
                            | "authorization"
                            | "secret"
                            | "productsecret"
                    ) {
                        Value::String("[REDACTED]".to_owned())
                    } else {
                        redact_credentials(value)
                    };
                    (key.clone(), value)
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(redact_credentials).collect()),
        _ => value.clone(),
    }
}

fn string_at<'a>(value: &'a Value, pointers: &[&str]) -> Option<&'a str> {
    pointers
        .iter()
        .find_map(|pointer| value.pointer(pointer).and_then(Value::as_str))
        .filter(|text| !text.is_empty())
}

fn scalar_at(value: &Value, pointers: &[&str]) -> Option<String> {
    pointers.iter().find_map(|pointer| {
        value.pointer(pointer).and_then(|item| match item {
            Value::String(text) => Some(text.clone()),
            Value::Number(number) => Some(number.to_string()),
            _ => None,
        })
    })
}

fn decode_tts_content(content: &str) -> String {
    base64::engine::general_purpose::STANDARD
        .decode(content)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .filter(|decoded| !decoded.is_empty())
        .unwrap_or_else(|| content.to_owned())
}

fn tts_url_from_value(value: &Value) -> Option<String> {
    if string_at(value, &["/action"]) != Some("result")
        || string_at(value, &["/data/sub"]) != Some("tts")
    {
        return None;
    }
    string_at(value, &["/data/url", "/data/secure_url"])
        .map(str::to_owned)
        .or_else(|| {
            string_at(value, &["/data/content"])
                .map(decode_tts_content)
                .filter(|content| !content.is_empty())
        })
}

fn text_stream_url_from_value(value: &Value) -> Option<&str> {
    if string_at(value, &["/action"]) != Some("result")
        || string_at(value, &["/data/sub"]) != Some("nlp")
    {
        return None;
    }
    string_at(
        value,
        &[
            "/data/nlp/stream_url",
            "/data/stream_url",
            "/data/url",
            "/data/secure_url",
        ],
    )
}

fn decode_stream_text(data: &str) -> Option<String> {
    let data = data.trim_end_matches(['\r', '\n']);
    if data.is_empty() || data == "[DONE]" {
        return None;
    }
    match serde_json::from_str::<Value>(data) {
        Ok(Value::String(text)) => Some(text),
        Ok(value) => string_at(&value, &["/text", "/data/text", "/content"])
            .map(str::to_owned)
            .or_else(|| Some(data.to_owned())),
        Err(_) => Some(data.to_owned()),
    }
}

fn merge_stream_text(accumulated: &mut String, next: &str) -> bool {
    if next.is_empty() || accumulated.ends_with(next) {
        return false;
    }
    if next.starts_with(accumulated.as_str()) {
        *accumulated = next.to_owned();
    } else {
        accumulated.push_str(next);
    }
    true
}

fn compact(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
}

#[derive(Debug, PartialEq, Eq)]
struct SseEvent {
    event: Option<String>,
    data: String,
    raw: String,
}

impl SseEvent {
    fn is_done(&self) -> bool {
        self.event.as_deref() == Some("done") || self.data == "[DONE]"
    }
}

#[derive(Default)]
struct SseDecoder {
    buffer: Vec<u8>,
    event: Option<String>,
    data: Vec<String>,
    raw: Vec<String>,
}

impl SseDecoder {
    fn push(&mut self, chunk: &[u8]) -> Vec<SseEvent> {
        self.buffer.extend_from_slice(chunk);
        let mut events = Vec::new();
        while let Some(newline) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let mut line = self.buffer.drain(..=newline).collect::<Vec<_>>();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            self.handle_line(&String::from_utf8_lossy(&line), &mut events);
        }
        events
    }

    fn finish(&mut self) -> Vec<SseEvent> {
        let mut events = Vec::new();
        if !self.buffer.is_empty() {
            let line = std::mem::take(&mut self.buffer);
            self.handle_line(
                String::from_utf8_lossy(&line).trim_end_matches('\r'),
                &mut events,
            );
        }
        self.dispatch(&mut events);
        events
    }

    fn handle_line(&mut self, line: &str, events: &mut Vec<SseEvent>) {
        if line.is_empty() {
            self.dispatch(events);
        } else {
            self.raw.push(line.to_owned());
            if let Some(event) = line.strip_prefix("event:") {
                self.event = Some(event.trim().to_owned());
            } else if let Some(data) = line.strip_prefix("data:") {
                self.data
                    .push(data.strip_prefix(' ').unwrap_or(data).to_owned());
            }
        }
    }

    fn dispatch(&mut self, events: &mut Vec<SseEvent>) {
        let event = self.event.take();
        let data = std::mem::take(&mut self.data).join("\n");
        let raw_lines = std::mem::take(&mut self.raw);
        if !raw_lines.is_empty() {
            events.push(SseEvent {
                event,
                data,
                raw: format!("{}\n\n", raw_lines.join("\n")),
            });
        }
    }
}

fn connection_params(opts: &RequestOptions) -> Value {
    let mut params = json!({
        "auth_id": opts.device_id,
        "scene": "main",
        "mcp": true,
        "tool_protocol_version": "v2",
        "type": "fullduplex",
        "firmware_info": {
            "type": "ling-cli",
            "version": env!("CARGO_PKG_VERSION"),
        },
    });
    if let Some(llm_app) = &opts.llm_app {
        params["llm_app"] = json!(llm_app);
    }
    params
}

fn text_payload(text: &str) -> Vec<u8> {
    text.as_bytes().to_vec()
}

fn audio_chunk_duration(bytes: usize) -> Duration {
    Duration::from_secs_f64(bytes as f64 / PCM_BYTES_PER_SECOND as f64)
}

async fn respond_to_device_mcp<S>(
    ws: &mut WebSocketStream<S>,
    frame: &Value,
    on_event: &mut impl FnMut(RequestEvent),
) -> Result<bool>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let Some(response) = device_mcp::response_for_frame(frame) else {
        return Ok(false);
    };
    let body = response.to_string();
    ws.send(Message::Text(body.clone()))
        .await
        .context("回传模拟设备 MCP 结果失败")?;
    on_event(RequestEvent::Frame {
        direction: RequestDirection::Upstream,
        body,
    });
    Ok(true)
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
            "设备授权失败：HTTP {status} {body}\n提示：本次 Device ID 为 {device_id}；若应用开启了强制白名单，请先导入该 ID，或用 --device-id 指定已导入的设备 ID"
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

    let param = connection_params(opts);
    let param = base64::engine::general_purpose::STANDARD.encode(param.to_string());

    let mut url = ling_core::ws_url(api_base_url, "/v1/interaction")?;
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

    let (mut ws, _) = tokio::time::timeout(CONNECT_TIMEOUT, connect_async(request))
        .await
        .context("连接端云链路超时")?
        .context("端云链路 WebSocket 连接失败")?;

    // 等待 connected
    let connected_deadline = tokio::time::Instant::now() + CONNECT_TIMEOUT;
    loop {
        let message = tokio::time::timeout_at(connected_deadline, ws.next())
            .await
            .context("等待链路连接响应超时")?
            .context("链路未返回连接响应")?
            .context("读取链路连接响应失败")?;
        match message {
            Message::Text(body) => {
                let frame: Value = serde_json::from_str(&body).context("链路响应不是合法 JSON")?;
                let action = frame.get("action").and_then(Value::as_str);
                let failed = action == Some("error");
                let connected = action == Some("connected");
                on_event(RequestEvent::Frame {
                    direction: RequestDirection::Downstream,
                    body,
                });
                respond_to_device_mcp(&mut ws, &frame, &mut on_event).await?;
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
            Message::Binary(bytes) => on_event(RequestEvent::Binary {
                direction: RequestDirection::Downstream,
                bytes: bytes.len(),
                text: None,
            }),
            Message::Ping(bytes) => ws
                .send(Message::Pong(bytes))
                .await
                .context("响应链路 Ping 失败")?,
            Message::Close(frame) => bail!("链路在连接阶段提前关闭：{frame:?}"),
            _ => {}
        }
    }

    // 创建会话
    let start = start_frame(input);
    let start = start.to_string();
    ws.send(Message::Text(start.clone()))
        .await
        .context("发送会话创建指令失败")?;
    on_event(RequestEvent::Frame {
        direction: RequestDirection::Upstream,
        body: start,
    });

    // 文本会话遵循端侧时序：start 后立即上传一个二进制文本消息，不再发送
    // end。只发送正文长度，避免把 C 字符串的 NUL 终止符带入模型输入。
    // 音频会话仍等待 started 后按真实时长上传，并以 end 收尾。
    match input {
        RequestInput::Text(text) => {
            let payload = text_payload(text);
            ws.send(Message::Binary(payload.clone()))
                .await
                .context("上传文本数据失败")?;
            on_event(RequestEvent::Binary {
                direction: RequestDirection::Upstream,
                bytes: payload.len(),
                text: Some(text.clone()),
            });
        }
        RequestInput::Audio(audio) => {
            let started_deadline = tokio::time::Instant::now() + SESSION_START_TIMEOUT;
            loop {
                let message = tokio::time::timeout_at(started_deadline, ws.next())
                    .await
                    .context("等待会话开始响应超时")?
                    .context("链路未返回会话开始响应")?
                    .context("等待会话开始响应失败")?;
                match message {
                    Message::Text(body) => {
                        let frame: Value =
                            serde_json::from_str(&body).context("链路响应不是合法 JSON")?;
                        let action = frame.get("action").and_then(Value::as_str);
                        let failed = action == Some("error");
                        let started = action == Some("started");
                        on_event(RequestEvent::Frame {
                            direction: RequestDirection::Downstream,
                            body,
                        });
                        respond_to_device_mcp(&mut ws, &frame, &mut on_event).await?;
                        if started {
                            break;
                        }
                        if failed {
                            bail!(
                                "创建会话失败：code={} {}",
                                scalar_at(&frame, &["/code"]).unwrap_or_else(|| "-".into()),
                                string_at(&frame, &["/desc"]).unwrap_or("-")
                            );
                        }
                        if action == Some("finish") {
                            bail!("服务端在音频上传前结束了请求");
                        }
                    }
                    Message::Binary(bytes) => on_event(RequestEvent::Binary {
                        direction: RequestDirection::Downstream,
                        bytes: bytes.len(),
                        text: None,
                    }),
                    Message::Ping(bytes) => ws
                        .send(Message::Pong(bytes))
                        .await
                        .context("响应链路 Ping 失败")?,
                    Message::Close(frame) => {
                        bail!("链路在会话开始前关闭：{frame:?}")
                    }
                    _ => {}
                }
            }

            for chunk in audio.chunks(AUDIO_CHUNK_BYTES) {
                ws.send(Message::Binary(chunk.to_vec()))
                    .await
                    .context("上传音频数据失败")?;
                on_event(RequestEvent::Binary {
                    direction: RequestDirection::Upstream,
                    bytes: chunk.len(),
                    text: None,
                });
                tokio::time::sleep(audio_chunk_duration(chunk.len())).await;
            }
            let end = json!({"action": "end"}).to_string();
            ws.send(Message::Text(end.clone()))
                .await
                .context("发送上传结束指令失败")?;
            on_event(RequestEvent::Frame {
                direction: RequestDirection::Upstream,
                body: end,
            });
        }
    }

    // 接收所有帧直至明确的 finish。异常关闭和读取错误不能伪装成成功。
    let finish_deadline = tokio::time::Instant::now() + SESSION_FINISH_TIMEOUT;
    loop {
        let message = tokio::time::timeout_at(finish_deadline, ws.next())
            .await
            .context("等待会话结束超时")?
            .context("链路在返回 finish 前结束")?
            .context("读取端云链路响应失败")?;
        match message {
            Message::Text(body) => {
                let frame: Value = serde_json::from_str(&body).context("链路响应不是合法 JSON")?;
                let action = frame.get("action").and_then(Value::as_str);
                on_event(RequestEvent::Frame {
                    direction: RequestDirection::Downstream,
                    body,
                });
                respond_to_device_mcp(&mut ws, &frame, &mut on_event).await?;
                if action == Some("finish") {
                    break;
                }
                if action == Some("error") {
                    bail!(
                        "链路请求失败：code={} {}",
                        scalar_at(&frame, &["/code"]).unwrap_or_else(|| "-".into()),
                        string_at(&frame, &["/desc"]).unwrap_or("-")
                    );
                }
            }
            Message::Binary(bytes) => on_event(RequestEvent::Binary {
                direction: RequestDirection::Downstream,
                bytes: bytes.len(),
                text: None,
            }),
            Message::Ping(bytes) => ws
                .send(Message::Pong(bytes))
                .await
                .context("响应链路 Ping 失败")?,
            Message::Close(frame) => {
                bail!("链路在返回 finish 前关闭：{frame:?}")
            }
            _ => {}
        }
    }

    let _ = ws.close(None).await;
    Ok(())
}

fn start_frame(input: &RequestInput) -> Value {
    let mut params = match input {
        RequestInput::Text(_) => json!({
            "data_type": "text",
            "features": ["nlu", "tts"],
        }),
        RequestInput::Audio(_) => json!({
            "data_type": "audio",
            "aue": "raw",
            "features": ["nlu", "tts"],
        }),
    };
    params["nlu_properties"] = json!({
        "custom": {},
        "llm_ws_version": LLM_WS_VERSION,
    });
    json!({"action": "start", "params": params})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interaction_ws_url_uses_core_helper() {
        let url = ling_core::ws_url("https://api.listenai.com", "/v1/interaction").unwrap();
        assert_eq!(url.as_str(), "wss://api.listenai.com/v1/interaction");
    }

    #[test]
    fn checksum_matches_md5_of_secret_and_curtime() {
        let digest = format!("{:x}", Md5::digest("secret1717691503".as_bytes()));
        assert_eq!(digest.len(), 32);
    }

    #[test]
    fn connection_params_match_the_device_protocol() {
        let opts = RequestOptions {
            device_id: "ling-cli".to_owned(),
            llm_app: None,
        };
        let params = connection_params(&opts);

        assert_eq!(params["scene"], "main");
        assert_eq!(params["mcp"], true);
        assert_eq!(params["tool_protocol_version"], "v2");
        assert_eq!(params["type"], "fullduplex");
        assert_eq!(params["firmware_info"]["type"], "ling-cli");
        assert_eq!(params["auth_id"], "ling-cli");
        assert!(params.get("llm_app").is_none());
    }

    #[test]
    fn explicit_app_id_is_added_as_a_debug_override() {
        let opts = RequestOptions {
            device_id: "ling-cli".to_owned(),
            llm_app: Some("app-1".to_owned()),
        };

        assert_eq!(connection_params(&opts)["llm_app"], "app-1");
    }

    #[test]
    fn start_frames_keep_device_properties_and_force_llm_websocket_v2() {
        for input in [
            RequestInput::Text("hello".to_owned()),
            RequestInput::Audio(vec![0, 1]),
        ] {
            let frame = start_frame(&input);
            assert_eq!(
                frame.pointer("/params/nlu_properties/custom"),
                Some(&json!({}))
            );
            assert_eq!(
                frame.pointer("/params/nlu_properties/llm_ws_version"),
                Some(&json!("2.0"))
            );
        }
    }

    #[test]
    fn audio_chunks_are_paced_at_pcm_realtime() {
        assert_eq!(
            audio_chunk_duration(AUDIO_CHUNK_BYTES),
            Duration::from_millis(80)
        );
    }

    #[test]
    fn text_payload_does_not_send_the_c_string_terminator() {
        assert_eq!(text_payload("你好"), b"\xe4\xbd\xa0\xe5\xa5\xbd");
    }

    #[test]
    fn renders_core_interaction_frames_for_humans() {
        assert_eq!(
            render_frame_at(
                &RequestDirection::Downstream,
                r#"{"action":"connected","code":"0","desc":"success"}"#,
                "16:42:01.123"
            ),
            "[16:42:01.123] ↓ 已连接"
        );
        assert_eq!(
            render_frame_at(
                &RequestDirection::Downstream,
                r#"{"action":"started","sid":"sid-123","code":"0"}"#,
                "16:42:02.456"
            ),
            "[16:42:02.456] ↓ 会话开始，sid: sid-123"
        );
        assert_eq!(
            render_frame_at(
                &RequestDirection::Downstream,
                r#"{"action":"finish","sid":"sid-123"}"#,
                "16:42:05.000"
            ),
            "[16:42:05.000] ↓ 结束"
        );
        assert_eq!(
            render_frame_at(
                &RequestDirection::Upstream,
                r#"{"action":"start","params":{"data_type":"text"}}"#,
                "16:42:02.000"
            ),
            "[16:42:02.000] ↑ 创建会话，数据类型：text"
        );
        assert_eq!(
            render_frame_at(
                &RequestDirection::Upstream,
                r#"{"action":"end"}"#,
                "16:42:02.500"
            ),
            "[16:42:02.500] ↑ 上传结束"
        );
    }

    #[test]
    fn renders_tts_and_nlp_results_for_humans() {
        let encoded_url =
            base64::engine::general_purpose::STANDARD.encode("https://example.com/audio.mp3");
        assert_eq!(
            render_frame_at(
                &RequestDirection::Downstream,
                &json!({
                    "action": "result",
                    "data": {"sub": "tts", "content": encoded_url}
                })
                .to_string(),
                "16:42:03.000"
            ),
            "[16:42:03.000] ↓ TTS URL：https://example.com/audio.mp3"
        );
        assert_eq!(
            render_frame_at(
                &RequestDirection::Downstream,
                r#"{"action":"result","data":{"sub":"nlp","nlp_origin":"reply_text","nlp":{"stream_url":"https://example.com/text"}}}"#,
                "16:42:03.500"
            ),
            "[16:42:03.500] ↓ 回复文本 URL：https://example.com/text"
        );
        assert_eq!(
            render_frame_at(
                &RequestDirection::Downstream,
                r#"{"action":"result","data":{"sub":"nlp","nlp_origin":"reply_text","nlp":{"text":"pong"}}}"#,
                "16:42:03.750"
            ),
            "[16:42:03.750] ↓ 回复：pong"
        );
        assert_eq!(
            render_frame_at(
                &RequestDirection::Downstream,
                r#"{"action":"result","data":{"sub":"setting","theme":{"frontend":{}}}}"#,
                "16:42:03.900"
            ),
            "[16:42:03.900] ↓ 会话配置已下发"
        );
    }

    #[test]
    fn renders_mcp_calls_and_results_for_humans() {
        assert_eq!(
            render_frame_at(
                &RequestDirection::Downstream,
                r#"{"action":"mcp","data":{"method":"tools/call","id":"1","params":{"name":"set_volume","arguments":{"value":50}}}}"#,
                "16:42:04.000"
            ),
            r#"[16:42:04.000] ↓ MCP 调用：tools/call set_volume {"arguments":{"value":50},"name":"set_volume"}"#
        );
        assert_eq!(
            render_frame_at(
                &RequestDirection::Upstream,
                r#"{"action":"mcp","method":"tools/call","id":"1","result":{"content":[{"type":"text","text":"ok"}],"isError":false}}"#,
                "16:42:04.500"
            ),
            r#"[16:42:04.500] ↑ MCP 结果：tools/call {"content":[{"text":"ok","type":"text"}],"isError":false}"#
        );
    }

    #[test]
    fn human_mcp_output_redacts_credentials_but_verbose_keeps_raw_frames() {
        let body = json!({
            "action": "mcp",
            "data": {
                "method": "initialize",
                "params": {
                    "capabilities": {
                        "vision": {
                            "token": "short-lived-jwt",
                            "url": "https://example.com/vision"
                        }
                    }
                }
            }
        })
        .to_string();
        let event = RequestEvent::Frame {
            direction: RequestDirection::Downstream,
            body: body.clone(),
        };

        let human = render_frame_at(&RequestDirection::Downstream, &body, "16:42:04.750");
        assert!(human.contains(r#""token":"[REDACTED]""#));
        assert!(!human.contains("short-lived-jwt"));
        assert!(render_verbose_event(&event).contains("short-lived-jwt"));
    }

    #[test]
    fn renders_binary_events_in_human_and_verbose_modes() {
        let event = RequestEvent::Binary {
            direction: RequestDirection::Upstream,
            bytes: 4,
            text: Some("ping".to_owned()),
        };
        assert_eq!(
            render_binary_at(&RequestDirection::Upstream, 4, Some("ping"), "16:42:02.250"),
            "[16:42:02.250] ↑ 上传文本：ping"
        );
        assert!(render_verbose_event(&event).contains("↑ [binary 4 bytes] ping"));
    }

    #[test]
    fn sse_decoder_handles_split_chunks_and_done_event() {
        let mut decoder = SseDecoder::default();
        assert!(decoder.push(b"event: data\ndata: \xe4\xbd").is_empty());
        assert_eq!(
            decoder.push(b"\xa0\xe5\xa5\xbd\n\nevent: data\ndata: \xe4\xb8\x96\xe7\x95\x8c\n\n"),
            vec![
                SseEvent {
                    event: Some("data".to_owned()),
                    data: "你好".to_owned(),
                    raw: "event: data\ndata: 你好\n\n".to_owned(),
                },
                SseEvent {
                    event: Some("data".to_owned()),
                    data: "世界".to_owned(),
                    raw: "event: data\ndata: 世界\n\n".to_owned(),
                }
            ]
        );
        assert_eq!(
            decoder.push(b"event: done\n\n"),
            vec![SseEvent {
                event: Some("done".to_owned()),
                data: String::new(),
                raw: "event: done\n\n".to_owned(),
            }]
        );
        assert!(decoder
            .push(b"event: done\n\n")
            .first()
            .is_some_and(SseEvent::is_done));
    }

    #[test]
    fn verbose_sse_frame_is_rendered_on_one_line() {
        let rendered = render_verbose_sse_frame("event: data\r\ndata: 你好\n\n");

        assert_eq!(rendered.lines().count(), 1);
        assert!(rendered.contains("↓ SSE event: data | data: 你好"));
    }

    #[test]
    fn stream_text_accumulates_deltas_and_accepts_cumulative_updates() {
        let mut text = String::new();
        assert!(merge_stream_text(&mut text, "今天天气"));
        assert_eq!(text, "今天天气");
        assert!(merge_stream_text(&mut text, "很好"));
        assert_eq!(text, "今天天气很好");
        assert!(merge_stream_text(&mut text, "今天天气很好。"));
        assert_eq!(text, "今天天气很好。");
        assert!(!merge_stream_text(&mut text, "。"));
    }

    #[test]
    fn reply_preview_stays_on_one_terminal_line_and_keeps_the_tail() {
        let preview = render_reply_preview(
            "这是一个很长的回复，终端预览应该始终保留最新的结尾部分。",
            36,
        );

        assert!(UnicodeWidthStr::width(preview.as_str()) <= 36);
        assert!(preview.contains('…'));
        assert!(preview.ends_with("结尾部分。"));
        assert!(!preview.contains('\n'));
    }

    #[test]
    fn extracts_text_and_tts_urls_from_result_frames() {
        let text_frame = json!({
            "action": "result",
            "data": {
                "sub": "nlp",
                "nlp": {"stream_url": "https://example.com/text"}
            }
        })
        .to_string();
        assert_eq!(
            text_stream_url(&text_frame).as_deref(),
            Some("https://example.com/text")
        );

        let tts_frame = json!({
            "action": "result",
            "data": {
                "sub": "tts",
                "content": base64::engine::general_purpose::STANDARD
                    .encode("https://example.com/audio.mp3")
            }
        })
        .to_string();
        assert_eq!(
            tts_url(&tts_frame).as_deref(),
            Some("https://example.com/audio.mp3")
        );
    }
}
