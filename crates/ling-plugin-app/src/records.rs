//! 按 SID 查询 Agent 执行日志并渲染时序概览。

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Local};

use serde_json::Value;
use std::time::Duration as StdDuration;

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
        _ => match string_field(payload, "level") {
            Some("error") => format!("错误：{}", normalize_line(message)),
            Some("warn") => format!("告警：{}", normalize_line(message)),
            _ => return None,
        },
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn log_entry(level: &str, message: &str) -> Value {
        json!({
            "timestamp": "2026-07-26T08:37:21.772Z",
            "content": json!({
                "level": level,
                "origin": "agent",
                "message": message,
                "sid": "sid-1",
                "time": "2026-07-26T08:37:21Z",
            })
            .to_string(),
        })
    }

    #[test]
    fn default_timeline_keeps_warn_and_error_but_drops_info_and_debug() {
        let output = json!({
            "data": [
                log_entry("debug", "内部细节"),
                log_entry("info", "常规进展"),
                log_entry("warn", "xiaoling interaction failed before any delta"),
                log_entry("error", "bundle 执行失败"),
            ]
        });

        let rendered = render_agent_trace(&output, "sid-1", false).expect("render trace");

        assert!(rendered.contains("告警：xiaoling interaction failed before any delta"));
        assert!(rendered.contains("错误：bundle 执行失败"));
        assert!(!rendered.contains("内部细节"));
        assert!(!rendered.contains("常规进展"));
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
}
