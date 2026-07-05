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
    let total = field(product, "assignedDeviceQuota");
    let used = field(product, "consumedDeviceQuota");
    let enforce = if bool_field(Some(product), "deviceAuthCheck") {
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
    project_data(value)
        .get("product")?
        .get("deviceAuthCheck")
        .and_then(Value::as_bool)
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
}
