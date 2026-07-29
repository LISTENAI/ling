use anyhow::{anyhow, bail, Context, Result};
use chrono::Local;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Map, Value};
use std::{path::Path, time::Duration};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{
        client::IntoClientRequest,
        http::{header, Request},
        Message,
    },
    WebSocketStream,
};
use unicode_width::UnicodeWidthStr;

const ASR_CHUNK_BYTES: usize = 1280 * 4; // 160ms of 16k 16bit mono PCM per frame
const ASR_CHUNK_PACE_MS: u64 = 40; // 4x realtime upload pacing; blasting breaks server-side init
const ASR_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const ASR_CONTROL_TIMEOUT: Duration = Duration::from_secs(15);
const ASR_SEND_TIMEOUT: Duration = Duration::from_secs(15);
const ASR_RESULT_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SupportedVcn {
    pub name: &'static str,
    pub value: &'static str,
}

pub const SUPPORTED_VCNS: &[SupportedVcn] = &[
    SupportedVcn {
        name: "聆玉昭pro",
        value: "x5_lingyuzhao_flow",
    },
    SupportedVcn {
        name: "聆小璇pro",
        value: "x5_lingxiaoxuan_flow",
    },
    SupportedVcn {
        name: "聆玉言pro",
        value: "x5_lingyuyan_flow",
    },
    SupportedVcn {
        name: "聆飞逸pro",
        value: "x5_lingfeiyi_flow",
    },
    SupportedVcn {
        name: "聆小玥pro",
        value: "x5_lingxiaoyue_flow",
    },
];

#[derive(Debug, Clone, Default)]
pub struct TtsOptions {
    pub vcn: Option<String>,
    /// mp3 | pcm
    pub format: Option<String>,
    /// 8000 | 16000 | 24000
    pub sample_rate: Option<u32>,
    /// 1-100
    pub speed: Option<u32>,
    /// 1-100
    pub volume: Option<u32>,
    /// 1-100
    pub pitch: Option<u32>,
    /// smartTTS 情感类别（emt）
    pub emotion: Option<String>,
    /// smartTTS 情感强度（-20..=20）
    pub emotion_scale: Option<i32>,
    /// smartTTS style
    pub style: Option<String>,
}

#[derive(Debug)]
pub struct TtsOutcome {
    pub url: String,
    /// (路径, 字节数)，仅当要求写文件时存在
    pub saved: Option<(String, u64)>,
}

/// 通过 wss /v1/tts/stream 合成一段文本，返回音频拉取 URL；
/// output 不为空时同时把音频下载到文件。
pub async fn tts(
    api_base_url: &str,
    api_key: &str,
    text: &str,
    opts: &TtsOptions,
    output: Option<&Path>,
) -> Result<TtsOutcome> {
    let text = text.trim();
    if text.is_empty() {
        bail!("合成文本不能为空");
    }

    let mut url = ling_core::ws_url(api_base_url, "/v1/tts/stream")?;
    url.query_pairs_mut().append_pair("api_key", api_key);

    let (mut ws, _) = connect_async(url.as_str())
        .await
        .context("TTS WebSocket 连接失败")?;

    ws.send(Message::Text(
        json!({
            "status": 0,
            "version": "v2",
            "payload": tts_init_payload(opts),
        })
        .to_string(),
    ))
    .await
    .context("发送 TTS 初始化请求失败")?;

    // 初始化响应：{"secure_url": ..., "url": ..., "error": 0, "message": "success"}
    let init = loop {
        match ws.next().await {
            Some(Ok(Message::Text(body))) => break body,
            Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_))) => continue,
            Some(Ok(Message::Binary(_))) => continue,
            Some(Ok(Message::Close(frame))) => {
                bail!("TTS 服务提前关闭连接：{frame:?}")
            }
            Some(Err(err)) => return Err(anyhow!(err).context("读取 TTS 初始化响应失败")),
            None => bail!("TTS 服务未返回初始化响应"),
        }
    };
    let init: Value = serde_json::from_str(&init).context("TTS 初始化响应不是合法 JSON")?;
    if init.get("error").and_then(Value::as_i64).unwrap_or(0) != 0 {
        bail!(
            "TTS 初始化失败：{}",
            init.get("message").and_then(Value::as_str).unwrap_or("-")
        );
    }
    let audio_url = init
        .get("secure_url")
        .or_else(|| init.get("url"))
        .and_then(Value::as_str)
        .context("TTS 初始化响应缺少音频 URL")?
        .to_owned();

    ws.send(Message::Text(
        json!({"status": 1, "payload": {"text": text}}).to_string(),
    ))
    .await
    .context("发送 TTS 合成文本失败")?;
    ws.send(Message::Text(json!({"status": 2}).to_string()))
        .await
        .context("发送 TTS 结束请求失败")?;

    // 服务端收到结束请求后会主动断开；这里读到关闭为止，忽略中间帧。
    while let Some(message) = ws.next().await {
        match message {
            Ok(Message::Close(_)) | Err(_) => break,
            _ => continue,
        }
    }

    let audio = fetch_audio(&audio_url).await?;
    let saved = match output {
        Some(path) => Some(save_audio(path, &audio)?),
        None => None,
    };

    Ok(TtsOutcome {
        url: audio_url,
        saved,
    })
}

fn tts_init_payload(opts: &TtsOptions) -> Value {
    let mut payload = Map::new();
    if let Some(vcn) = &opts.vcn {
        payload.insert("vcn".into(), json!(vcn));
    }
    if let Some(format) = &opts.format {
        payload.insert("format".into(), json!(format));
    }
    if let Some(rate) = opts.sample_rate {
        payload.insert("auf".into(), json!(format!("audio/L16;rate={rate}")));
    }
    if let Some(speed) = opts.speed {
        payload.insert("speed".into(), json!(speed));
    }
    if let Some(volume) = opts.volume {
        payload.insert("volume".into(), json!(volume));
    }
    if let Some(pitch) = opts.pitch {
        payload.insert("pitch".into(), json!(pitch));
    }
    if let Some(emotion) = &opts.emotion {
        payload.insert("emt".into(), json!(emotion));
    }
    if let Some(scale) = opts.emotion_scale {
        payload.insert("emotion_scale".into(), json!(scale));
    }
    if let Some(style) = &opts.style {
        payload.insert("style".into(), json!(style));
    }
    Value::Object(payload)
}

async fn fetch_audio(audio_url: &str) -> Result<Vec<u8>> {
    let response = ling_core::client()?
        .get(audio_url)
        .send()
        .await
        .context("下载 TTS 音频失败")?;
    let status = response.status();
    if !status.is_success() {
        bail!("下载 TTS 音频失败：HTTP {status}");
    }
    let bytes = response.bytes().await.context("读取 TTS 音频数据失败")?;
    validate_audio(&bytes)?;
    Ok(bytes.to_vec())
}

fn validate_audio(bytes: &[u8]) -> Result<()> {
    if bytes.is_empty() {
        bail!("TTS 未生成音频（0 bytes），当前发音人可能不支持所选参数");
    }
    Ok(())
}

fn save_audio(path: &Path, bytes: &[u8]) -> Result<(String, u64)> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建输出目录失败：{}", parent.display()))?;
    }
    std::fs::write(path, bytes).with_context(|| format!("写入音频文件失败：{}", path.display()))?;
    Ok((path.display().to_string(), bytes.len() as u64))
}

pub fn supported_vcns() -> Value {
    json!({
        "code": 0,
        "data": SUPPORTED_VCNS
            .iter()
            .map(|voice| json!({"name": voice.name, "value": voice.value}))
            .collect::<Vec<_>>()
    })
}

pub fn render_vcns(value: &Value) -> Result<String> {
    let list = value
        .get("data")
        .and_then(Value::as_array)
        .context("发音人列表响应缺少 data 数组")?;
    if list.is_empty() {
        return Ok("暂无可用发音人。".to_owned());
    }
    let rows = list
        .iter()
        .map(|item| {
            vec![
                item.get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("-")
                    .to_owned(),
                item.get("value")
                    .and_then(Value::as_str)
                    .unwrap_or("-")
                    .to_owned(),
            ]
        })
        .collect::<Vec<_>>();
    let mut output = render_table(&["名称", "VCN"], &rows);
    output.push_str(&format!("\n共 {} 个发音人。", rows.len()));
    Ok(output)
}

#[derive(Debug, Clone, Default)]
pub struct AsrOptions {
    /// vad 后端点（毫秒）
    pub vad_eos: Option<u32>,
    /// 识别引擎
    pub ent: Option<String>,
    /// 开启大模型 VAD
    pub asr_vad: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsrDirection {
    Upstream,
    Downstream,
}

impl AsrDirection {
    fn marker(self) -> &'static str {
        match self {
            Self::Upstream => "↑",
            Self::Downstream => "↓",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AsrEvent {
    Frame {
        direction: AsrDirection,
        body: String,
    },
    Binary {
        direction: AsrDirection,
        bytes: usize,
    },
    Partial {
        text: String,
    },
}

pub fn render_asr_verbose_event(event: &AsrEvent) -> Option<String> {
    let time = Local::now().format("%H:%M:%S%.3f");
    match event {
        AsrEvent::Frame { direction, body } => {
            Some(format!("[{time}] {} {body}", direction.marker()))
        }
        AsrEvent::Binary { direction, bytes } => Some(format!(
            "[{time}] {} BINARY {bytes} bytes",
            direction.marker()
        )),
        AsrEvent::Partial { .. } => None,
    }
}

fn asr_start_payload(opts: &AsrOptions) -> Value {
    let mut asr_properties = Map::new();
    if let Some(ent) = &opts.ent {
        asr_properties.insert("ent".into(), json!(ent));
    }

    let mut vad_properties = Map::new();
    if let Some(vad_eos) = opts.vad_eos {
        vad_properties.insert("vad_eos".into(), json!(vad_eos));
    }
    if opts.asr_vad {
        vad_properties.insert("vad_model".into(), json!("v2"));
    }

    json!({
        "action": "start",
        "params": {
            "data_type": "audio",
            "aue": "raw",
            "asr_properties": asr_properties,
            "vad_properties": vad_properties,
            "wake_properties": {
                "words": []
            },
            "vpr_properties": {}
        }
    })
}

fn asr_request(api_base_url: &str, api_key: &str) -> Result<Request<()>> {
    let url = ling_core::ws_url(api_base_url, "/v2/asr")?;
    let mut request = url
        .as_str()
        .into_client_request()
        .context("构造 ASR WebSocket 请求失败")?;
    request.headers_mut().insert(
        header::AUTHORIZATION,
        ling_core::bearer(api_key)
            .parse()
            .context("API Key 含有非法字符")?,
    );
    Ok(request)
}

fn asr_error(frame: &Value, stage: &str) -> anyhow::Error {
    let code = frame.get("code").map_or_else(
        || "-".to_owned(),
        |code| {
            code.as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| code.to_string())
        },
    );
    let desc = frame.get("desc").and_then(Value::as_str).unwrap_or("-");
    anyhow!("{stage}失败：code={code} {desc}")
}

async fn send_asr_text<S>(
    ws: &mut WebSocketStream<S>,
    body: String,
    on_event: &mut impl FnMut(AsrEvent),
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    on_event(AsrEvent::Frame {
        direction: AsrDirection::Upstream,
        body: body.clone(),
    });
    tokio::time::timeout(ASR_SEND_TIMEOUT, ws.send(Message::Text(body)))
        .await
        .context("发送 ASR 控制帧超时")?
        .context("发送 ASR 控制帧失败")
}

async fn wait_for_asr_action<S>(
    ws: &mut WebSocketStream<S>,
    expected: &str,
    stage: &str,
    on_event: &mut impl FnMut(AsrEvent),
) -> Result<Value>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    tokio::time::timeout(ASR_CONTROL_TIMEOUT, async {
        loop {
            match ws.next().await {
                Some(Ok(Message::Text(body))) => {
                    on_event(AsrEvent::Frame {
                        direction: AsrDirection::Downstream,
                        body: body.clone(),
                    });
                    let frame: Value =
                        serde_json::from_str(&body).context("ASR 响应不是合法 JSON")?;
                    match frame.get("action").and_then(Value::as_str) {
                        Some(action) if action == expected => return Ok(frame),
                        Some("error") => return Err(asr_error(&frame, stage)),
                        _ => continue,
                    }
                }
                Some(Ok(Message::Close(frame))) => {
                    bail!("{stage}前服务端关闭连接：{frame:?}")
                }
                Some(Ok(_)) => continue,
                Some(Err(err)) => {
                    return Err(anyhow!(err).context(format!("读取 ASR {stage}响应失败")))
                }
                None => bail!("{stage}前 ASR 连接已结束"),
            }
        }
    })
    .await
    .with_context(|| {
        format!(
            "等待 ASR {stage}超时（{} 秒）",
            ASR_CONTROL_TIMEOUT.as_secs()
        )
    })?
}

async fn receive_asr_result<S>(
    ws: &mut WebSocketStream<S>,
    on_event: &mut impl FnMut(AsrEvent),
) -> Result<String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    tokio::time::timeout(ASR_RESULT_TIMEOUT, async {
        loop {
            match ws.next().await {
                Some(Ok(Message::Text(body))) => {
                    on_event(AsrEvent::Frame {
                        direction: AsrDirection::Downstream,
                        body: body.clone(),
                    });
                    let frame: Value =
                        serde_json::from_str(&body).context("ASR 响应不是合法 JSON")?;
                    match frame.get("action").and_then(Value::as_str) {
                        Some("result") => {
                            let data = frame.get("data").cloned().unwrap_or(Value::Null);
                            if data.get("sub").and_then(Value::as_str) != Some("iat") {
                                continue;
                            }
                            let text = data
                                .get("text")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_owned();
                            if data
                                .get("is_last")
                                .and_then(Value::as_bool)
                                .unwrap_or(false)
                            {
                                return Ok(text);
                            }
                            on_event(AsrEvent::Partial { text });
                        }
                        Some("finish") => bail!("ASR 会话结束但未返回最终识别结果"),
                        Some("error") => return Err(asr_error(&frame, "ASR 识别")),
                        _ => continue,
                    }
                }
                Some(Ok(Message::Close(frame))) => {
                    bail!("ASR 服务在最终结果前关闭连接：{frame:?}")
                }
                Some(Ok(_)) => continue,
                Some(Err(err)) => return Err(anyhow!(err).context("读取 ASR 识别结果失败")),
                None => bail!("ASR 连接结束但未返回最终识别结果"),
            }
        }
    })
    .await
    .with_context(|| {
        format!(
            "等待 ASR 最终识别结果超时（{} 秒）",
            ASR_RESULT_TIMEOUT.as_secs()
        )
    })?
}

async fn run_asr_session<S>(
    ws: &mut WebSocketStream<S>,
    audio: &[u8],
    opts: &AsrOptions,
    on_event: &mut impl FnMut(AsrEvent),
) -> Result<String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    wait_for_asr_action(ws, "connected", "连接", on_event).await?;

    send_asr_text(ws, asr_start_payload(opts).to_string(), on_event).await?;
    wait_for_asr_action(ws, "started", "会话创建", on_event).await?;

    for chunk in audio.chunks(ASR_CHUNK_BYTES) {
        tokio::time::timeout(ASR_SEND_TIMEOUT, ws.send(Message::Binary(chunk.to_vec())))
            .await
            .context("上传 ASR 音频超时")?
            .context("上传音频数据失败")?;
        on_event(AsrEvent::Binary {
            direction: AsrDirection::Upstream,
            bytes: chunk.len(),
        });
        tokio::time::sleep(std::time::Duration::from_millis(ASR_CHUNK_PACE_MS)).await;
    }
    send_asr_text(ws, json!({"action": "end"}).to_string(), on_event).await?;

    receive_asr_result(ws, on_event).await
}

/// 通过 wss /v2/asr 识别一段 16k 16bit LE 单声道 PCM。
pub async fn asr(
    api_base_url: &str,
    api_key: &str,
    audio: &[u8],
    opts: &AsrOptions,
    mut on_event: impl FnMut(AsrEvent),
) -> Result<String> {
    let request = asr_request(api_base_url, api_key)?;
    let (mut ws, _) = tokio::time::timeout(ASR_CONNECT_TIMEOUT, connect_async(request))
        .await
        .with_context(|| {
            format!(
                "ASR WebSocket 连接超时（{} 秒）",
                ASR_CONNECT_TIMEOUT.as_secs()
            )
        })?
        .context("ASR WebSocket 连接失败")?;

    run_asr_session(&mut ws, audio, opts, &mut on_event).await
}

/// 读取音频文件。WAV 会校验为 16k 16bit LE 单声道并剥离头部；其余按 raw PCM 处理。
pub fn load_pcm_audio(path: &Path) -> Result<Vec<u8>> {
    let bytes =
        std::fs::read(path).with_context(|| format!("读取音频文件失败：{}", path.display()))?;
    let has_wav_header = bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE";
    let has_wav_extension = path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("wav"));
    if has_wav_header {
        extract_wav_pcm(&bytes)
    } else if has_wav_extension {
        bail!("WAV 文件头无效：{}", path.display())
    } else if bytes.len() % 2 != 0 {
        bail!("裸 PCM 文件不是完整的 16bit 采样：{}", path.display())
    } else {
        Ok(bytes)
    }
}

fn extract_wav_pcm(bytes: &[u8]) -> Result<Vec<u8>> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        bail!("WAV 文件头无效");
    }
    let riff_size = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    let riff_end = riff_size.checked_add(8).context("WAV RIFF 长度溢出")?;
    if riff_end > bytes.len() {
        bail!("WAV RIFF 块不完整");
    }

    let mut pos = 12usize;
    let mut format_ok = false;
    let mut data: Option<&[u8]> = None;

    while pos + 8 <= riff_end {
        let chunk_id = &bytes[pos..pos + 4];
        let chunk_size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let body_start = pos + 8;
        let body_end = body_start
            .checked_add(chunk_size)
            .context("WAV chunk 长度溢出")?;
        if body_end > riff_end {
            bail!("WAV {} 块不完整", String::from_utf8_lossy(chunk_id).trim());
        }

        match chunk_id {
            b"fmt " => {
                let body = &bytes[body_start..body_end];
                if body.len() < 16 {
                    bail!("WAV fmt 块不完整");
                }
                let audio_format = u16::from_le_bytes(body[0..2].try_into().unwrap());
                let channels = u16::from_le_bytes(body[2..4].try_into().unwrap());
                let sample_rate = u32::from_le_bytes(body[4..8].try_into().unwrap());
                let byte_rate = u32::from_le_bytes(body[8..12].try_into().unwrap());
                let block_align = u16::from_le_bytes(body[12..14].try_into().unwrap());
                let bits = u16::from_le_bytes(body[14..16].try_into().unwrap());
                if audio_format != 1
                    || channels != 1
                    || sample_rate != 16000
                    || bits != 16
                    || byte_rate != 32_000
                    || block_align != 2
                {
                    bail!(
                        "WAV 格式不符合要求（需 PCM 16kHz 16bit 单声道），实际：format={audio_format} channels={channels} rate={sample_rate} bits={bits} byte_rate={byte_rate} block_align={block_align}"
                    );
                }
                format_ok = true;
            }
            b"data" => {
                data = Some(&bytes[body_start..body_end]);
            }
            _ => {}
        }
        // 块按 2 字节对齐
        pos = body_start + chunk_size + (chunk_size & 1);
    }

    if !format_ok {
        bail!("WAV 文件缺少 fmt 块");
    }
    let data = data.context("WAV 文件缺少 data 块")?;
    if data.len() % 2 != 0 {
        bail!("WAV data 块不是完整的 16bit PCM 采样");
    }
    Ok(data.to_vec())
}

fn render_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut widths = headers
        .iter()
        .map(|header| UnicodeWidthStr::width(*header))
        .collect::<Vec<_>>();
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(UnicodeWidthStr::width(cell.as_str()));
        }
    }
    let border = |left: &str, join: &str, right: &str| {
        format!(
            "{}{}{}",
            left,
            widths
                .iter()
                .map(|width| "─".repeat(width + 2))
                .collect::<Vec<_>>()
                .join(join),
            right
        )
    };
    let row_line = |cells: &[String]| {
        format!(
            "│ {} │",
            cells
                .iter()
                .zip(widths.iter())
                .map(|(cell, width)| format!(
                    "{}{}",
                    cell,
                    " ".repeat(width - UnicodeWidthStr::width(cell.as_str()))
                ))
                .collect::<Vec<_>>()
                .join(" │ ")
        )
    };

    let mut output = String::new();
    output.push_str(&border("╭", "┬", "╮"));
    output.push('\n');
    output.push_str(&row_line(
        &headers.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
    ));
    output.push('\n');
    output.push_str(&border("├", "┼", "┤"));
    for row in rows {
        output.push('\n');
        output.push_str(&row_line(row));
    }
    output.push('\n');
    output.push_str(&border("╰", "┴", "╯"));
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_tungstenite::tungstenite::protocol::Role;

    fn wav_header(format: u16, channels: u16, rate: u32, bits: u16, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        out.extend_from_slice(b"WAVE");
        out.extend_from_slice(b"fmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&format.to_le_bytes());
        out.extend_from_slice(&channels.to_le_bytes());
        out.extend_from_slice(&rate.to_le_bytes());
        let byte_rate = rate * channels as u32 * bits as u32 / 8;
        out.extend_from_slice(&byte_rate.to_le_bytes());
        let block_align = channels * bits / 8;
        out.extend_from_slice(&block_align.to_le_bytes());
        out.extend_from_slice(&bits.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(data);
        out
    }

    #[test]
    fn extracts_pcm_from_valid_wav() {
        let pcm = vec![1u8, 2, 3, 4];
        let wav = wav_header(1, 1, 16000, 16, &pcm);
        assert_eq!(extract_wav_pcm(&wav).unwrap(), pcm);
    }

    #[test]
    fn rejects_wrong_sample_rate() {
        let wav = wav_header(1, 1, 44100, 16, &[0u8; 4]);
        let err = extract_wav_pcm(&wav).unwrap_err().to_string();
        assert!(err.contains("16kHz") || err.contains("rate=44100"));
    }

    #[test]
    fn rejects_stereo() {
        let wav = wav_header(1, 2, 16000, 16, &[0u8; 4]);
        assert!(extract_wav_pcm(&wav).is_err());
    }

    #[test]
    fn rejects_non_pcm_and_wrong_bit_depth() {
        let float_wav = wav_header(3, 1, 16000, 32, &[0u8; 8]);
        assert!(extract_wav_pcm(&float_wav).is_err());

        let pcm_24bit = wav_header(1, 1, 16000, 24, &[0u8; 6]);
        assert!(extract_wav_pcm(&pcm_24bit).is_err());
    }

    #[test]
    fn rejects_truncated_or_partial_wav_samples() {
        let mut truncated = wav_header(1, 1, 16000, 16, &[0u8; 4]);
        truncated.pop();
        assert!(extract_wav_pcm(&truncated).is_err());

        let partial_sample = wav_header(1, 1, 16000, 16, &[0u8; 3]);
        assert!(extract_wav_pcm(&partial_sample).is_err());
    }

    #[test]
    fn rejects_invalid_header_for_wav_extension() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "ling-asr-invalid-{}-{unique}.wav",
            std::process::id()
        ));
        std::fs::write(&path, [0u8; 32]).expect("write invalid WAV");
        let result = load_pcm_audio(&path);
        std::fs::remove_file(&path).expect("remove invalid WAV");
        assert!(result
            .expect_err("invalid WAV header must fail")
            .to_string()
            .contains("文件头无效"));
    }

    #[test]
    fn builds_v2_start_payload_from_asr_options() {
        let payload = asr_start_payload(&AsrOptions {
            vad_eos: Some(800),
            ent: Some("home-va".into()),
            asr_vad: true,
        });
        assert_eq!(payload["action"], "start");
        assert_eq!(payload["params"]["data_type"], "audio");
        assert_eq!(payload["params"]["aue"], "raw");
        assert_eq!(payload["params"]["asr_properties"]["ent"], "home-va");
        assert_eq!(payload["params"]["vad_properties"]["vad_eos"], 800);
        assert_eq!(payload["params"]["vad_properties"]["vad_model"], "v2");
        assert_eq!(payload["params"]["wake_properties"]["words"], json!([]));
    }

    #[test]
    fn builds_v2_asr_request_with_bearer_auth() {
        let request =
            asr_request("https://api.example.com", "test-key").expect("build ASR request");
        assert_eq!(request.uri().to_string(), "wss://api.example.com/v2/asr");
        assert_eq!(
            request
                .headers()
                .get("authorization")
                .and_then(|value| value.to_str().ok()),
            Some("Bearer test-key")
        );
    }

    #[tokio::test]
    async fn runs_complete_v2_asr_session() {
        let (client_io, server_io) = tokio::io::duplex(64 * 1024);
        let (mut client, mut ws) = tokio::join!(
            WebSocketStream::from_raw_socket(client_io, Role::Client, None),
            WebSocketStream::from_raw_socket(server_io, Role::Server, None),
        );
        let server = tokio::spawn(async move {
            ws.send(Message::Text(
                json!({
                    "action": "connected",
                    "code": "0",
                    "cid": "cid-test"
                })
                .to_string(),
            ))
            .await
            .expect("send connected");

            let start = ws.next().await.expect("start frame").expect("read start");
            let Message::Text(start) = start else {
                panic!("expected ASR start text frame");
            };
            let start: Value = serde_json::from_str(&start).expect("parse start");
            assert_eq!(start["action"], "start");
            assert_eq!(start["params"]["aue"], "raw");

            ws.send(Message::Text(
                json!({
                    "action": "started",
                    "code": "0",
                    "sid": "sid-test"
                })
                .to_string(),
            ))
            .await
            .expect("send started");

            let mut uploaded = Vec::new();
            loop {
                match ws.next().await.expect("client frame").expect("read client") {
                    Message::Binary(bytes) => uploaded.extend_from_slice(&bytes),
                    Message::Text(body) => {
                        let frame: Value = serde_json::from_str(&body).expect("parse end");
                        assert_eq!(frame["action"], "end");
                        break;
                    }
                    other => panic!("unexpected ASR client frame: {other:?}"),
                }
            }
            assert_eq!(uploaded, vec![7u8; 6_000]);

            ws.send(Message::Text(
                json!({
                    "action": "result",
                    "code": "0",
                    "data": {
                        "sub": "iat",
                        "is_last": false,
                        "text": "你好"
                    },
                    "sid": "sid-test"
                })
                .to_string(),
            ))
            .await
            .expect("send partial");
            ws.send(Message::Text(
                json!({
                    "action": "result",
                    "code": "0",
                    "data": {
                        "sub": "iat",
                        "is_last": true,
                        "text": "你好，世界"
                    },
                    "sid": "sid-test"
                })
                .to_string(),
            ))
            .await
            .expect("send final");
        });

        let mut events = Vec::new();
        let text = run_asr_session(
            &mut client,
            &[7u8; 6_000],
            &AsrOptions::default(),
            &mut |event| events.push(event),
        )
        .await
        .expect("complete ASR session");
        server.await.expect("mock ASR server");

        assert_eq!(text, "你好，世界");
        assert!(events.iter().any(|event| matches!(
            event,
            AsrEvent::Frame {
                direction: AsrDirection::Upstream,
                body
            } if body.contains("\"action\":\"start\"")
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            AsrEvent::Partial { text } if text == "你好"
        )));
    }

    #[test]
    fn tts_payload_includes_options() {
        let opts = TtsOptions {
            vcn: Some("x5_lingyuzhao_flow".into()),
            format: Some("pcm".into()),
            sample_rate: Some(16000),
            speed: Some(60),
            volume: Some(50),
            pitch: None,
            emotion: Some("cheerful".into()),
            emotion_scale: Some(10),
            style: None,
        };
        let payload = tts_init_payload(&opts);
        assert_eq!(payload["vcn"], "x5_lingyuzhao_flow");
        assert_eq!(payload["format"], "pcm");
        assert_eq!(payload["auf"], "audio/L16;rate=16000");
        assert_eq!(payload["speed"], 60);
        assert_eq!(payload["emt"], "cheerful");
        assert_eq!(payload["emotion_scale"], 10);
        assert!(payload.get("pitch").is_none());
    }

    #[test]
    fn rejects_empty_tts_audio() {
        let err = validate_audio(&[]).unwrap_err().to_string();
        assert!(err.contains("0 bytes"));
        assert!(err.contains("可能不支持所选参数"));
        validate_audio(&[1]).expect("non-empty audio should be accepted");
    }

    #[test]
    fn renders_vcn_table() {
        let value = supported_vcns();
        let out = render_vcns(&value).unwrap();
        for voice in SUPPORTED_VCNS {
            assert!(out.contains(voice.name));
            assert!(out.contains(voice.value));
        }
        assert!(out.contains("共 5 个发音人"));
        assert!(!out.contains("x2_chongchong"));
    }
}
