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

pub fn render_agent_logs(value: &Value) -> Result<String> {
    let logs = value
        .get("data")
        .and_then(Value::as_array)
        .context("Agent 日志响应缺少 data 数组")?;
    let mut output = String::new();
    for log in logs {
        let timestamp = log.get("timestamp").and_then(Value::as_str).unwrap_or("-");
        let content = log.get("content").and_then(Value::as_str).unwrap_or("-");
        output.push_str(timestamp);
        output.push_str("  ");
        output.push_str(content);
        output.push('\n');
    }
    if value
        .get("truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        output.push_str("注意：服务端日志结果已截断。\n");
    }
    Ok(output.trim_end().to_owned())
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

pub fn render_record(record: &Value, full: bool) -> String {
    let mut output = String::new();
    {
        for (label, value) in [
            (
                "时间",
                text_at(record, "/request_created_at").map(|time| {
                    DateTime::parse_from_rfc3339(&time)
                        .map(|parsed| {
                            parsed
                                .with_timezone(&Local)
                                .format("%Y-%m-%d %H:%M:%S%.3f")
                                .to_string()
                        })
                        .unwrap_or(time)
                }),
            ),
            ("路径", text_at(record, "/request_path")),
            ("模型", text_at(record, "/response_body/model")),
            ("API Key", text_at(record, "/user_api_key_preview")),
            ("请求", request_text(record)),
            ("响应", response_text(record)),
            ("Tokens", token_summary(record)),
        ] {
            if let Some(value) = value.filter(|value| !value.is_empty()) {
                if !output.is_empty() {
                    output.push('\n');
                }
                output.push_str(&format!("{label}: {value}"));
            }
        }

        output.push_str(&render_timeline(record, full));

        if full {
            if let Some(messages) = record
                .pointer("/request_body/messages")
                .and_then(Value::as_array)
            {
                output.push_str("\n请求上下文:");
                for message in messages {
                    let role = message.get("role").and_then(Value::as_str).unwrap_or("-");
                    let content = message
                        .get("content")
                        .map(|content| match content {
                            Value::String(text) => text.clone(),
                            other => other.to_string(),
                        })
                        .unwrap_or_default();
                    output.push_str(&format!("\n  [{role}] {content}"));
                }
            }
        }
    }
    if !full {
        output.push_str("\n\n使用 --full 查看完整上下文与工具明细，--json 输出原始记录。");
    } else {
        output.push_str("\n\n使用 --json 输出原始记录。");
    }
    output
}

/// 时间线：请求到达 → 各链路节点 → 响应完成，时间按本地时区显示。
fn render_timeline(record: &Value, full: bool) -> String {
    const TIME_WIDTH: usize = 12; // "11:28:23.349"
    let pad = |time: String| format!("{time:<TIME_WIDTH$}");
    let indent = " ".repeat(TIME_WIDTH + 1);

    let push_event = |output: &mut String, time: String, text: &str| {
        output.push_str(&format!("\n  {} {}", pad(time), text));
    };

    let mut output = String::from("\n时间线:");
    if let Some(time) = text_at(record, "/request_created_at").and_then(local_hms_from_rfc3339) {
        push_event(&mut output, time, "请求到达");
    }

    let mut detail_lines: Vec<String> = Vec::new();
    for node in link_nodes(record) {
        let time = node
            .started_at
            .and_then(local_hms_from_unix)
            .unwrap_or_default();
        push_event(&mut output, time, &node.tag);
        detail_lines.clear();
        if let Some(tool_input) = &node.tool_input {
            detail_lines.push(format!("工具输入: {}", clip(tool_input, full)));
        }
        for result in &node.tool_results {
            if full {
                let pretty =
                    serde_json::to_string_pretty(result).unwrap_or_else(|_| result.to_string());
                detail_lines.push("工具结果:".to_owned());
                detail_lines.extend(pretty.lines().map(|line| format!("  {line}")));
            } else {
                detail_lines.push(format!(
                    "工具结果: {}",
                    clip(&summarize_tool_result(result), false)
                ));
            }
        }
        if !node.content.is_empty() {
            detail_lines.push(format!("输出: {}", clip(&node.content, full)));
        }
        for line in &detail_lines {
            output.push_str(&format!("\n  {indent}{line}"));
        }
    }

    if let Some(time) = text_at(record, "/response_created_at").and_then(local_hms_from_rfc3339) {
        let delay = record
            .get("delay_ms")
            .and_then(Value::as_i64)
            .map(|ms| format!("（耗时 {ms} ms）"))
            .unwrap_or_default();
        push_event(&mut output, time, &format!("响应完成{delay}"));
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

/// 默认视图截断长文本；--full 不截断。
fn clip(text: &str, full: bool) -> String {
    const LIMIT: usize = 160;
    let text = text.trim();
    if full || text.chars().count() <= LIMIT {
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
    fn renders_agent_logs_in_server_order() {
        let output = json!({
            "data": [
                {"timestamp": "2026-07-26T16:37:21.344+08:00", "content": "llm connecting version=2.0"},
                {"timestamp": "2026-07-26T16:37:21.500+08:00", "content": "connected"}
            ],
            "truncated": true
        });
        let rendered = render_agent_logs(&output).unwrap();
        let first = rendered.find("llm connecting version=2.0").unwrap();
        let second = rendered.find("connected").unwrap();
        assert!(first < second);
        assert!(rendered.contains("服务端日志结果已截断"));
    }

    #[test]
    fn renders_hit_summary_with_link_nodes() {
        let out = render_record(&sample_record(), false);
        assert!(out.contains("ls-interaction-v1"));
        assert!(out.contains("你好"));
        assert!(out.contains("1980（prompt 1940 + completion 40）"));
        assert!(out.contains("时间线:"));
        assert!(out.contains("请求到达"));
        assert!(out.contains("aiui_datetimePro"));
        assert!(out.contains("工具输入: 查询当前时间。"));
        assert!(out.contains("工具结果: 当前是11点28分。"));
        assert!(out.contains("reply_text"));
        assert!(out.contains("输出: 现在是11点28分。"));
        assert!(out.contains("响应完成（耗时 1968 ms）"));
        // 默认模式不展示完整上下文
        assert!(!out.contains("请求上下文"));
        assert!(out.contains("--full"));
    }

    #[test]
    fn full_mode_shows_context_and_raw_tool_results() {
        let out = render_record(&sample_record(), true);
        assert!(out.contains("请求上下文:"));
        assert!(out.contains("[user] 现在几点"));
        assert!(out.contains("[assistant] 现在是11点。"));
        assert!(out.contains("\"service\": \"datetime\""));
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
