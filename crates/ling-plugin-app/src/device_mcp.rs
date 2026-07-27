use serde_json::{json, Value};

const LED_SWITCH: &str = "ls.led_switch";
const LED_BLINK: &str = "ls.led_blink";
const DEVICE_VERSION_INFO: &str = "ls.device_version_info";
const LED_SWITCH_DESCRIPTION: &str = "用于控制 LED 的静态开关状态，只处理打开、关闭或常亮请求；需要控制闪烁速度或停止闪烁时应使用 led_blink。强制调用顺序：首次响应必须先输出非空自然语言“我来处理设备控制。”，工具调用必须作为后续事件发送；禁止首事件直接调用工具，也禁止调用后再补前置文本。";
const LED_BLINK_DESCRIPTION: &str = "用于控制 LED 的闪烁模式，包括关闭闪烁、正常闪烁、快速闪烁和慢速闪烁。用户要求 LED 常亮或仅开关 LED 时不要调用本工具，应使用 led_switch。强制调用顺序：首次响应必须先输出非空自然语言“我来处理设备控制。”，工具调用必须作为后续事件发送；禁止首事件直接调用工具，也禁止调用后再补前置文本。";
const VERSION_INFO_DESCRIPTION: &str =
    "获取设备固件版本信息，包括版本号和提交信息。可以通过类似'设备版本是多少'、'固件版本'、'查看版本信息'等方式触发。";

pub(crate) fn response_for_frame(frame: &Value) -> Option<Value> {
    if frame.get("action").and_then(Value::as_str) != Some("mcp") {
        return None;
    }

    let message = frame
        .get("data")
        .filter(|value| value.is_object())
        .unwrap_or(frame);
    let method = message.get("method").and_then(Value::as_str)?;
    let id = message.get("id")?.clone();
    let result = match method {
        "initialize" => initialize_result(),
        "tools/list" => json!({"tools": tools()}),
        "tools/call" => call_tool(message)?,
        _ => return None,
    };

    Some(json!({
        "action": "mcp",
        "method": method,
        "id": id,
        "result": result,
    }))
}

fn initialize_result() -> Value {
    json!({
        "capabilities": {
            "tools": {
                "count": tools().len(),
                "categories": ["system", "device", "utility"],
            }
        },
        "serverInfo": {
            "name": "arcs-mini",
            "version": "1.0.0",
        },
        "instructions": "设备端MCP服务，提供系统信息查询、设备控制等工具调用功能",
    })
}

fn tools() -> Vec<Value> {
    vec![
        json!({
            "name": LED_SWITCH,
            "description": LED_SWITCH_DESCRIPTION,
            "inputSchema": {
                "type": "object",
                "properties": {
                    "value": {
                        "description": "LED开关状态，打开为true，关闭为false",
                        "type": "boolean",
                    }
                },
                "required": ["value"],
            }
        }),
        json!({
            "name": LED_BLINK,
            "description": LED_BLINK_DESCRIPTION,
            "inputSchema": {
                "type": "object",
                "properties": {
                    "mode": {
                        "description": "LED闪烁模式，可以是 'off'、'normal'、'fast' 或 'slow'",
                        "type": "string",
                    }
                },
                "required": ["mode"],
            }
        }),
        json!({
            "name": DEVICE_VERSION_INFO,
            "description": VERSION_INFO_DESCRIPTION,
            "inputSchema": {
                "type": "object",
                "properties": {},
                "required": [],
            }
        }),
    ]
}

fn call_tool(message: &Value) -> Option<Value> {
    let params = message.get("params").and_then(Value::as_object);
    let name = params
        .and_then(|params| params.get("name"))
        .and_then(Value::as_str);
    let arguments = params
        .and_then(|params| params.get("arguments"))
        .unwrap_or(&Value::Null);

    match name {
        Some(LED_SWITCH) => arguments
            .get("value")
            .and_then(Value::as_bool)
            .map(|_| success_result(LED_SWITCH, "已完成操作")),
        Some(LED_BLINK) => match arguments.get("mode").and_then(Value::as_str) {
            Some("off" | "normal" | "fast" | "slow") => {
                Some(success_result(LED_BLINK, "已完成操作"))
            }
            _ => None,
        },
        Some(DEVICE_VERSION_INFO) => Some(success_result(
            DEVICE_VERSION_INFO,
            &format!(
                "当前固件版本号为{}, commit=ling-cli",
                env!("CARGO_PKG_VERSION")
            ),
        )),
        _ => None,
    }
}

fn success_result(tool: &str, text: &str) -> Value {
    json!({
        "tool": tool,
        "content": [{"type": "text", "text": text}],
        "isError": false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initializes_like_a_device_mcp_server() {
        let response = response_for_frame(&json!({
            "action": "mcp",
            "data": {"method": "initialize", "id": "init-1"}
        }))
        .unwrap();

        assert_eq!(response["method"], "initialize");
        assert_eq!(response["id"], "init-1");
        assert_eq!(response["result"]["serverInfo"]["name"], "arcs-mini");
        assert_eq!(response["result"]["serverInfo"]["version"], "1.0.0");
        assert_eq!(
            response["result"]["instructions"],
            "设备端MCP服务，提供系统信息查询、设备控制等工具调用功能"
        );
        assert_eq!(
            response["result"]["capabilities"]["tools"]["categories"],
            json!(["system", "device", "utility"])
        );
        assert_eq!(response["result"]["capabilities"]["tools"]["count"], 3);
    }

    #[test]
    fn lists_tools_with_the_exact_arcs_mini_contracts() {
        let response = response_for_frame(&json!({
            "action": "mcp",
            "data": {"method": "tools/list", "id": "list-1"}
        }))
        .unwrap();
        let tools = response["result"]["tools"].as_array().unwrap();

        assert_eq!(
            tools,
            &vec![
                json!({
                    "name": "ls.led_switch",
                    "description": LED_SWITCH_DESCRIPTION,
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "value": {
                                "description": "LED开关状态，打开为true，关闭为false",
                                "type": "boolean",
                            }
                        },
                        "required": ["value"],
                    }
                }),
                json!({
                    "name": "ls.led_blink",
                    "description": LED_BLINK_DESCRIPTION,
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "mode": {
                                "description": "LED闪烁模式，可以是 'off'、'normal'、'fast' 或 'slow'",
                                "type": "string",
                            }
                        },
                        "required": ["mode"],
                    }
                }),
                json!({
                    "name": "ls.device_version_info",
                    "description": VERSION_INFO_DESCRIPTION,
                    "inputSchema": {
                        "type": "object",
                        "properties": {},
                        "required": [],
                    }
                }),
            ]
        );
    }

    #[test]
    fn simulates_led_switch_calls() {
        let response = response_for_frame(&json!({
            "action": "mcp",
            "data": {
                "method": "tools/call",
                "id": "call-1",
                "params": {"name": LED_SWITCH, "arguments": {"value": true}}
            }
        }))
        .unwrap();

        assert_eq!(response["result"]["tool"], LED_SWITCH);
        assert_eq!(response["result"]["isError"], false);
        assert_eq!(response["result"]["content"][0]["text"], "已完成操作");
    }

    #[test]
    fn simulates_led_blink_calls() {
        let response = response_for_frame(&json!({
            "action": "mcp",
            "data": {
                "method": "tools/call",
                "id": "call-blink",
                "params": {"name": LED_BLINK, "arguments": {"mode": "fast"}}
            }
        }))
        .unwrap();

        assert_eq!(response["result"]["tool"], LED_BLINK);
        assert_eq!(response["result"]["isError"], false);
        assert_eq!(response["result"]["content"][0]["text"], "已完成操作");
    }

    #[test]
    fn reports_firmware_like_the_real_device() {
        let response = response_for_frame(&json!({
            "action": "mcp",
            "data": {
                "method": "tools/call",
                "id": "call-version",
                "params": {"name": DEVICE_VERSION_INFO, "arguments": {}}
            }
        }))
        .unwrap();

        assert_eq!(response["result"]["tool"], DEVICE_VERSION_INFO);
        assert_eq!(response["result"]["isError"], false);
        assert_eq!(
            response["result"]["content"][0]["text"],
            format!(
                "当前固件版本号为{}, commit=ling-cli",
                env!("CARGO_PKG_VERSION")
            )
        );
    }

    #[test]
    fn ignores_unknown_tools_like_the_real_device() {
        assert!(response_for_frame(&json!({
            "action": "mcp",
            "data": {
                "method": "tools/call",
                "id": "call-2",
                "params": {"name": "unknown", "arguments": {}}
            }
        }))
        .is_none());
    }

    #[test]
    fn ignores_tool_lifecycle_notifications_like_the_real_device() {
        for method in ["tools/start", "tools/complete"] {
            assert!(response_for_frame(&json!({
                "action": "mcp",
                "data": {
                    "method": method,
                    "id": "notification-1",
                    "params": {"name": "device_version_info"}
                }
            }))
            .is_none());
        }
    }
}
