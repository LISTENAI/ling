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

pub fn render_management_config(value: &Value) -> Result<String> {
    let prompt = value
        .get("system_prompt")
        .and_then(Value::as_str)
        .unwrap_or("");
    let description = value
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("");
    let rows = vec![
        vec![
            "name".to_owned(),
            "应用名称".to_owned(),
            config_value(value, "name"),
            config_field_constraint(value, "name"),
        ],
        vec![
            "description".to_owned(),
            "应用描述".to_owned(),
            if description.is_empty() {
                "（空）".to_owned()
            } else {
                config_preview(description, 32)
            },
            config_field_constraint(value, "description"),
        ],
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

pub fn framework_agent_version(value: &Value) -> Result<&str> {
    response_data(value)?
        .get("version")
        .and_then(Value::as_str)
        .context("链路版本响应缺少 data.version")
}

pub fn render_framework_agent_version(value: &Value) -> Result<String> {
    let version = framework_agent_version(value)?;
    let (mode, version) = if version.is_empty() {
        ("managed", "官方最新版本")
    } else {
        ("custom", version)
    };
    Ok(render_table(
        &["配置", "当前值"],
        &[
            vec!["模式".to_owned(), mode.to_owned()],
            vec!["版本".to_owned(), version.to_owned()],
        ],
    ))
}

pub fn render_framework_agent_versions(value: &Value, current_version: &str) -> Result<String> {
    let data = response_data(value)?;
    let items = data
        .get("items")
        .and_then(Value::as_array)
        .context("版本列表响应缺少 data.items 数组")?;
    let current = if current_version.is_empty() {
        "managed（官方最新版本）".to_owned()
    } else {
        format!("custom（{current_version}）")
    };
    if items.is_empty() {
        return Ok(format!(
            "当前测试链路：{current}\n\n暂无已上传的自定义 Agent 版本。"
        ));
    }
    let rows = items
        .iter()
        .map(|item| {
            let version = field(item, "version");
            vec![
                if version == current_version {
                    "当前".to_owned()
                } else {
                    "-".to_owned()
                },
                version,
                config_preview(&field(item, "version_name"), 28),
                field(item, "sdk_version"),
                item.get("file_size")
                    .and_then(Value::as_u64)
                    .map(format_file_size)
                    .unwrap_or_else(|| "-".to_owned()),
                field(item, "created_at"),
            ]
        })
        .collect::<Vec<_>>();
    let mut output = format!(
        "当前测试链路：{current}\n\n{}",
        render_table(&["状态", "版本", "名称", "SDK", "大小", "创建时间"], &rows)
    );
    let total = data
        .get("total")
        .and_then(Value::as_u64)
        .unwrap_or(rows.len() as u64);
    let page = data.get("page").and_then(Value::as_u64).unwrap_or(1);
    let page_size = data
        .get("page_size")
        .and_then(Value::as_u64)
        .unwrap_or(rows.len() as u64);
    output.push_str(&format!(
        "\n共 {total} 个版本；当前第 {page} 页，每页 {page_size} 个。"
    ));
    output.push_str("\n使用 `ling app chain set custom <version>` 切换测试链路。");
    Ok(output)
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
    let knowledge = role
        .get("knowledge")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let guide = role.get("idle_guide");
    let guide_resources = guide
        .and_then(|guide| guide.get("resources"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let tts = role.get("tts");
    let mut output = [
        ("角色 ID", field(role, "id")),
        ("状态", field(role, "status")),
        ("默认", yes_no(bool_field(Some(role), "is_default"))),
        ("类型", role_type.to_owned()),
        ("创建时间", field(role, "created_at")),
    ]
    .into_iter()
    .map(|(label, value)| format!("{label}: {value}"))
    .collect::<Vec<_>>()
    .join("\n");

    output.push_str("\n\n可编辑配置:\n");
    output.push_str(&render_table(
        &["Key", "配置", "当前值", "可用值/格式"],
        &[
            vec![
                "name".to_owned(),
                "角色名称".to_owned(),
                role_text_preview(role, "name", 28),
                "非空文本 ≤12".to_owned(),
            ],
            vec![
                "persona".to_owned(),
                "角色描述".to_owned(),
                role_text_preview(role, "persona", 28),
                "文本 ≤2000".to_owned(),
            ],
            vec![
                "avatar_url".to_owned(),
                "头像".to_owned(),
                role_text_preview(role, "avatar_url", 28),
                "URL ≤2048".to_owned(),
            ],
            vec![
                "vcn".to_owned(),
                "发音人".to_owned(),
                tts.map(|tts| field(tts, "vcn"))
                    .unwrap_or_else(|| "-".to_owned()),
                "VCN ID".to_owned(),
            ],
            vec![
                "volume".to_owned(),
                "音量".to_owned(),
                tts.map(|tts| field(tts, "volume"))
                    .unwrap_or_else(|| "-".to_owned()),
                "数字".to_owned(),
            ],
            vec![
                "speed".to_owned(),
                "语速".to_owned(),
                tts.map(|tts| field(tts, "speed"))
                    .unwrap_or_else(|| "-".to_owned()),
                "数字".to_owned(),
            ],
            vec![
                "knowledge".to_owned(),
                "知识库关联".to_owned(),
                item_count(knowledge.len(), "项"),
                "JSON 数组".to_owned(),
            ],
            vec![
                "idle_guide.interval_ms".to_owned(),
                "闲时引导间隔".to_owned(),
                guide
                    .map(|guide| field(guide, "interval_ms"))
                    .unwrap_or_else(|| "-".to_owned()),
                "数字（毫秒）".to_owned(),
            ],
            vec![
                "idle_guide.resources".to_owned(),
                "闲时引导文案".to_owned(),
                item_count(guide_resources.len(), "条"),
                "JSON 数组 ≤10".to_owned(),
            ],
        ],
    ));

    if let Some(persona) = role.get("persona").and_then(Value::as_str) {
        if !persona.is_empty() {
            output.push_str("\n\n角色描述全文（persona）:\n");
            output.push_str(persona);
        }
    }

    if knowledge.is_empty() {
        output.push_str("\n\n知识库详情（knowledge）: 无");
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
        output.push_str("\n\n知识库详情（knowledge）:\n");
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
        output.push_str("\n\n唤醒词（使用 `role wakeword` 管理）:\n");
        output.push_str(&render_table(
            &["唤醒词", "ID", "默认", "灵敏度", "状态"],
            &rows,
        ));
    }

    if guide_resources.is_empty() {
        output.push_str("\n\n闲时引导文案（idle_guide.resources）: 无");
    } else {
        let rows = guide_resources
            .iter()
            .enumerate()
            .map(|(index, resource)| vec![(index + 1).to_string(), field(resource, "text")])
            .collect::<Vec<_>>();
        output.push_str("\n\n闲时引导文案（idle_guide.resources）:\n");
        output.push_str(&render_table(&["序号", "文案"], &rows));
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

fn role_text_preview(role: &Value, key: &str, max_chars: usize) -> String {
    match role.get(key).and_then(Value::as_str) {
        Some("") => "（空）".to_owned(),
        Some(value) => config_preview(value, max_chars),
        None => "-".to_owned(),
    }
}

fn item_count(count: usize, noun: &str) -> String {
    if count == 0 {
        "无".to_owned()
    } else {
        format!("{count} {noun}")
    }
}

pub fn wakeup_word_status(status: &str) -> &'static str {
    match status {
        "pending" => "等待生成",
        "training" => "生成中",
        "ready" => "可用",
        "failed" => "生成失败",
        _ => "未知",
    }
}

fn wakeup_word_sensitivity(value: &str) -> &'static str {
    match value {
        "high" => "高",
        "medium" => "中",
        "low" => "低",
        _ => "未知",
    }
}

fn wakeup_word_response_texts(value: &Value) -> Vec<String> {
    value
        .get("responses")
        .and_then(Value::as_array)
        .map(|responses| {
            responses
                .iter()
                .filter_map(|response| {
                    response
                        .get("text")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn render_management_wakeup_word_list(value: &Value) -> Result<String> {
    let items = response_items(value)?;
    if items.is_empty() {
        return Ok("暂无唤醒词。".to_owned());
    }
    let rows = items
        .iter()
        .map(|item| {
            let responses = wakeup_word_response_texts(item).join(" / ");
            vec![
                field(item, "name"),
                field(item, "id"),
                wakeup_word_status(
                    item.get("status")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                )
                .to_owned(),
                wakeup_word_sensitivity(
                    item.get("sensitivity")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                )
                .to_owned(),
                if bool_field(Some(item), "is_system") {
                    "系统".to_owned()
                } else {
                    "生成".to_owned()
                },
                config_preview(&responses, 36),
            ]
        })
        .collect::<Vec<_>>();
    Ok(with_page_summary(
        render_table(&["唤醒词", "ID", "状态", "灵敏度", "类型", "应答语"], &rows),
        value,
        "唤醒词",
        rows.len(),
    ))
}

pub fn render_management_wakeup_word_detail(value: &Value) -> Result<String> {
    let item = response_data(value)?;
    let status = item
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let sensitivity = item
        .get("sensitivity")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut output = [
        ("唤醒词", field(item, "name")),
        ("ID", field(item, "id")),
        ("状态", wakeup_word_status(status).to_owned()),
        ("灵敏度", wakeup_word_sensitivity(sensitivity).to_owned()),
        (
            "类型",
            if bool_field(Some(item), "is_system") {
                "系统".to_owned()
            } else {
                "生成".to_owned()
            },
        ),
        ("创建时间", field(item, "created_at")),
        ("更新时间", field(item, "updated_at")),
    ]
    .into_iter()
    .map(|(label, value)| format!("{label}: {value}"))
    .collect::<Vec<_>>()
    .join("\n");
    let responses = wakeup_word_response_texts(item);
    output.push_str("\n\n应答语:");
    if responses.is_empty() {
        output.push_str(" 无");
    } else {
        for (index, response) in responses.iter().enumerate() {
            output.push_str(&format!("\n  {}. {response}", index + 1));
        }
    }
    Ok(output)
}

pub fn render_wakeup_word_responses(value: &Value) -> Result<String> {
    let data = response_data(value)?;
    let responses = wakeup_word_response_texts(data);
    if responses.is_empty() {
        return Ok("暂无唤醒应答语。".to_owned());
    }
    Ok(render_table(
        &["序号", "应答语"],
        &responses
            .iter()
            .enumerate()
            .map(|(index, text)| vec![(index + 1).to_string(), text.clone()])
            .collect::<Vec<_>>(),
    ))
}

pub fn render_role_wakeup_word(role_id: &str, value: &Value) -> Result<String> {
    let detail = render_management_wakeup_word_detail(value)?;
    Ok(format!("角色 ID: {role_id}\n\n{detail}"))
}

pub fn render_management_ota_list(value: &Value) -> Result<String> {
    render_resource_list(
        value,
        &["OTA 包 ID", "版本", "版本号", "模式", "状态", "描述"],
        "OTA 包",
        |item| {
            vec![
                field(item, "package_id"),
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
    let items = response_items(value)?;
    if items.is_empty() {
        return Ok("暂无白名单设备。".to_owned());
    }

    let has_metadata = items.iter().any(|item| {
        item.get("status").is_some()
            || item.get("created_at").is_some()
            || item.get("createdAt").is_some()
    });
    let (headers, rows) = if has_metadata {
        (
            vec!["设备 ID", "状态", "创建时间"],
            items
                .iter()
                .map(|item| {
                    vec![
                        item.as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| first_field(item, &["device_id", "id", "sn"])),
                        field(item, "status"),
                        first_field(item, &["created_at", "createdAt"]),
                    ]
                })
                .collect::<Vec<_>>(),
        )
    } else {
        (
            vec!["设备 ID"],
            items
                .iter()
                .map(|item| {
                    vec![item
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| first_field(item, &["device_id", "id", "sn"]))]
                })
                .collect::<Vec<_>>(),
        )
    };

    Ok(with_page_summary(
        render_table(&headers, &rows),
        value,
        "白名单设备",
        rows.len(),
    ))
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
        &["名称", "ID", "Server ID", "启用", "类型", "状态", "工具数"],
        "MCP 服务器",
        |item| {
            vec![
                field(item, "name"),
                field(item, "id"),
                field(item, "server_id"),
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

pub fn render_management_mcp_detail(value: &Value) -> Result<String> {
    let mcp = response_data(value)?;
    let built_in = bool_field(Some(mcp), "built_in");
    let authorization_configured = mcp
        .get("authorization")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.is_empty())
        || mcp
            .get("authorization_configured")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    let mut output = [
        ("名称", field(mcp, "name")),
        ("ID", field(mcp, "id")),
        ("Server ID", field(mcp, "server_id")),
        ("类型", if built_in { "内置" } else { "外部" }.to_owned()),
        ("启用", yes_no(bool_field(Some(mcp), "enabled"))),
        ("传输协议", field(mcp, "transport_type")),
        ("URL", field(mcp, "url")),
        ("描述", field(mcp, "description")),
        (
            "Authorization",
            if authorization_configured {
                "已配置（密钥不可读取）".to_owned()
            } else {
                "未配置".to_owned()
            },
        ),
        ("工具状态", field(mcp, "tool_status")),
        ("上次检查", field(mcp, "tool_last_checked_at")),
    ]
    .into_iter()
    .map(|(label, value)| format!("{label}: {value}"))
    .collect::<Vec<_>>()
    .join("\n");

    let tools = mcp
        .get("tools")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    if tools.is_empty() {
        output.push_str("\n\n工具: 无");
    } else {
        let rows = tools
            .iter()
            .map(|tool| {
                vec![
                    first_field(tool, &["name", "tool_name"]),
                    config_preview(&field(tool, "description"), 48),
                ]
            })
            .collect::<Vec<_>>();
        output.push_str("\n\n工具:\n");
        output.push_str(&render_table(&["名称", "描述"], &rows));
    }

    output.push_str("\n\n可编辑字段:\n");
    let mut rows = vec![vec![
        "enabled".to_owned(),
        "启用状态".to_owned(),
        "true / false".to_owned(),
    ]];
    if !built_in {
        rows.splice(
            0..0,
            [
                vec![
                    "name".to_owned(),
                    "显示名称".to_owned(),
                    "非空文本 ≤20".to_owned(),
                ],
                vec![
                    "server_id".to_owned(),
                    "服务标识".to_owned(),
                    "[A-Za-z0-9_-]，1–20 字符".to_owned(),
                ],
                vec![
                    "transport_type".to_owned(),
                    "传输协议".to_owned(),
                    "sse / http".to_owned(),
                ],
                vec![
                    "url".to_owned(),
                    "服务地址".to_owned(),
                    "URL ≤1000".to_owned(),
                ],
                vec![
                    "description".to_owned(),
                    "描述".to_owned(),
                    "文本".to_owned(),
                ],
                vec![
                    "authorization".to_owned(),
                    "鉴权信息".to_owned(),
                    "文本 ≤500；只写".to_owned(),
                ],
            ],
        );
    }
    output.push_str(&render_table(&["Key", "配置", "可用值/格式"], &rows));
    Ok(output)
}

pub fn redact_mcp_credentials(value: &Value) -> Value {
    let mut value = value.clone();
    let redact = |item: &mut Value| {
        let Some(object) = item.as_object_mut() else {
            return;
        };
        let configured = object
            .remove("authorization")
            .and_then(|value| value.as_str().map(|value| !value.is_empty()))
            .unwrap_or(false);
        if configured {
            object.insert("authorization_configured".to_owned(), Value::Bool(true));
        }
    };
    match value.get_mut("data") {
        Some(Value::Array(items)) => {
            for item in items {
                redact(item);
            }
        }
        Some(item) => redact(item),
        None => redact(&mut value),
    }
    value
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

fn format_file_size(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    if bytes < 1024 * 1024 {
        return format!("{:.1} KiB", bytes as f64 / 1024.0);
    }
    format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
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
        .get("non_empty")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        constraint.push_str("；非空");
    }
    if field
        .get("empty_clears")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        constraint.push_str("；空值=清除");
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
        assert!(output.contains("角色 ID: role-1"));
        assert!(output.contains("可编辑配置:"));
        assert!(output.contains("当前值"));
        assert!(output.contains("name"));
        assert!(output.contains("小聆老师"));
        assert!(output.contains("persona"));
        assert!(output.contains("先给结论，再讲理由。"));
        assert!(!output.contains("description"));
        assert!(!output.contains("默认角色"));
        assert!(output.contains("vcn"));
        assert!(output.contains("x4_lingxiaoyue_oral"));
        assert!(output.contains("idle_guide.interval_ms"));
        assert!(output.contains("3000"));
        assert!(output.contains("idle_guide.resources"));
        assert!(output.contains("1 条"));
        assert!(output.contains("角色描述全文（persona）:"));
        assert!(output.contains("唤醒词（使用 `role wakeword` 管理）:"));
        assert!(output.contains("闲时引导文案（idle_guide.resources）:"));
        assert!(output.contains("表情资源:"));
        assert!(!output.contains("\"persona\""));
    }

    #[test]
    fn renders_wakeup_word_list_and_detail() {
        let item = json!({
            "id": "word-1",
            "name": "小聆小聆",
            "sensitivity": "medium",
            "status": "ready",
            "is_system": true,
            "responses": [{"text": "你好"}, {"text": "我在"}],
            "created_at": "2026-07-29 10:00:00",
            "updated_at": "2026-07-29 10:01:00"
        });
        let list = json!({
            "data": [item.clone()],
            "page": 1,
            "pageSize": 20,
            "total": 1
        });
        let output = render_management_wakeup_word_list(&list).unwrap();
        assert!(output.contains("小聆小聆"));
        assert!(output.contains("word-1"));
        assert!(output.contains("可用"));
        assert!(output.contains("你好 / 我在"));
        assert!(output.contains("共 1 个唤醒词"));

        let detail = render_management_wakeup_word_detail(&json!({"data": item})).unwrap();
        assert!(detail.contains("灵敏度: 中"));
        assert!(detail.contains("类型: 系统"));
        assert!(detail.contains("1. 你好"));
        assert!(detail.contains("2. 我在"));
    }

    #[test]
    fn renders_wakeup_word_responses_and_role_assignment() {
        let responses = json!({
            "data": {
                "responses": [{"text": "你好"}, {"text": "我在"}]
            }
        });
        let output = render_wakeup_word_responses(&responses).unwrap();
        assert!(output.contains("序号"));
        assert!(output.contains("你好"));
        assert!(!output.contains('{'));

        let detail = json!({
            "data": {
                "id": "word-1",
                "name": "小聆小聆",
                "sensitivity": "medium",
                "status": "ready",
                "is_system": true,
                "responses": [{"text": "你好"}]
            }
        });
        let output = render_role_wakeup_word("role-1", &detail).unwrap();
        assert!(output.contains("角色 ID: role-1"));
        assert!(output.contains("唤醒词: 小聆小聆"));
    }

    #[test]
    fn renders_ota_package_id_instead_of_internal_record_id() {
        let value = json!({
            "data": [{
                "id": 1979,
                "package_id": "33348d36417b86caf8f174db332ae644",
                "version": "0.1.0",
                "version_number": 1,
                "ota_mode": "selectable",
                "status": 0
            }],
            "page": 1,
            "pageSize": 20,
            "total": 1
        });
        let output = render_management_ota_list(&value).unwrap();
        assert!(output.contains("OTA 包 ID"));
        assert!(output.contains("33348d36417b86caf8f174db332ae644"));
        assert!(!output.contains("1979"));
    }

    #[test]
    fn renders_ota_whitelist_string_items_as_device_ids() {
        let value = json!({
            "code": "SUCCESS",
            "data": ["ling_132456"],
            "message": "查询成功",
            "page": 1,
            "pageSize": 20,
            "total": 1
        });

        let output = render_management_ota_whitelist(&value).unwrap();

        assert!(output.contains("设备 ID"));
        assert!(output.contains("ling_132456"));
        assert!(output.contains("共 1 个白名单设备"));
        assert!(!output.contains("│ - "));
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
                "id": "mcp-record-1",
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
        assert!(render_management_mcps(&mcp)
            .unwrap()
            .contains("mcp-record-1"));
    }

    #[test]
    fn renders_mcp_detail_and_redacts_authorization() {
        let value = json!({
            "data": {
                "id": "mcp-record-1",
                "name": "天气查询工具",
                "server_id": "weather",
                "enabled": true,
                "built_in": false,
                "transport_type": "http",
                "url": "https://mcp.example.com",
                "description": "查询天气",
                "authorization": "Bearer secret-token",
                "tool_status": "ready",
                "tools": [{
                    "name": "get_weather",
                    "description": "按城市查询天气"
                }]
            }
        });

        let output = render_management_mcp_detail(&value).unwrap();
        assert!(output.contains("ID: mcp-record-1"));
        assert!(output.contains("get_weather"));
        assert!(output.contains("authorization"));
        assert!(output.contains("文本 ≤500；只写"));
        assert!(output.contains("已配置（密钥不可读取）"));
        assert!(!output.contains("secret-token"));

        let redacted = redact_mcp_credentials(&value);
        assert!(redacted["data"].get("authorization").is_none());
        assert_eq!(redacted["data"]["authorization_configured"], true);
        assert!(!redacted.to_string().contains("secret-token"));
    }

    #[test]
    fn renders_management_config_with_editable_fields() {
        let long_prompt = format!("第一行\n第二行 {}", "很长的系统提示词".repeat(12));
        let value = json!({
            "name": "设备助手",
            "description": "",
            "interaction_mode": "full-duplex",
            "system_prompt": long_prompt.clone(),
            "protocol": "chat_completions",
            "endpoint": "https://example.com/v1",
            "model": "deepseek-chat",
            "authorization_configured": true,
            "editable_fields": {
                "name": {
                    "type": "string",
                    "max_length": 30,
                    "non_empty": true
                },
                "description": {
                    "type": "string",
                    "max_length": 60,
                    "empty_clears": true
                },
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
        assert!(output.contains("name"));
        assert!(output.contains("设备助手"));
        assert!(output.contains("description"));
        assert!(output.contains("（空）"));
        assert!(output.contains("interaction_mode"));
        assert!(output.contains("system_prompt"));
        assert!(output.contains("authorization"));
        assert!(output.contains("已配置（密钥不可读取）"));
        assert!(output.contains("第一行 第二行"));
        assert!(output.contains('…'));
        assert!(!output.contains(&long_prompt));
        assert!(output.contains("oneshot / full-duplex / half-duplex"));
        assert!(output.contains("chat_completions"));
        assert!(output.contains("文本 ≤30；非空"));
        assert!(output.contains("文本 ≤60；空值=清除"));
        assert!(output.contains("文本 ≤8192；只写"));
    }

    #[test]
    fn renders_framework_agent_versions_without_storage_details() {
        let value = json!({
            "code": 0,
            "data": {
                "items": [{
                    "app_id": "app-1",
                    "version": "v0.1.2",
                    "version_name": "scope isolation",
                    "description": "test version",
                    "sdk_version": "0.1.0-mvp.5",
                    "created_at": "2026-07-27T03:44:35Z",
                    "published_by": "123",
                    "oss_bucket": "private-bucket",
                    "oss_path": "agent-bundles/app-1/v0.1.2.js",
                    "file_size": 3073,
                    "file_hash": "secret-ish-internal-detail"
                }],
                "page": 1,
                "page_size": 20,
                "total": 1
            }
        });

        let output = render_framework_agent_versions(&value, "v0.1.2").unwrap();

        assert!(output.contains("当前测试链路：custom（v0.1.2）"));
        assert!(output.contains("当前"));
        assert!(output.contains("v0.1.2"));
        assert!(output.contains("scope isolation"));
        assert!(output.contains("0.1.0-mvp.5"));
        assert!(output.contains("3.0 KiB"));
        assert!(output.contains("共 1 个版本"));
        assert!(output.contains("chain set custom"));
        assert!(!output.contains("private-bucket"));
        assert!(!output.contains("secret-ish-internal-detail"));
    }

    #[test]
    fn renders_managed_and_custom_framework_agent_versions() {
        let managed = json!({"data": {"version": ""}});
        let custom = json!({"data": {"version": "v0.1.1"}});

        let managed_output = render_framework_agent_version(&managed).unwrap();
        assert!(managed_output.contains("managed"));
        assert!(managed_output.contains("官方最新版本"));

        let custom_output = render_framework_agent_version(&custom).unwrap();
        assert!(custom_output.contains("custom"));
        assert!(custom_output.contains("v0.1.1"));
    }
}
