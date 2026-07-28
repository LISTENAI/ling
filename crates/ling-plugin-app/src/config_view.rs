//! 基于 `GET /v1/projects/{product_id}` 详情渲染应用配置的只读视图。

use crate::{array_len, bool_field, field, render_table, string_field};
use anyhow::{Context, Result};
use serde_json::Value;

pub fn project_data(value: &Value) -> &Value {
    value.get("data").unwrap_or(value)
}

fn app_config(value: &Value) -> Option<&Value> {
    project_data(value)
        .get("apps")
        .and_then(Value::as_array)
        .and_then(|apps| apps.first())
        .and_then(|app| app.get("config"))
}

/// product.secret（用于端云链路模拟请求）。
pub fn product_secret(value: &Value) -> Option<String> {
    let product = project_data(value).get("product")?;
    product
        .get("secret")
        .and_then(Value::as_str)
        .filter(|secret| !secret.is_empty() && !secret.contains('*'))
        .map(str::to_owned)
}

pub fn render_device_quota(value: &Value) -> Result<String> {
    let product = project_data(value)
        .get("product")
        .context("项目详情缺少 product 字段")?;
    let total = first_field(product, &["assignedDeviceQuota", "assigned_device_quota"]);
    let used = first_field(product, &["consumedDeviceQuota", "consumed_device_quota"]);
    let enforce = if first_bool(product, &["deviceAuthCheck", "device_auth_check"]) {
        "开启"
    } else {
        "关闭"
    };
    Ok(render_table(
        &["总额度", "已使用", "强制白名单"],
        &[vec![total, used, enforce.to_owned()]],
    ))
}

pub fn device_auth_check(value: &Value) -> Option<bool> {
    let product = project_data(value).get("product")?;
    ["deviceAuthCheck", "device_auth_check"]
        .iter()
        .find_map(|key| product.get(key).and_then(Value::as_bool))
}

pub fn render_role_list(value: &Value) -> Result<String> {
    let config = app_config(value).context("项目详情缺少应用配置")?;
    let roles = config
        .get("llm_roles")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    if roles.is_empty() {
        return Ok("未配置角色。".to_owned());
    }
    let rows = roles
        .iter()
        .map(|role| {
            vec![
                field(role, "name"),
                field(role, "id"),
                yes_dash(bool_field(Some(role), "is_default")),
                if bool_field(Some(role), "is_builtin") {
                    "内置".to_owned()
                } else {
                    "自定义".to_owned()
                },
                role.get("tts")
                    .and_then(|tts| string_field(Some(tts), "vcn"))
                    .unwrap_or_else(|| "-".to_owned()),
                role.get("tts")
                    .and_then(|tts| tts.get("speed"))
                    .map(|speed| speed.to_string())
                    .unwrap_or_else(|| "-".to_owned()),
                role.get("tts")
                    .and_then(|tts| tts.get("volume"))
                    .map(|volume| volume.to_string())
                    .unwrap_or_else(|| "-".to_owned()),
                array_len(Some(role), "knowledge").to_string(),
            ]
        })
        .collect::<Vec<_>>();
    let mut output = render_table(
        &[
            "角色",
            "ID",
            "默认",
            "类型",
            "发音人",
            "语速",
            "音量",
            "知识库",
        ],
        &rows,
    );
    output.push_str(&format!("\n共 {} 个角色。", rows.len()));
    if let Some(word) = config.get("default_wakeup_word") {
        let name = field(word, "name");
        if name != "-" {
            output.push_str(&format!(
                "\n默认唤醒词：{name}（灵敏度 {}）",
                field(word, "sensitivity")
            ));
        }
    }
    Ok(output)
}

pub fn interact_mode_label(mode: i64) -> &'static str {
    match mode {
        0 => "oneshot",
        1 => "full-duplex",
        2 => "half-duplex",
        _ => "unknown",
    }
}

pub fn render_interact_mode(value: &Value) -> Result<String> {
    let config = app_config(value).context("项目详情缺少应用配置")?;
    let mode = config
        .get("interaction_mode")
        .and_then(Value::as_i64)
        .context("应用配置缺少 interaction_mode 字段")?;
    let desc = match mode {
        0 => "单工模式（一次唤醒，一次对话）",
        1 => "全双工模式（一次唤醒，连续对话，支持打断）",
        2 => "半双工模式（一次唤醒，连续对话）",
        _ => "未知模式",
    };
    Ok(format!("{}：{}", interact_mode_label(mode), desc))
}

pub fn render_tone_show(value: &Value) -> Result<String> {
    let config = app_config(value).context("项目详情缺少应用配置")?;
    let tones = config
        .get("prompt_tone_texts")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    if tones.is_empty() {
        return Ok("未配置提示语。".to_owned());
    }
    let rows = tones
        .iter()
        .map(|tone| {
            vec![
                field(tone, "key"),
                field(tone, "name"),
                field(tone, "text"),
                yes_dash(bool_field(Some(tone), "is_default")),
            ]
        })
        .collect::<Vec<_>>();
    let mut output = render_table(&["Key", "名称", "文案", "默认"], &rows);
    output.push_str(&format!("\n共 {} 条提示语。", rows.len()));
    Ok(output)
}

pub fn render_lexicon_list(value: &Value) -> Result<String> {
    let config = app_config(value).context("项目详情缺少应用配置")?;
    let hotwords = config
        .get("llm_feature")
        .and_then(|feature| feature.get("hotwords"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    if hotwords.is_empty() {
        return Ok("未配置专业词汇。".to_owned());
    }
    let rows = hotwords
        .iter()
        .map(|word| vec![generic_label(word)])
        .collect::<Vec<_>>();
    let mut output = render_table(&["词汇"], &rows);
    output.push_str(&format!("\n共 {} 个专业词汇。", rows.len()));
    Ok(output)
}

pub fn render_app_kb_list(value: &Value) -> Result<String> {
    let config = app_config(value).context("项目详情缺少应用配置")?;
    let knowledge = config
        .get("llm_feature")
        .and_then(|feature| feature.get("knowledge"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    if knowledge.is_empty() {
        return Ok("应用未关联知识库。".to_owned());
    }
    let rows = knowledge
        .iter()
        .map(|kb| match kb {
            Value::String(id) => vec![id.clone(), "-".to_owned()],
            other => vec![
                string_field(Some(other), "id")
                    .or_else(|| string_field(Some(other), "index_id"))
                    .unwrap_or_else(|| "-".to_owned()),
                string_field(Some(other), "name")
                    .or_else(|| string_field(Some(other), "index_name"))
                    .unwrap_or_else(|| "-".to_owned()),
            ],
        })
        .collect::<Vec<_>>();
    let mut output = render_table(&["知识库 ID", "名称"], &rows);
    output.push_str(&format!("\n共关联 {} 个知识库。", rows.len()));
    Ok(output)
}

pub fn render_management_capabilities(value: &Value) -> Result<String> {
    let data = response_data(value)?;
    let capabilities = data
        .get("capabilities")
        .and_then(Value::as_object)
        .context("能力响应缺少 data.capabilities")?;
    let mut rows = capabilities
        .iter()
        .map(|(name, version)| vec![name.clone(), scalar(version)])
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| a[0].cmp(&b[0]));
    let mut output = format!(
        "API 版本: {}\n修订版本: {}\n\n{}",
        field(data, "api_version"),
        field(data, "revision"),
        render_table(&["能力", "版本"], &rows)
    );
    output.push_str(&format!("\n共 {} 项能力。", rows.len()));
    Ok(output)
}

pub fn render_management_config(value: &Value) -> Result<String> {
    let prompt = value
        .get("system_prompt")
        .and_then(Value::as_str)
        .unwrap_or("");
    let rows = vec![
        vec![
            "interaction_mode".to_owned(),
            "交互模式".to_owned(),
            config_value(value, "interaction_mode"),
            config_field_constraint(value, "interaction_mode"),
        ],
        vec![
            "system_prompt".to_owned(),
            "系统提示词".to_owned(),
            if prompt.is_empty() {
                "(默认)".to_owned()
            } else {
                config_preview(prompt, 32)
            },
            config_field_constraint(value, "system_prompt"),
        ],
        vec![
            "protocol".to_owned(),
            "模型协议".to_owned(),
            config_value(value, "protocol"),
            config_field_constraint(value, "protocol"),
        ],
        vec![
            "endpoint".to_owned(),
            "模型端点".to_owned(),
            config_preview(&config_value(value, "endpoint"), 36),
            config_field_constraint(value, "endpoint"),
        ],
        vec![
            "model".to_owned(),
            "模型".to_owned(),
            config_value(value, "model"),
            config_field_constraint(value, "model"),
        ],
        vec![
            "authorization".to_owned(),
            "模型凭据".to_owned(),
            if value
                .get("authorization_configured")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                "已配置（密钥不可读取）".to_owned()
            } else {
                "未配置".to_owned()
            },
            config_field_constraint(value, "authorization"),
        ],
    ];
    Ok(render_table(
        &["Key", "配置", "当前值", "可用值/格式"],
        &rows,
    ))
}

pub fn render_management_role_list(value: &Value) -> Result<String> {
    let roles = response_items(value)?;
    if roles.is_empty() {
        return Ok("暂无角色。".to_owned());
    }
    let rows = roles
        .iter()
        .map(|role| {
            vec![
                field(role, "name"),
                field(role, "id"),
                yes_dash(bool_field(Some(role), "is_default")),
                if bool_field(Some(role), "is_builtin") {
                    "内置".to_owned()
                } else {
                    "自定义".to_owned()
                },
                field(role, "status"),
                role.get("tts")
                    .and_then(|tts| string_field(Some(tts), "vcn"))
                    .unwrap_or_else(|| "-".to_owned()),
            ]
        })
        .collect::<Vec<_>>();
    Ok(with_page_summary(
        render_table(&["角色", "ID", "默认", "类型", "状态", "发音人"], &rows),
        value,
        "角色",
        rows.len(),
    ))
}

pub fn render_management_role_detail(value: &Value) -> Result<String> {
    let role = response_data(value)?;
    let role_type = if bool_field(Some(role), "is_builtin") {
        "内置"
    } else {
        "自定义"
    };
    let mut output = [
        ("角色", field(role, "name")),
        ("ID", field(role, "id")),
        ("描述", field(role, "description")),
        ("状态", field(role, "status")),
        ("默认", yes_no(bool_field(Some(role), "is_default"))),
        ("类型", role_type.to_owned()),
        ("创建时间", field(role, "created_at")),
    ]
    .into_iter()
    .map(|(label, value)| format!("{label}: {value}"))
    .collect::<Vec<_>>()
    .join("\n");

    if let Some(tts) = role.get("tts") {
        output.push_str(&format!(
            "\n\nTTS:\n  发音人: {}\n  语速: {}\n  音量: {}",
            field(tts, "vcn"),
            field(tts, "speed"),
            field(tts, "volume")
        ));
    }
    if let Some(persona) = role.get("persona").and_then(Value::as_str) {
        if !persona.is_empty() {
            output.push_str("\n\n人设:\n");
            output.push_str(persona);
        }
    }

    let knowledge = role
        .get("knowledge")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    if knowledge.is_empty() {
        output.push_str("\n\n知识库: 无");
    } else {
        let rows = knowledge
            .iter()
            .map(|item| {
                vec![
                    first_field(item, &["index_id", "id"]),
                    first_field(item, &["name", "index_name", "type"]),
                ]
            })
            .collect::<Vec<_>>();
        output.push_str("\n\n知识库:\n");
        output.push_str(&render_table(&["ID", "名称/类型"], &rows));
    }

    let wakewords = role
        .get("wakeup_words")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    if !wakewords.is_empty() {
        let rows = wakewords
            .iter()
            .map(|word| {
                vec![
                    field(word, "name"),
                    field(word, "id"),
                    yes_dash(bool_field(Some(word), "is_default")),
                    field(word, "sensitivity"),
                    field(word, "status"),
                ]
            })
            .collect::<Vec<_>>();
        output.push_str("\n\n唤醒词:\n");
        output.push_str(&render_table(
            &["唤醒词", "ID", "默认", "灵敏度", "状态"],
            &rows,
        ));
    }

    if let Some(guide) = role.get("idle_guide") {
        let resources = guide
            .get("resources")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        output.push_str(&format!(
            "\n\n闲时引导（间隔 {} ms）:",
            field(guide, "interval_ms")
        ));
        if resources.is_empty() {
            output.push_str(" 无");
        } else {
            for (index, resource) in resources.iter().enumerate() {
                output.push_str(&format!("\n  {}. {}", index + 1, field(resource, "text")));
            }
        }
    }

    let expressions = role
        .get("expressions")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    if !expressions.is_empty() {
        let rows = expressions
            .iter()
            .map(|expression| {
                vec![
                    field(expression, "name"),
                    field(expression, "preset_key"),
                    field(expression, "type"),
                    yes_no(bool_field(Some(expression), "usable")),
                ]
            })
            .collect::<Vec<_>>();
        output.push_str("\n\n表情资源:\n");
        output.push_str(&render_table(&["名称", "Key", "类型", "可用"], &rows));
        output.push_str(&format!("\n共 {} 个表情资源。", rows.len()));
    }
    Ok(output)
}

pub fn render_management_ota_list(value: &Value) -> Result<String> {
    render_resource_list(
        value,
        &["ID", "版本", "版本号", "模式", "状态", "描述"],
        "OTA 包",
        |item| {
            vec![
                first_field(item, &["id", "package_id"]),
                field(item, "version"),
                first_field(item, &["version_number", "versionNumber"]),
                first_field(item, &["ota_mode", "otaMode"]),
                first_field(item, &["status", "publish_status"]),
                field(item, "description"),
            ]
        },
    )
}

pub fn render_management_ota_whitelist(value: &Value) -> Result<String> {
    render_resource_list(
        value,
        &["设备 ID", "状态", "创建时间"],
        "白名单设备",
        |item| {
            vec![
                first_field(item, &["device_id", "id", "sn"]),
                field(item, "status"),
                first_field(item, &["created_at", "createdAt"]),
            ]
        },
    )
}

pub fn render_management_app_kbs(value: &Value) -> Result<String> {
    render_resource_list(
        value,
        &["知识库 ID", "类型", "名称"],
        "关联知识库",
        |item| {
            vec![
                first_field(item, &["index_id", "id"]),
                field(item, "type"),
                first_field(item, &["name", "index_name"]),
            ]
        },
    )
}

pub fn render_management_lexicon(value: &Value) -> Result<String> {
    render_resource_list(value, &["ID", "词汇"], "专业词汇", |item| {
        vec![
            first_field(item, &["id", "hotword_id"]),
            generic_label(item),
        ]
    })
}

pub fn render_management_tones(value: &Value) -> Result<String> {
    render_resource_list(
        value,
        &["Key", "名称", "文案", "默认"],
        "提示语",
        |item| {
            vec![
                field(item, "key"),
                field(item, "name"),
                field(item, "text"),
                yes_dash(bool_field(Some(item), "is_default")),
            ]
        },
    )
}

pub fn render_management_mcps(value: &Value) -> Result<String> {
    render_resource_list(
        value,
        &["名称", "Server ID", "启用", "类型", "状态", "工具数"],
        "MCP 服务器",
        |item| {
            vec![
                field(item, "name"),
                first_field(item, &["server_id", "id"]),
                yes_no(bool_field(Some(item), "enabled")),
                if bool_field(Some(item), "built_in") {
                    "内置".to_owned()
                } else {
                    field(item, "transport_type")
                },
                field(item, "tool_status"),
                array_len(Some(item), "tools").to_string(),
            ]
        },
    )
}

fn render_resource_list<F>(value: &Value, headers: &[&str], noun: &str, row: F) -> Result<String>
where
    F: Fn(&Value) -> Vec<String>,
{
    let items = response_items(value)?;
    if items.is_empty() {
        return Ok(format!("暂无{noun}。"));
    }
    let rows = items.iter().map(row).collect::<Vec<_>>();
    Ok(with_page_summary(
        render_table(headers, &rows),
        value,
        noun,
        rows.len(),
    ))
}

fn response_data(value: &Value) -> Result<&Value> {
    value.get("data").context("服务端响应缺少 data 字段")
}

fn response_items(value: &Value) -> Result<&[Value]> {
    response_data(value)?
        .as_array()
        .map(Vec::as_slice)
        .context("服务端响应的 data 不是数组")
}

fn with_page_summary(mut output: String, value: &Value, noun: &str, shown: usize) -> String {
    let total = value
        .get("total")
        .and_then(Value::as_u64)
        .unwrap_or(shown as u64);
    let page = value.get("page").and_then(Value::as_u64);
    let page_size = value.get("pageSize").and_then(Value::as_u64);
    match (page, page_size) {
        (Some(page), Some(page_size)) => output.push_str(&format!(
            "\n共 {total} 个{noun}；当前第 {page} 页，每页 {page_size} 个。"
        )),
        _ => output.push_str(&format!("\n共 {total} 个{noun}。")),
    }
    output
}

fn scalar(value: &Value) -> String {
    match value {
        Value::Null => "-".to_owned(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn config_value(value: &Value, key: &str) -> String {
    match value.get(key) {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Null) | None => "-".to_owned(),
        Some(other) => other.to_string(),
    }
}

fn config_preview(value: &str, max_chars: usize) -> String {
    let single_line = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if single_line.chars().count() <= max_chars {
        return single_line;
    }
    let mut preview = single_line.chars().take(max_chars).collect::<String>();
    preview.push('…');
    preview
}

fn config_field_constraint(value: &Value, key: &str) -> String {
    let Some(field) = value
        .get("editable_fields")
        .and_then(|fields| fields.get(key))
    else {
        return "-".to_owned();
    };
    if let Some(values) = field.get("values").and_then(Value::as_array) {
        return values
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(" / ");
    }
    let kind = match field.get("type").and_then(Value::as_str) {
        Some("url") => "URL",
        Some("string") => "文本",
        Some(other) => other,
        None => "-",
    };
    let mut constraint = kind.to_owned();
    if let Some(max_length) = field.get("max_length").and_then(Value::as_u64) {
        constraint.push_str(&format!(" ≤{max_length}"));
    }
    if field
        .get("empty_restores_default")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        constraint.push_str("；空值=默认");
    }
    if field
        .get("write_only")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        constraint.push_str("；只写");
    }
    constraint
}

fn generic_label(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => string_field(Some(other), "word")
            .or_else(|| string_field(Some(other), "name"))
            .or_else(|| string_field(Some(other), "text"))
            .unwrap_or_else(|| other.to_string()),
    }
}

fn yes_dash(flag: bool) -> String {
    if flag {
        "是".to_owned()
    } else {
        "-".to_owned()
    }
}

fn yes_no(flag: bool) -> String {
    if flag {
        "是".to_owned()
    } else {
        "否".to_owned()
    }
}

fn first_field(value: &Value, keys: &[&str]) -> String {
    keys.iter()
        .find_map(|key| value.get(key).map(|_| field(value, key)))
        .unwrap_or_else(|| "-".to_owned())
}

fn first_bool(value: &Value, keys: &[&str]) -> bool {
    keys.iter()
        .find_map(|key| value.get(key).and_then(Value::as_bool))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_project() -> Value {
        json!({
            "data": {
                "product": {
                    "secret": "3cf268dd-50ba-460d-ad75-6e06eb5a9bce",
                    "deviceAuthCheck": true,
                    "assignedDeviceQuota": 10,
                    "consumedDeviceQuota": 2
                },
                "apps": [{
                    "config": {
                        "llm_roles": [{
                            "id": "2b3adad0",
                            "name": "湾湾女孩",
                            "tts": {"vcn": "s_volc_zh", "volume": 50, "speed": 50},
                            "knowledge": [],
                            "is_builtin": true,
                            "is_default": true
                        }],
                        "llm_feature": {
                            "hotwords": ["聆思", {"word": "CSK6"}],
                            "knowledge": [{"id": "kb-1", "name": "产品手册"}]
                        },
                        "default_wakeup_word": {"name": "小聆小聆", "sensitivity": "medium"},
                        "interaction_mode": 1,
                        "prompt_tone_texts": [
                            {"name": "网络连接成功", "key": "network_suc", "text": "网络连接成功", "is_default": true}
                        ]
                    }
                }]
            }
        })
    }

    #[test]
    fn renders_device_quota() {
        let out = render_device_quota(&sample_project()).unwrap();
        assert!(out.contains("10"));
        assert!(out.contains("2"));
        assert!(out.contains("开启"));
    }

    #[test]
    fn renders_role_list_with_wakeword() {
        let out = render_role_list(&sample_project()).unwrap();
        assert!(out.contains("湾湾女孩"));
        assert!(out.contains("s_volc_zh"));
        assert!(out.contains("小聆小聆"));
    }

    #[test]
    fn renders_interact_mode() {
        let out = render_interact_mode(&sample_project()).unwrap();
        assert!(out.contains("full-duplex"));
        assert!(out.contains("全双工"));
    }

    #[test]
    fn renders_lexicon_mixed_shapes() {
        let out = render_lexicon_list(&sample_project()).unwrap();
        assert!(out.contains("聆思"));
        assert!(out.contains("CSK6"));
    }

    #[test]
    fn renders_app_kb_list() {
        let out = render_app_kb_list(&sample_project()).unwrap();
        assert!(out.contains("kb-1"));
        assert!(out.contains("产品手册"));
    }

    #[test]
    fn reads_product_secret() {
        assert_eq!(
            product_secret(&sample_project()).unwrap(),
            "3cf268dd-50ba-460d-ad75-6e06eb5a9bce"
        );
    }

    #[test]
    fn masked_secret_is_rejected() {
        let value = json!({"data": {"product": {"secret": "3cf26*******a9bce"}}});
        assert!(product_secret(&value).is_none());
    }

    #[test]
    fn renders_management_role_list_and_page() {
        let value = json!({
            "data": [{
                "id": "role-1",
                "name": "小聆老师",
                "is_default": true,
                "is_builtin": true,
                "status": "active",
                "tts": {"vcn": "x4_lingxiaoyue_oral"}
            }],
            "page": 1,
            "pageSize": 20,
            "total": 3
        });
        let output = render_management_role_list(&value).unwrap();
        assert!(output.contains("小聆老师"));
        assert!(output.contains("role-1"));
        assert!(output.contains("共 3 个角色"));
        assert!(!output.contains('{'));
    }

    #[test]
    fn renders_management_role_detail_sections() {
        let value = json!({
            "data": {
                "id": "role-1",
                "name": "小聆老师",
                "description": "默认角色",
                "status": "active",
                "is_default": true,
                "is_builtin": true,
                "persona": "先给结论，再讲理由。",
                "tts": {"vcn": "x4_lingxiaoyue_oral", "speed": 50, "volume": 50},
                "knowledge": [],
                "wakeup_words": [{
                    "id": "word-1",
                    "name": "小聆小聆",
                    "is_default": true,
                    "sensitivity": "medium",
                    "status": "ready"
                }],
                "idle_guide": {
                    "interval_ms": 3000,
                    "resources": [{"text": "#唤醒词#，今天有什么新鲜事"}]
                },
                "expressions": [{
                    "name": "开心",
                    "preset_key": "happy",
                    "type": "PRESET",
                    "usable": true
                }]
            }
        });
        let output = render_management_role_detail(&value).unwrap();
        assert!(output.contains("角色: 小聆老师"));
        assert!(output.contains("人设:"));
        assert!(output.contains("唤醒词:"));
        assert!(output.contains("闲时引导（间隔 3000 ms）"));
        assert!(output.contains("表情资源:"));
        assert!(!output.contains("\"persona\""));
    }

    #[test]
    fn renders_management_lists_without_json_fallback() {
        let lexicon = json!({
            "data": [{"id": "hotword-1", "word": "聆思"}],
            "page": 1,
            "pageSize": 20,
            "total": 1
        });
        let tone = json!({
            "data": [{
                "key": "network_suc",
                "name": "网络连接成功",
                "text": "网络连接成功",
                "is_default": true
            }]
        });
        let mcp = json!({
            "data": [{
                "name": "天气查询工具",
                "server_id": "weather",
                "enabled": true,
                "built_in": true,
                "tool_status": "ready",
                "tools": [{"name": "get_weather"}]
            }]
        });
        assert!(render_management_lexicon(&lexicon)
            .unwrap()
            .contains("hotword-1"));
        assert!(render_management_tones(&tone)
            .unwrap()
            .contains("network_suc"));
        assert!(render_management_mcps(&mcp)
            .unwrap()
            .contains("天气查询工具"));
    }

    #[test]
    fn renders_management_capabilities_as_table() {
        let value = json!({
            "data": {
                "api_version": "v1",
                "revision": "2026-07-24",
                "capabilities": {
                    "project.agent.model": 1
                }
            }
        });
        let output = render_management_capabilities(&value).unwrap();
        assert!(output.contains("API 版本: v1"));
        assert!(output.contains("project.agent.model"));
        assert!(!output.contains('{'));
    }

    #[test]
    fn renders_management_config_with_editable_fields() {
        let long_prompt = format!("第一行\n第二行 {}", "很长的系统提示词".repeat(12));
        let value = json!({
            "interaction_mode": "full-duplex",
            "system_prompt": long_prompt.clone(),
            "protocol": "chat_completions",
            "endpoint": "https://example.com/v1",
            "model": "deepseek-chat",
            "authorization_configured": true,
            "editable_fields": {
                "interaction_mode": {
                    "type": "enum",
                    "values": ["oneshot", "full-duplex", "half-duplex"]
                },
                "system_prompt": {
                    "type": "string",
                    "max_length": 20000,
                    "empty_restores_default": true
                },
                "protocol": {
                    "type": "enum",
                    "values": ["chat_completions"]
                },
                "endpoint": {"type": "url", "max_length": 2048},
                "model": {"type": "string", "max_length": 256},
                "authorization": {
                    "type": "string",
                    "max_length": 8192,
                    "write_only": true
                }
            }
        });

        let output = render_management_config(&value).unwrap();

        assert!(output.contains("Key"));
        assert!(output.contains("interaction_mode"));
        assert!(output.contains("system_prompt"));
        assert!(output.contains("authorization"));
        assert!(output.contains("已配置（密钥不可读取）"));
        assert!(output.contains("第一行 第二行"));
        assert!(output.contains('…'));
        assert!(!output.contains(&long_prompt));
        assert!(output.contains("oneshot / full-duplex / half-duplex"));
        assert!(output.contains("chat_completions"));
        assert!(output.contains("文本 ≤8192；只写"));
    }
}
