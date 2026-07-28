use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Map, Value};
use std::path::Path;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, http::header, Message},
};
use unicode_width::UnicodeWidthStr;

const ASR_CHUNK_BYTES: usize = 1280 * 4; // 160ms of 16k 16bit mono PCM per frame
const ASR_CHUNK_PACE_MS: u64 = 40; // 4x realtime upload pacing; blasting breaks server-side init

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

/// 通过 wss /v2/asr（云云对接，Bearer API Key）识别一段 16k 16bit LE 单声道 PCM。
/// on_partial 用于输出中间识别结果；返回最终识别文本。
pub async fn asr(
    api_base_url: &str,
    api_key: &str,
    audio: &[u8],
    opts: &AsrOptions,
    mut on_partial: impl FnMut(&str),
) -> Result<String> {
    let mut param = Map::new();
    param.insert("aue".into(), json!("raw"));
    if let Some(vad_eos) = opts.vad_eos {
        param.insert("vad_eos".into(), json!(vad_eos));
    }
    if let Some(ent) = &opts.ent {
        param.insert("ent".into(), json!(ent));
    }
    if opts.asr_vad {
        param.insert("asr_vad".into(), json!("1"));
    }
    let param = base64::engine::general_purpose::STANDARD.encode(Value::Object(param).to_string());

    let mut url = ling_core::ws_url(api_base_url, "/v2/asr")?;
    // 服务端不对 query 做 urldecode，base64 需原样拼接（不能百分号转义 = / +）
    url.set_query(Some(&format!("param={param}")));

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

    let (mut ws, _) = connect_async(request)
        .await
        .context("ASR WebSocket 连接失败")?;

    // 等待 connected 帧
    loop {
        match ws.next().await {
            Some(Ok(Message::Text(body))) => {
                let frame: Value = serde_json::from_str(&body).context("ASR 响应不是合法 JSON")?;
                match frame.get("action").and_then(Value::as_str) {
                    Some("connected") => break,
                    Some("error") => bail!(
                        "ASR 连接失败：code={} {}",
                        frame.get("code").and_then(Value::as_str).unwrap_or("-"),
                        frame.get("desc").and_then(Value::as_str).unwrap_or("-")
                    ),
                    _ => continue,
                }
            }
            Some(Ok(Message::Close(frame))) => bail!("ASR 服务提前关闭连接：{frame:?}"),
            Some(Ok(_)) => continue,
            Some(Err(err)) => return Err(anyhow!(err).context("读取 ASR 连接响应失败")),
            None => bail!("ASR 服务未返回连接响应"),
        }
    }

    for chunk in audio.chunks(ASR_CHUNK_BYTES) {
        ws.send(Message::Binary(chunk.to_vec()))
            .await
            .context("上传音频数据失败")?;
        tokio::time::sleep(std::time::Duration::from_millis(ASR_CHUNK_PACE_MS)).await;
    }
    ws.send(Message::Text(json!({"action": "end"}).to_string()))
        .await
        .context("发送上传结束指令失败")?;

    let mut final_text: Option<String> = None;
    while let Some(message) = ws.next().await {
        let body = match message {
            Ok(Message::Text(body)) => body,
            Ok(Message::Close(_)) => break,
            Ok(_) => continue,
            Err(_) => break,
        };
        let frame: Value = match serde_json::from_str(&body) {
            Ok(frame) => frame,
            Err(_) => continue,
        };
        match frame.get("action").and_then(Value::as_str) {
            Some("result") => {
                let data = frame.get("data").cloned().unwrap_or(Value::Null);
                if data.get("sub").and_then(Value::as_str) == Some("iat") {
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
                        // 最终结果即会话结束；部分环境不会及时下发 finish/关闭帧，直接返回
                        final_text = Some(text);
                        break;
                    } else {
                        on_partial(&text);
                    }
                }
            }
            Some("finish") => break,
            Some("error") => bail!(
                "ASR 识别失败：code={} {}",
                frame
                    .get("code")
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "-".into()),
                frame.get("desc").and_then(Value::as_str).unwrap_or("-")
            ),
            _ => continue,
        }
    }

    final_text.context("未收到最终识别结果")
}

/// 读取音频文件。WAV 会校验为 16k 16bit LE 单声道并剥离头部；其余按 raw PCM 处理。
pub fn load_pcm_audio(path: &Path) -> Result<Vec<u8>> {
    let bytes =
        std::fs::read(path).with_context(|| format!("读取音频文件失败：{}", path.display()))?;
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
        extract_wav_pcm(&bytes)
    } else {
        Ok(bytes)
    }
}

fn extract_wav_pcm(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut pos = 12usize;
    let mut format_ok = false;
    let mut data: Option<&[u8]> = None;

    while pos + 8 <= bytes.len() {
        let chunk_id = &bytes[pos..pos + 4];
        let chunk_size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let body_start = pos + 8;
        let body_end = (body_start + chunk_size).min(bytes.len());

        match chunk_id {
            b"fmt " => {
                let body = &bytes[body_start..body_end];
                if body.len() < 16 {
                    bail!("WAV fmt 块不完整");
                }
                let audio_format = u16::from_le_bytes(body[0..2].try_into().unwrap());
                let channels = u16::from_le_bytes(body[2..4].try_into().unwrap());
                let sample_rate = u32::from_le_bytes(body[4..8].try_into().unwrap());
                let bits = u16::from_le_bytes(body[14..16].try_into().unwrap());
                if audio_format != 1 || channels != 1 || sample_rate != 16000 || bits != 16 {
                    bail!(
                        "WAV 格式不符合要求（需 PCM 16kHz 16bit 单声道），实际：format={audio_format} channels={channels} rate={sample_rate} bits={bits}"
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
