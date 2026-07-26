//! 请求记录查询（POST /v1/requests）与按 SID 追查。

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, Local, SecondsFormat, TimeZone, Utc};

use serde_json::{json, Value};
use std::time::Duration as StdDuration;

const PAGE_LIMIT: u32 = 50;
const MAX_PAGES: u32 = 20;
const REQUEST_TIMEOUT: StdDuration = StdDuration::from_secs(15);

/// 按 SID 直接查询 Agent 执行日志。旧服务端没有该路由时返回 None。
pub async fn query_agent_logs(
    api_base_url: &str,
    api_key: &str,
    sid: &str,
) -> Result<Option<Value>> {
    let mut url = ling_core::http_url(api_base_url, "/v1/log/agent")?;
    url.query_pairs_mut().append_pair("sid", sid);
    let response = ling_core::client_with_timeout(REQUEST_TIMEOUT)?
        .get(url)
        .header("authorization", ling_core::bearer(api_key))
        .send()
        .await
        .context("按 SID 查询 Agent 日志失败")?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        bail!("按 SID 查询 Agent 日志失败：HTTP {status} {body}");
    }
    let value = serde_json::from_str(&body).context("Agent 日志响应不是合法 JSON")?;
    Ok(Some(value))
}

/// 查询一页请求记录。start/end 为 RFC3339 UTC 字符串。
pub async fn query_requests(
    api_base_url: &str,
    api_key: &str,
    start: &str,
    end: &str,
    page: u32,
    limit: u32,
) -> Result<Value> {
    let url = ling_core::http_url(api_base_url, "/v1/requests")?;
    let response = ling_core::client_with_timeout(REQUEST_TIMEOUT)?
        .post(url)
        .header("authorization", ling_core::bearer(api_key))
        .json(&json!({
            "timeFilter": {"start": start, "end": end},
            "page": page,
            "limit": limit,
        }))
        .send()
        .await
        .context("查询请求记录失败")?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        bail!("查询请求记录失败：HTTP {status} {body}");
    }
    serde_json::from_str(&body).context("请求记录响应不是合法 JSON")
}

/// Render the structured Agent log stream as a concise timeline. Verbose mode
/// appends every original log entry as one compact line.
pub fn render_agent_trace(value: &Value, requested_sid: &str, verbose: bool) -> Result<String> {
    let logs = value
        .get("data")
        .and_then(Value::as_array)
        .context("Agent 日志响应缺少 data 数组")?;

    let parsed = logs.iter().map(parse_agent_log).collect::<Vec<_>>();
    let mut summary = AgentTraceSummary::default();
    let mut timeline = Vec::new();
    let mut model_call_count = 0_u64;

    for log in &parsed {
        summary.observe(log);
        if let Some(line) = agent_timeline_line(log, &mut model_call_count) {
            timeline.push(line);
        }
    }

    if timeline.is_empty() {
        timeline.push("[--:--:--.---] 会话日志已返回，但没有可识别的概览事件".to_owned());
    }

    let mut output = timeline.join("\n");
    output.push_str("\n\n");
    output.push_str(&summary.render(requested_sid));

    if value
        .get("truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        output.push_str("\n- 日志：服务端结果已截断");
    }

    if verbose {
        output.push_str("\n\n详细步骤：");
        for log in &parsed {
            output.push('\n');
            output.push_str(&render_verbose_agent_log(log));
        }
    } else {
        output.push_str("\n\n使用 --verbose 查看全部步骤，--json 输出服务端原始日志。");
    }

    Ok(output)
}

#[derive(Debug)]
struct ParsedAgentLog<'a> {
    timestamp: &'a str,
    content: &'a str,
    payload: Option<Value>,
}

fn parse_agent_log(log: &Value) -> ParsedAgentLog<'_> {
    let timestamp = log.get("timestamp").and_then(Value::as_str).unwrap_or("");
    let content = log.get("content").and_then(Value::as_str).unwrap_or("");
    let payload = serde_json::from_str::<Value>(content).ok();
    ParsedAgentLog {
        timestamp,
        content,
        payload,
    }
}

#[derive(Debug, Default)]
struct AgentTraceSummary {
    sid: Option<String>,
    device_id: Option<String>,
    product_id: Option<String>,
    agent_version: Option<String>,
    models: Vec<String>,
    prompt_tokens: u64,
    completion_tokens: u64,
    cached_prompt_tokens: u64,
    model_calls: u64,
    elapsed_ms: Option<u64>,
}

impl AgentTraceSummary {
    fn observe(&mut self, log: &ParsedAgentLog<'_>) {
        let Some(payload) = log.payload.as_ref() else {
            return;
        };

        fill_first(&mut self.sid, payload, "sid");
        fill_first(&mut self.device_id, payload, "deviceId");
        fill_first(&mut self.product_id, payload, "productId");

        let message = string_field(payload, "message").unwrap_or_default();
        if message.contains("agent bundle resolved") {
            fill_first(&mut self.agent_version, payload, "version");
        }

        match string_field(payload, "event") {
            Some("model.started") => {
                self.model_calls += 1;
                if let Some(model) = string_field(payload, "model") {
                    if !self.models.iter().any(|known| known == model) {
                        self.models.push(model.to_owned());
                    }
                }
            }
            Some("model.usage") => {
                self.prompt_tokens += u64_field(payload, "promptTokens").unwrap_or(0);
                self.completion_tokens += u64_field(payload, "completionTokens").unwrap_or(0);
                self.cached_prompt_tokens += u64_field(payload, "cachedPromptTokens").unwrap_or(0);
            }
            Some("turn.completed") => {
                self.elapsed_ms = u64_field(payload, "elapsedMs").or(self.elapsed_ms);
            }
            _ => {}
        }
    }

    fn render(&self, requested_sid: &str) -> String {
        let mut lines = vec![format!(
            "- SID：{}",
            self.sid.as_deref().unwrap_or(requested_sid)
        )];
        if let Some(device_id) = &self.device_id {
            lines.push(format!("- 设备：{device_id}"));
        }
        if let Some(product_id) = &self.product_id {
            lines.push(format!("- Product ID：{product_id}"));
        }
        if let Some(version) = &self.agent_version {
            lines.push(format!("- Agent：{version}"));
        }
        if !self.models.is_empty() {
            lines.push(format!("- 模型：{}", self.models.join(", ")));
        }
        if self.model_calls > 0 {
            lines.push(format!("- 模型调用：{} 次", self.model_calls));
        }
        if self.prompt_tokens > 0 || self.completion_tokens > 0 {
            let mut token = format!(
                "- Token：输入 {}，输出 {}",
                self.prompt_tokens, self.completion_tokens
            );
            if self.cached_prompt_tokens > 0 {
                token.push_str(&format!("，缓存 {}", self.cached_prompt_tokens));
            }
            lines.push(token);
        }
        if let Some(elapsed_ms) = self.elapsed_ms {
            lines.push(format!("- 耗时：{}", render_duration(elapsed_ms)));
        }
        lines.join("\n")
    }
}

fn agent_timeline_line(log: &ParsedAgentLog<'_>, model_call_count: &mut u64) -> Option<String> {
    let payload = log.payload.as_ref()?;
    let time = timeline_time(log.timestamp, payload);
    let event = string_field(payload, "event");
    let message = string_field(payload, "message").unwrap_or_default();

    let summary = match event {
        Some("session.opened") => {
            let mut fields = Vec::new();
            if let Some(sid) = string_field(payload, "sid") {
                fields.push(format!("sid: {sid}"));
            }
            if let Some(device_id) = string_field(payload, "deviceId") {
                fields.push(format!("设备: {device_id}"));
            }
            if fields.is_empty() {
                "会话开始".to_owned()
            } else {
                format!("会话开始，{}", fields.join("，"))
            }
        }
        Some("asr.final") => format!(
            "↑ 用户：{}",
            string_field(payload, "text").unwrap_or("(服务端未记录文本)")
        ),
        Some("response.first_frame") => match u64_field(payload, "firstFrameMs") {
            Some(ms) => format!("↓ 首帧到达（{}）", render_duration(ms)),
            None => "↓ 首帧到达".to_owned(),
        },
        Some("model.started") => {
            *model_call_count += 1;
            let model = string_field(payload, "model").unwrap_or("unknown");
            format!("模型调用 #{}：{model}", *model_call_count)
        }
        Some("tool.started") => {
            let name = string_field(payload, "toolName").unwrap_or("unknown");
            match payload.get("toolArguments") {
                Some(arguments) if !arguments.is_null() => {
                    format!("工具调用：{name} {}", compact(arguments))
                }
                _ => format!("工具调用：{name}"),
            }
        }
        Some("tool.completed") => {
            let name = string_field(payload, "toolName").unwrap_or("unknown");
            let result = summarize_agent_tool_result(payload);
            let duration = u64_field(payload, "durationMs")
                .map(|ms| format!("（{}）", render_duration(ms)))
                .unwrap_or_default();
            match result {
                Some(result) => format!("工具结果：{name} {result}{duration}"),
                None => format!("工具完成：{name}{duration}"),
            }
        }
        Some("response.final") => format!(
            "↓ 回复：{}",
            string_field(payload, "answer")
                .or_else(|| string_field(payload, "answerPreview"))
                .unwrap_or("(服务端未记录完整回复)")
        ),
        Some("turn.completed") => {
            let outcome = string_field(payload, "outcome").unwrap_or("completed");
            let duration = u64_field(payload, "elapsedMs")
                .map(|ms| format!("，耗时 {}", render_duration(ms)))
                .unwrap_or_default();
            format!("会话结束：{outcome}{duration}")
        }
        _ if message.contains("agent bundle resolved") => match string_field(payload, "version") {
            Some(version) => format!("Agent 版本：{version}"),
            None => "Agent Bundle 已加载".to_owned(),
        },
        _ if message.contains("nlp TTS initialized") => {
            format!("↓ TTS URL：{}", message_url(message).unwrap_or("(未记录)"))
        }
        _ if message.contains("nlp text stream initialized") => {
            format!("↓ 文本 URL：{}", message_url(message).unwrap_or("(未记录)"))
        }
        _ if string_field(payload, "level") == Some("error") => {
            format!("错误：{}", normalize_line(message))
        }
        _ => return None,
    };

    Some(format!("[{time}] {summary}"))
}

fn render_verbose_agent_log(log: &ParsedAgentLog<'_>) -> String {
    let time = log
        .payload
        .as_ref()
        .map(|payload| timeline_time(log.timestamp, payload))
        .unwrap_or_else(|| timeline_time(log.timestamp, &Value::Null));
    match &log.payload {
        Some(payload) => {
            let origin = string_field(payload, "origin").unwrap_or("unknown");
            let level = string_field(payload, "level").unwrap_or("info");
            let event = string_field(payload, "event")
                .or_else(|| string_field(payload, "message"))
                .unwrap_or("log");
            format!(
                "[{time}] {origin} {level} {event} {}",
                serde_json::to_string(payload).unwrap_or_else(|_| "{}".to_owned())
            )
        }
        None => format!("[{time}] log {}", normalize_line(log.content)),
    }
}

fn fill_first(target: &mut Option<String>, payload: &Value, key: &str) {
    if target.is_none() {
        *target = string_field(payload, key).map(str::to_owned);
    }
}

fn string_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

fn u64_field(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(Value::as_u64)
}

fn timeline_time(timestamp: &str, payload: &Value) -> String {
    let source = if timestamp.is_empty() {
        string_field(payload, "time").unwrap_or("")
    } else {
        timestamp
    };
    DateTime::parse_from_rfc3339(source)
        .map(|parsed| {
            parsed
                .with_timezone(&Local)
                .format("%H:%M:%S%.3f")
                .to_string()
        })
        .unwrap_or_else(|_| {
            if source.is_empty() {
                "--:--:--.---".to_owned()
            } else {
                source.to_owned()
            }
        })
}

fn message_url(message: &str) -> Option<&str> {
    message
        .split_once("url=")
        .map(|(_, url)| url.trim())
        .filter(|url| !url.is_empty())
}

fn summarize_agent_tool_result(payload: &Value) -> Option<String> {
    let content = payload.get("resultContent")?.as_array()?;
    let mut parts = Vec::new();
    for item in content {
        if let Some(text) = string_field(item, "text") {
            parts.push(text.to_owned());
        } else if !item.is_null() {
            parts.push(compact(item));
        }
    }
    (!parts.is_empty()).then(|| parts.join("；"))
}

fn render_duration(milliseconds: u64) -> String {
    if milliseconds < 1000 {
        format!("{milliseconds} ms")
    } else {
        format!("{:.2} s", milliseconds as f64 / 1000.0)
    }
}

fn normalize_line(text: &str) -> String {
    text.replace('\n', "\\n").replace('\r', "\\r")
}

fn compact(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
}

#[derive(Debug)]
pub struct TraceOutcome {
    /// SID 是请求的唯一标识：要么未命中，要么恰好一条。
    pub record: Option<Value>,
    /// 扫描是否因翻页上限而截断
    pub truncated: bool,
}

/// 在最近 hours 小时的记录中按 SID 检索（匹配 response_body.id / request_id /
/// response_id，均为唯一键），命中即停止翻页。
pub async fn find_by_sid(
    api_base_url: &str,
    api_key: &str,
    sid: &str,
    hours: u32,
) -> Result<TraceOutcome> {
    let end = Utc::now();
    let start = end - Duration::hours(hours as i64);
    let start = start.to_rfc3339_opts(SecondsFormat::Millis, true);
    let end = end.to_rfc3339_opts(SecondsFormat::Millis, true);

    let mut truncated = false;
    for page in 1..=MAX_PAGES {
        let output = query_requests(api_base_url, api_key, &start, &end, page, PAGE_LIMIT).await?;
        let data = output
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let page_len = data.len();
        if let Some(record) = data
            .into_iter()
            .find(|record| record_matches_sid(record, sid))
        {
            return Ok(TraceOutcome {
                record: Some(record),
                truncated: false,
            });
        }
        if page_len < PAGE_LIMIT as usize {
            break;
        }
        if page == MAX_PAGES {
            truncated = true;
        }
    }
    Ok(TraceOutcome {
        record: None,
        truncated,
    })
}

/// 未命中时的提示文案。
pub fn miss_message(sid: &str, hours: u32, truncated: bool) -> String {
    let mut output =
        format!("最近 {hours} 小时内未找到 SID 为 {sid} 的请求记录（SID 有误或已过期）。");
    if truncated {
        output.push_str("\n注意：该时间窗内记录较多，扫描被截断；可用 --hours 缩小范围。");
    }
    output.push_str("\n提示：可用 --hours 调整检索时间窗（默认 168，即 7 天）。");
    output
}

fn record_matches_sid(record: &Value, sid: &str) -> bool {
    let id_matches = |value: Option<&Value>| {
        value
            .map(|value| match value {
                Value::String(text) => text == sid,
                Value::Number(number) => number.to_string() == sid,
                _ => false,
            })
            .unwrap_or(false)
    };
    id_matches(record.pointer("/response_body/id"))
        || id_matches(record.get("request_id"))
        || id_matches(record.get("response_id"))
}

pub fn render_record(record: &Value, requested_sid: &str, verbose: bool) -> String {
    let mut timeline = Vec::new();
    let start_time = text_at(record, "/request_created_at")
        .and_then(local_hms_from_rfc3339)
        .unwrap_or_else(|| "--:--:--.---".to_owned());
    timeline.push(format!("[{start_time}] 请求开始"));

    if let Some(text) = request_text(record) {
        timeline.push(format!("[{start_time}] ↑ 用户：{}", normalize_line(&text)));
    }

    let mut last_reply = None;
    for node in link_nodes(record) {
        let time = node
            .started_at
            .and_then(local_hms_from_unix)
            .unwrap_or_else(|| "--:--:--.---".to_owned());
        if let Some(tool_input) = &node.tool_input {
            timeline.push(format!(
                "[{time}] 工具调用：{} {}",
                node.tag,
                clip(tool_input, false)
            ));
        }
        for result in &node.tool_results {
            timeline.push(format!(
                "[{time}] 工具结果：{} {}",
                node.tag,
                clip(&summarize_tool_result(result), false)
            ));
        }
        if !node.content.is_empty() {
            last_reply = Some(node.content);
        }
    }

    let response = response_text(record).or(last_reply);
    if let Some(response) = response {
        let time = text_at(record, "/response_created_at")
            .and_then(local_hms_from_rfc3339)
            .unwrap_or_else(|| "--:--:--.---".to_owned());
        timeline.push(format!("[{time}] ↓ 回复：{}", normalize_line(&response)));
    }

    if let Some(time) = text_at(record, "/response_created_at").and_then(local_hms_from_rfc3339) {
        let duration = record
            .get("delay_ms")
            .and_then(Value::as_u64)
            .map(|ms| format!("，耗时 {}", render_duration(ms)))
            .unwrap_or_default();
        timeline.push(format!("[{time}] 请求结束{duration}"));
    }

    let mut output = timeline.join("\n");
    let mut summary = vec![format!("- SID：{requested_sid}")];
    if let Some(path) = text_at(record, "/request_path") {
        summary.push(format!("- 路径：{path}"));
    }
    if let Some(model) = text_at(record, "/response_body/model") {
        summary.push(format!("- 模型：{model}"));
    }
    if let Some(tokens) = token_summary(record) {
        summary.push(format!("- Token：{tokens}"));
    }
    if let Some(ms) = record.get("delay_ms").and_then(Value::as_u64) {
        summary.push(format!("- 耗时：{}", render_duration(ms)));
    }
    output.push_str("\n\n");
    output.push_str(&summary.join("\n"));

    if verbose {
        output.push_str("\n\n详细步骤：");
        if let Some(body) = record.get("request_body") {
            output.push_str(&format!(
                "\n[{start_time}] client → llm request {}",
                compact(body)
            ));
        }
        if let Some(frames) = record
            .pointer("/response_body/streamed_data")
            .and_then(Value::as_array)
        {
            for frame in frames {
                let time = frame
                    .get("created")
                    .and_then(Value::as_i64)
                    .and_then(local_hms_from_unix)
                    .unwrap_or_else(|| "--:--:--.---".to_owned());
                output.push_str(&format!("\n[{time}] llm → client frame {}", compact(frame)));
            }
        }
    } else {
        output.push_str("\n\n使用 --verbose 查看全部步骤，--json 输出服务端原始记录。");
    }
    output
}

/// RFC3339 时间转本地时区 "HH:MM:SS.mmm"。
fn local_hms_from_rfc3339(text: String) -> Option<String> {
    let parsed = DateTime::parse_from_rfc3339(&text).ok()?;
    Some(
        parsed
            .with_timezone(&Local)
            .format("%H:%M:%S%.3f")
            .to_string(),
    )
}

/// Unix 秒转本地时区 "HH:MM:SS"（streamed_data 帧只有秒级精度）。
fn local_hms_from_unix(secs: i64) -> Option<String> {
    Some(
        Local
            .timestamp_opt(secs, 0)
            .single()?
            .format("%H:%M:%S")
            .to_string(),
    )
}

/// 链路节点：从 streamed_data 按 (tag, index) 聚合的一次技能/工具/回复片段。
#[derive(Debug)]
struct LinkNode {
    tag: String,
    tool_input: Option<String>,
    tool_results: Vec<Value>,
    content: String,
    /// 节点首帧的 Unix 秒时间戳
    started_at: Option<i64>,
}

fn link_nodes(record: &Value) -> Vec<LinkNode> {
    let Some(frames) = record
        .pointer("/response_body/streamed_data")
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };

    let mut nodes: Vec<(String, LinkNode)> = Vec::new();
    for frame in frames {
        let Some(delta) = frame.pointer("/choices/0/delta") else {
            continue;
        };
        let tag = delta
            .get("tag")
            .and_then(Value::as_str)
            .filter(|tag| !tag.is_empty())
            // 无 tag 的片段是普通模型回复
            .unwrap_or("回复")
            .to_owned();
        let key = delta
            .get("index")
            .and_then(Value::as_str)
            .filter(|index| !index.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| tag.clone());

        if nodes.last().map(|(last_key, _)| last_key != &key) != Some(false) {
            nodes.push((
                key,
                LinkNode {
                    tag,
                    tool_input: None,
                    tool_results: Vec::new(),
                    content: String::new(),
                    started_at: frame.get("created").and_then(Value::as_i64),
                },
            ));
        }
        let node = &mut nodes.last_mut().expect("just pushed").1;

        if node.tool_input.is_none() {
            node.tool_input = delta
                .get("tool_input")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|input| !input.is_empty())
                .map(str::to_owned);
        }
        if let Some(result) = delta.get("tool_result") {
            if !result.is_null() {
                node.tool_results.push(result.clone());
            }
        }
        if let Some(content) = delta.get("content").and_then(Value::as_str) {
            node.content.push_str(content);
        }
    }
    nodes.into_iter().map(|(_, node)| node).collect()
}

/// 工具结果摘要：优先意图答案文本，其次服务名/子类型，兜底截断 JSON。
fn summarize_tool_result(result: &Value) -> String {
    for pointer in ["/intent/answer/text", "/intent/service", "/sub"] {
        if let Some(text) = result
            .pointer(pointer)
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
        {
            return text.to_owned();
        }
    }
    result.to_string()
}

fn token_summary(record: &Value) -> Option<String> {
    let usage = record.pointer("/response_body/usage")?;
    let total = usage.get("total_tokens").and_then(Value::as_i64)?;
    match (
        usage.get("prompt_tokens").and_then(Value::as_i64),
        usage.get("completion_tokens").and_then(Value::as_i64),
    ) {
        (Some(prompt), Some(completion)) => Some(format!(
            "{total}（prompt {prompt} + completion {completion}）"
        )),
        _ => Some(total.to_string()),
    }
}

/// 默认视图可截断长文本，需要完整内容的调用方可以显式关闭截断。
fn clip(text: &str, unabridged: bool) -> String {
    const LIMIT: usize = 160;
    let text = text.trim();
    if unabridged || text.chars().count() <= LIMIT {
        return text.to_owned();
    }
    let clipped: String = text.chars().take(LIMIT).collect();
    format!("{clipped}…")
}

/// 提取请求文本：优先 request_body.messages 中最后一条 user 消息（多轮上下文时
/// request_prompt 是历史首条，会产生误导），其次 request_prompt。
fn request_text(record: &Value) -> Option<String> {
    record
        .pointer("/request_body/messages")
        .and_then(Value::as_array)
        .and_then(|messages| {
            messages
                .iter()
                .rev()
                .find(|message| message.get("role").and_then(Value::as_str) == Some("user"))
        })
        .and_then(|message| message.get("content").and_then(Value::as_str))
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
        .or_else(|| text_at(record, "/request_prompt"))
}

fn text_at(record: &Value, pointer: &str) -> Option<String> {
    record
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// 提取响应文本：优先 choices[0].message.content，其次 response_prompt。
fn response_text(record: &Value) -> Option<String> {
    record
        .pointer("/response_body/choices/0/message/content")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            let raw = text_at(record, "/response_prompt")?;
            // response_prompt 可能是 JSON 字符串，尝试取其中的 content
            match serde_json::from_str::<Value>(&raw) {
                Ok(value) => value
                    .get("content")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .or(Some(raw)),
                Err(_) => Some(raw),
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_record() -> Value {
        json!({
            "request_id": "99e2e08c-fff6-4bc5-9cc8-c39edb4b52c4",
            "response_id": 192977817,
            "request_created_at": "2026-07-05T03:28:23.349Z",
            "response_created_at": "2026-07-05T03:28:25.317Z",
            "request_path": "/v1/chat/completions",
            "request_prompt": "现在几点",
            "request_body": {
                "messages": [
                    {"role": "user", "content": "现在几点"},
                    {"role": "assistant", "content": "现在是11点。"},
                    {"role": "user", "content": "你好"}
                ]
            },
            "delay_ms": 1968,
            "user_api_key_preview": "b71ca...0d8",
            "response_body": {
                "id": "746c7ab660b341dbb27937c77f8223c7",
                "model": "ls-interaction-v1",
                "usage": {"total_tokens": 1980, "prompt_tokens": 1940, "completion_tokens": 40},
                "choices": [{"message": {"role": "assistant", "content": "现在是11点28分。"}}],
                "streamed_data": [
                    {"created": 1783222105, "choices": [{"delta": {
                        "tag": "aiui_datetimePro", "index": "node-1", "type": "start",
                        "content": "", "tool_input": "查询当前时间。"
                    }}]},
                    {"choices": [{"delta": {
                        "tag": "aiui_datetimePro", "index": "node-1", "type": "delta",
                        "content": "",
                        "tool_result": {"sub": "nlp", "intent": {"service": "datetime",
                            "answer": {"text": "当前是11点28分。"}}}
                    }}]},
                    {"choices": [{"delta": {
                        "tag": "reply_text", "index": "node-2", "type": "delta",
                        "content": "现在是"
                    }}]},
                    {"choices": [{"delta": {
                        "tag": "reply_text", "index": "node-2", "type": "delta",
                        "content": "11点28分。"
                    }}]}
                ]
            }
        })
    }

    #[test]
    fn matches_interaction_sid_via_response_body_id() {
        assert!(record_matches_sid(
            &sample_record(),
            "746c7ab660b341dbb27937c77f8223c7"
        ));
        assert!(!record_matches_sid(&sample_record(), "deadbeef"));
    }

    #[test]
    fn matches_request_and_response_ids() {
        assert!(record_matches_sid(
            &sample_record(),
            "99e2e08c-fff6-4bc5-9cc8-c39edb4b52c4"
        ));
        assert!(record_matches_sid(&sample_record(), "192977817"));
    }

    #[test]
    fn renders_agent_trace_as_human_timeline() {
        let output = json!({
            "data": [
                {
                    "timestamp": "2026-07-26T08:37:21.772Z",
                    "content": json!({
                        "level": "info", "origin": "host", "version": "v0.0.7",
                        "deviceId": "device-1", "productId": "product-1", "sid": "sid-1",
                        "message": "[LSAgentFramework] nlp agent bundle resolved"
                    }).to_string()
                },
                {
                    "timestamp": "2026-07-26T08:37:22.375Z",
                    "content": json!({
                        "level": "info", "origin": "host", "event": "session.opened",
                        "deviceId": "device-1", "productId": "product-1", "sid": "sid-1",
                        "message": "[LSAgentFramework] nlp session open"
                    }).to_string()
                },
                {
                    "timestamp": "2026-07-26T08:37:22.384Z",
                    "content": json!({
                        "level": "info", "origin": "host", "event": "asr.final",
                        "deviceId": "device-1", "productId": "product-1", "sid": "sid-1",
                        "text": "今天天气怎么样",
                        "message": "[LSAgentFramework] process frame data"
                    }).to_string()
                },
                {
                    "timestamp": "2026-07-26T08:37:23.173Z",
                    "content": json!({
                        "level": "info", "origin": "agent", "event": "model.started",
                        "deviceId": "device-1", "productId": "product-1", "sid": "sid-1",
                        "model": "deepseek-v4-flash",
                        "message": "[LSAgentFramework] official voice react model started"
                    }).to_string()
                },
                {
                    "timestamp": "2026-07-26T08:37:24.100Z",
                    "content": json!({
                        "level": "info", "origin": "agent", "event": "tool.started",
                        "deviceId": "device-1", "productId": "product-1", "sid": "sid-1",
                        "toolName": "get_weather_info",
                        "toolArguments": {"location": "台北", "date": "今天"},
                        "message": "[LSAgentFramework] official voice react tool started"
                    }).to_string()
                },
                {
                    "timestamp": "2026-07-26T08:37:24.500Z",
                    "content": json!({
                        "level": "info", "origin": "agent", "event": "tool.completed",
                        "deviceId": "device-1", "productId": "product-1", "sid": "sid-1",
                        "toolName": "get_weather_info", "durationMs": 400,
                        "resultContent": [{"type": "text", "text": "今天台北晴。"}],
                        "message": "[LSAgentFramework] official voice react tool completed"
                    }).to_string()
                },
                {
                    "timestamp": "2026-07-26T08:37:26.198Z",
                    "content": json!({
                        "level": "info", "origin": "agent", "event": "model.usage",
                        "deviceId": "device-1", "productId": "product-1", "sid": "sid-1",
                        "promptTokens": 7542, "completionTokens": 28,
                        "cachedPromptTokens": 7424,
                        "message": "[LSAgentFramework] official voice react model usage"
                    }).to_string()
                },
                {
                    "timestamp": "2026-07-26T08:37:26.201Z",
                    "content": json!({
                        "level": "info", "origin": "agent", "event": "response.final",
                        "deviceId": "device-1", "productId": "product-1", "sid": "sid-1",
                        "answer": "今天台北大晴天。",
                        "message": "[LSAgentFramework] official voice final reply"
                    }).to_string()
                },
                {
                    "timestamp": "2026-07-26T08:37:26.479Z",
                    "content": json!({
                        "level": "info", "origin": "agent", "event": "turn.completed",
                        "deviceId": "device-1", "productId": "product-1", "sid": "sid-1",
                        "outcome": "COMPLETED", "elapsedMs": 4092,
                        "message": "[LSAgentFramework] official voice turn completed"
                    }).to_string()
                }
            ],
            "truncated": true
        });
        let rendered = render_agent_trace(&output, "sid-1", false).unwrap();
        assert!(rendered.contains("Agent 版本：v0.0.7"));
        assert!(rendered.contains("会话开始，sid: sid-1，设备: device-1"));
        assert!(rendered.contains("↑ 用户：今天天气怎么样"));
        assert!(rendered.contains("模型调用 #1：deepseek-v4-flash"));
        assert!(rendered
            .contains("工具调用：get_weather_info {\"date\":\"今天\",\"location\":\"台北\"}"));
        assert!(rendered.contains("工具结果：get_weather_info 今天台北晴。（400 ms）"));
        assert!(rendered.contains("↓ 回复：今天台北大晴天。"));
        assert!(rendered.contains("会话结束：COMPLETED，耗时 4.09 s"));
        assert!(rendered.contains("- Token：输入 7542，输出 28，缓存 7424"));
        assert!(rendered.contains("- 日志：服务端结果已截断"));
        assert!(rendered.contains("--verbose"));
        assert!(!rendered.contains("详细步骤："));

        let verbose = render_agent_trace(&output, "sid-1", true).unwrap();
        assert!(verbose.contains("详细步骤："));
        assert!(verbose.contains("agent info tool.started {\""));
        assert!(verbose.contains("\"toolArguments\":{\"date\":\"今天\",\"location\":\"台北\"}"));
        assert!(!verbose.contains("使用 --verbose"));
    }

    #[test]
    fn renders_hit_summary_with_link_nodes() {
        let out = render_record(&sample_record(), "746c7ab660b341dbb27937c77f8223c7", false);
        assert!(out.contains("ls-interaction-v1"));
        assert!(out.contains("你好"));
        assert!(out.contains("1980（prompt 1940 + completion 40）"));
        assert!(out.contains("请求开始"));
        assert!(out.contains("↑ 用户：你好"));
        assert!(out.contains("工具调用：aiui_datetimePro 查询当前时间。"));
        assert!(out.contains("工具结果：aiui_datetimePro 当前是11点28分。"));
        assert!(out.contains("↓ 回复：现在是11点28分。"));
        assert!(out.contains("请求结束，耗时 1.97 s"));
        assert!(!out.contains("详细步骤："));
        assert!(out.contains("--verbose"));
    }

    #[test]
    fn verbose_mode_shows_each_raw_record_frame_on_one_line() {
        let out = render_record(&sample_record(), "746c7ab660b341dbb27937c77f8223c7", true);
        assert!(out.contains("详细步骤："));
        assert!(out.contains("client → llm request"));
        assert!(out.contains("llm → client frame"));
        assert!(out
            .lines()
            .filter(|line| line.contains("llm → client frame"))
            .all(|line| !line.contains('\n')));
    }

    #[test]
    fn miss_message_hints_expiry_and_window() {
        let out = miss_message("abc", 24, false);
        assert!(out.contains("未找到"));
        assert!(out.contains("有误或已过期"));
        assert!(out.contains("--hours"));

        let truncated = miss_message("abc", 24, true);
        assert!(truncated.contains("截断"));
    }

    #[test]
    fn clip_truncates_only_in_default_mode() {
        let long = "很".repeat(300);
        assert!(clip(&long, false).chars().count() <= 161);
        assert_eq!(clip(&long, true), long);
    }
}
