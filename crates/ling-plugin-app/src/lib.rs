pub mod config_view;
mod device_mcp;
pub mod management;
pub mod records;
pub mod request;

use anyhow::{anyhow, Context, Result};
use reqwest::{StatusCode, Url};
use serde_json::Value;
use unicode_width::UnicodeWidthStr;

pub async fn list_projects(
    api_base_url: &str,
    api_key: &str,
    page: u32,
    page_size: u32,
) -> Result<Value> {
    let url = projects_url(api_base_url, page, page_size)?;
    get_json(url, api_key).await
}

fn projects_url(api_base_url: &str, page: u32, page_size: u32) -> Result<Url> {
    validate_pagination(page, page_size)?;
    let mut url = ling_core::http_url(api_base_url, "/v1/projects")?;
    url.query_pairs_mut()
        .append_pair("page", &page.to_string())
        .append_pair("pageSize", &page_size.to_string())
        .append_pair("service_type", "device");
    Ok(url)
}

fn validate_pagination(page: u32, page_size: u32) -> Result<()> {
    if !(1..=1000).contains(&page) {
        anyhow::bail!("page 必须在 1 到 1000 之间");
    }
    if !(1..=100).contains(&page_size) {
        anyhow::bail!("page-size 必须在 1 到 100 之间");
    }
    Ok(())
}

pub async fn list_all_projects(api_base_url: &str, api_key: &str) -> Result<Vec<Value>> {
    let mut page = 1;
    let mut projects = Vec::new();
    loop {
        let output = list_projects(api_base_url, api_key, page, 100).await?;
        let batch = output
            .get("data")
            .and_then(Value::as_array)
            .context("app list 响应缺少 data 数组")?;
        let batch_len = batch.len();
        projects.extend(batch.iter().cloned());
        let total = output
            .get("total")
            .and_then(Value::as_u64)
            .unwrap_or(projects.len() as u64);
        if batch_len == 0 || projects.len() as u64 >= total {
            break;
        }
        page += 1;
    }
    Ok(projects)
}

pub async fn list_product_projects(
    api_base_url: &str,
    api_key: &str,
    page: u32,
    page_size: u32,
) -> Result<Value> {
    let projects = list_all_projects(api_base_url, api_key).await?;
    product_projects_page(projects, page, page_size)
}

fn product_projects_page(projects: Vec<Value>, page: u32, page_size: u32) -> Result<Value> {
    validate_pagination(page, page_size)?;
    let projects = projects
        .into_iter()
        .filter(has_product_id)
        .collect::<Vec<_>>();
    let total = projects.len();
    let start = ((page - 1) as usize).saturating_mul(page_size as usize);
    let data = projects
        .into_iter()
        .skip(start)
        .take(page_size as usize)
        .collect::<Vec<_>>();
    Ok(serde_json::json!({
        "code": "SUCCESS",
        "message": "请求成功",
        "data": data,
        "page": page,
        "pageSize": page_size,
        "total": total,
    }))
}

pub fn has_product_id(project: &Value) -> bool {
    project_product_id(project).is_some()
}

pub fn project_product_id(project: &Value) -> Option<String> {
    string_field(Some(project), "product_id").or_else(|| {
        project
            .get("product")
            .and_then(|product| string_field(Some(product), "id"))
    })
}

pub fn project_app_id(project: &Value) -> Option<String> {
    string_field(Some(project), "app_id").or_else(|| {
        project
            .get("apps")
            .and_then(Value::as_array)
            .and_then(|apps| apps.first())
            .and_then(|app| string_field(Some(app), "id"))
    })
}

pub fn project_id(project: &Value) -> Option<String> {
    string_field(Some(project), "id")
}

pub async fn inspect_product(api_base_url: &str, api_key: &str, product_id: &str) -> Result<Value> {
    let mut url = ling_core::http_url(api_base_url, "/v1/projects/")?;
    url.path_segments_mut()
        .map_err(|_| anyhow!("接口 URL 不支持 path segment 拼接"))?
        .pop_if_empty()
        .push(product_id);

    get_json(url, api_key).await
}

/// 查询设备是否存在（POST /v1/device/query，云云）。
pub async fn device_query(
    api_base_url: &str,
    api_key: &str,
    product_id: &str,
    device_id: &str,
) -> Result<Value> {
    let url = ling_core::http_url(api_base_url, "/v1/device/query")?;
    let response = ling_core::client()?
        .post(url)
        .header("authorization", ling_core::bearer(api_key))
        .json(&serde_json::json!({
            "product_id": product_id,
            "device_id": device_id,
        }))
        .send()
        .await?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("设备查询失败：HTTP {status} {body}");
    }
    serde_json::from_str(&body).context("设备查询响应不是合法 JSON")
}

pub fn render_project_list(value: &Value) -> Result<String> {
    let projects = value
        .get("data")
        .and_then(Value::as_array)
        .context("app list 响应缺少 data 数组")?;

    if projects.is_empty() {
        return Ok("No apps found.".to_owned());
    }

    let headers = [
        "Name",
        "Product ID",
        "App ID",
        "Type",
        "Deploy",
        "Cost",
        "Status",
        "Created",
    ];
    let rows = projects
        .iter()
        .map(|project| {
            vec![
                field(project, "name"),
                field(project, "product_id"),
                field(project, "app_id"),
                field(project, "service_type"),
                field(project, "deploy_type"),
                field(project, "cost_type"),
                field(project, "status"),
                format_created_at(&field(project, "created_at")),
            ]
        })
        .collect::<Vec<_>>();

    let total = value
        .get("total")
        .and_then(Value::as_u64)
        .unwrap_or(projects.len() as u64);
    let page = value.get("page").and_then(Value::as_u64).unwrap_or(1);
    let page_size = value
        .get("pageSize")
        .and_then(Value::as_u64)
        .unwrap_or(projects.len() as u64);
    let total_pages = if total == 0 || page_size == 0 {
        1
    } else {
        total.div_ceil(page_size)
    };

    let mut output = render_table(&headers, &rows);
    output.push_str(&format!(
        "\nShowing {} of {} apps (page {}/{}; page size {}). Use --json for raw output.",
        projects.len(),
        total,
        page,
        total_pages,
        page_size
    ));
    if page < total_pages {
        output.push_str(&format!("\nNext: ling app list --page {}", page + 1));
    }
    if page > 1 {
        output.push_str(&format!("\nPrev: ling app list --page {}", page - 1));
    }
    Ok(output)
}

pub fn render_project_inspect(value: &Value) -> Result<String> {
    render_project_inspect_with_mcp_count(value, None)
}

pub fn render_project_inspect_with_mcp_count(
    value: &Value,
    resolved_mcp_count: Option<usize>,
) -> Result<String> {
    let project = value.get("data").unwrap_or(value);
    let app = project
        .get("apps")
        .and_then(Value::as_array)
        .and_then(|apps| apps.first());
    let product = project.get("product");
    let config = app.and_then(|app| app.get("config"));
    let feature = config.and_then(|config| config.get("llm_feature"));
    let roles = config
        .and_then(|config| config.get("llm_roles"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);

    let mut output = String::new();
    let title = format!(
        "✦ {}  {} · {} · {}",
        field(project, "name"),
        deploy_title(&field(project, "deploy_type")),
        field(project, "service_type"),
        field(project, "status")
    );
    output.push_str(&title);

    append_section(
        &mut output,
        "概览",
        render_key_values(vec![
            ("项目 ID", field(project, "id")),
            ("应用 ID", option_field(app, "id")),
            (
                "产品 ID",
                string_field(Some(project), "product_id")
                    .or_else(|| product.and_then(|value| string_field(Some(value), "id")))
                    .unwrap_or_else(|| "-".to_owned()),
            ),
            ("产品密钥", product_secret(product)),
            ("接入模式", access_mode(app)),
            ("计费", field(project, "cost_type")),
            ("创建人", field(project, "created_by")),
            ("创建时间", format_created_at(&field(project, "created_at"))),
        ]),
    );

    append_section(
        &mut output,
        "角色",
        if roles.is_empty() {
            "未配置角色".to_owned()
        } else {
            render_table(
                &["角色", "默认", "类型", "音色", "知识库"],
                &roles
                    .iter()
                    .map(|role| {
                        vec![
                            field(role, "name"),
                            if bool_field(Some(role), "is_default") {
                                "是".to_owned()
                            } else {
                                "-".to_owned()
                            },
                            if bool_field(Some(role), "is_builtin") {
                                "内置".to_owned()
                            } else {
                                "自定义".to_owned()
                            },
                            role.get("tts")
                                .and_then(|tts| string_field(Some(tts), "vcn"))
                                .unwrap_or_else(|| "-".to_owned()),
                            array_len(Some(role), "knowledge").to_string(),
                        ]
                    })
                    .collect::<Vec<_>>(),
            )
        },
    );

    append_section(
        &mut output,
        "配置",
        render_key_values(vec![
            ("唤醒词", wake_word(config)),
            ("主模型", main_model(app, feature)),
            ("应用版本", app_version(app, feature)),
            (
                "更新策略",
                string_field(feature, "agent_version_policy").unwrap_or_else(|| "-".to_owned()),
            ),
            ("知识库", array_len(feature, "knowledge").to_string()),
            ("专业词汇", array_len(feature, "hotwords").to_string()),
            ("提示语", array_len(config, "prompt_tone_texts").to_string()),
            (
                "MCP 服务器",
                resolved_mcp_count
                    .or_else(|| mcp_server_count(config, feature))
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| "-".to_owned()),
            ),
        ]),
    );

    output.push_str("\n\nUse --json for the full raw response.");

    Ok(output.trim_end().to_owned())
}

async fn get_json(url: Url, api_key: &str) -> Result<Value> {
    let response = ling_core::client()?
        .get(url)
        .header("authorization", ling_core::bearer(api_key))
        .send()
        .await?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if status == StatusCode::UNAUTHORIZED {
        anyhow::bail!(
            "API Key 鉴权失败：HTTP 401，请先确认 `ling login` 使用的是 /keys 页面 API Key"
        );
    }
    if !status.is_success() {
        anyhow::bail!("app 接口请求失败：HTTP {status} {body}");
    }

    serde_json::from_str(&body).context("app 接口响应不是合法 JSON")
}

pub(crate) fn field(value: &Value, key: &str) -> String {
    match value.get(key) {
        Some(Value::String(text)) => text.to_owned(),
        Some(Value::Number(number)) => number.to_string(),
        Some(Value::Bool(flag)) => flag.to_string(),
        Some(Value::Null) | None => "-".to_owned(),
        Some(other) => other.to_string(),
    }
}

fn format_created_at(value: &str) -> String {
    if value.len() >= 16 {
        value[..16].replace('T', " ")
    } else if value.is_empty() {
        "-".to_owned()
    } else {
        value.to_owned()
    }
}

fn access_mode(app: Option<&Value>) -> String {
    let Some(app) = app else {
        return "-".to_owned();
    };
    match app
        .pointer("/framework_config/app_type")
        .and_then(Value::as_str)
    {
        Some("custom") => "custom（自定义接入）".to_owned(),
        Some("official") | None => "managed（托管接入）".to_owned(),
        Some(value) => format!("unknown（服务端值：{value}）"),
    }
}

pub(crate) fn render_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut widths = headers
        .iter()
        .map(|header| display_width(header))
        .collect::<Vec<_>>();

    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(display_width(cell));
        }
    }

    let mut output = String::new();
    output.push_str(&border("╭", "┬", "╮", &widths));
    output.push('\n');
    output.push_str(&row_line(
        &headers
            .iter()
            .map(|header| header.to_string())
            .collect::<Vec<_>>(),
        &widths,
    ));
    output.push('\n');
    output.push_str(&border("├", "┼", "┤", &widths));
    for row in rows {
        output.push('\n');
        output.push_str(&row_line(row, &widths));
    }
    output.push('\n');
    output.push_str(&border("╰", "┴", "╯", &widths));
    output
}

fn border(left: &str, join: &str, right: &str, widths: &[usize]) -> String {
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
}

fn row_line(cells: &[String], widths: &[usize]) -> String {
    format!(
        "│ {} │",
        cells
            .iter()
            .zip(widths.iter())
            .map(|(cell, width)| format!("{}{}", cell, " ".repeat(width - display_width(cell))))
            .collect::<Vec<_>>()
            .join(" │ ")
    )
}

fn display_width(value: &str) -> usize {
    UnicodeWidthStr::width(value)
}

fn option_field(value: Option<&Value>, key: &str) -> String {
    value
        .map(|value| field(value, key))
        .unwrap_or_else(|| "-".to_owned())
}

pub(crate) fn string_field(value: Option<&Value>, key: &str) -> Option<String> {
    value
        .and_then(|value| value.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

pub(crate) fn array_len(value: Option<&Value>, key: &str) -> usize {
    value
        .and_then(|value| value.get(key))
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0)
}

pub(crate) fn bool_field(value: Option<&Value>, key: &str) -> bool {
    value
        .and_then(|value| value.get(key))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn first_non_empty(values: Vec<Option<String>>) -> Option<String> {
    values.into_iter().flatten().find(|value| !value.is_empty())
}

fn deploy_title(deploy_type: &str) -> &'static str {
    match deploy_type {
        "config" => "配置应用",
        "lowcode" => "业务流编排",
        "hosting" => "官方托管应用",
        "serverless" => "本地上传应用",
        "webhook" => "服务配置应用",
        _ => "应用详情",
    }
}

fn mcp_server_count(config: Option<&Value>, feature: Option<&Value>) -> Option<usize> {
    [
        (config, "mcp_servers"),
        (config, "mcpServers"),
        (feature, "mcp_servers"),
        (feature, "mcpServers"),
    ]
    .into_iter()
    .filter_map(|(value, key)| value?.get(key)?.as_array().map(Vec::len))
    .max()
}

fn main_model(app: Option<&Value>, feature: Option<&Value>) -> String {
    first_non_empty(vec![
        string_field(feature, "main_model"),
        string_field(feature, "main_model_id"),
        string_field(feature, "model"),
        string_field(app, "model"),
    ])
    .unwrap_or_else(|| {
        if option_field(app, "serverless_type") == "4" {
            "ls-xiaoling".to_owned()
        } else {
            "-".to_owned()
        }
    })
}

fn app_version(app: Option<&Value>, feature: Option<&Value>) -> String {
    first_non_empty(vec![
        string_field(feature, "agent_version"),
        string_field(app, "build_version"),
        string_field(app, "image_version"),
        string_field(feature, "agent_version_policy"),
    ])
    .unwrap_or_else(|| "-".to_owned())
}

fn render_key_values(rows: Vec<(&str, String)>) -> String {
    render_table(
        &["字段", "值"],
        &rows
            .into_iter()
            .map(|(key, value)| vec![key.to_owned(), value])
            .collect::<Vec<_>>(),
    )
}

fn append_section(output: &mut String, title: &str, content: String) {
    output.push_str("\n\n");
    output.push_str("▸ ");
    output.push_str(title);
    output.push('\n');
    output.push_str(&content);
}

fn wake_word(config: Option<&Value>) -> String {
    let Some(wakeup_word) = config.and_then(|config| config.get("default_wakeup_word")) else {
        return "-".to_owned();
    };

    let name = field(wakeup_word, "name");
    let sensitivity = field(wakeup_word, "sensitivity");
    if sensitivity == "-" {
        name
    } else {
        format!("{name} ({sensitivity})")
    }
}

fn product_secret(product: Option<&Value>) -> String {
    string_field(product, "secret")
        .or_else(|| string_field(product, "previewSecret"))
        .or_else(|| string_field(product, "preview_secret"))
        .unwrap_or_else(|| "-".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_product_id_as_single_path_segment() {
        let mut url = ling_core::http_url("https://api.listenai.com", "/v1/projects/").unwrap();
        url.path_segments_mut()
            .unwrap()
            .pop_if_empty()
            .push("product/id");

        assert_eq!(
            url.as_str(),
            "https://api.listenai.com/v1/projects/product%2Fid"
        );
    }

    #[test]
    fn renders_project_list_as_table() {
        let value = serde_json::json!({
            "data": [{
                "name": "小聆",
                "id": "2d910f43-9133-4f05-a8e2-ef2f7ac86c8e",
                "product_id": "adf675fb-2e92-4c5b-b367-74710f048b2a",
                "app_id": "b8c2f846",
                "service_type": "device",
                "deploy_type": "config",
                "cost_type": "token",
                "status": "available",
                "created_at": "2026-04-02T11:05:57.000Z"
            }],
            "page": 1,
            "pageSize": 20,
            "total": 1
        });

        let table = render_project_list(&value).unwrap();
        assert!(table.contains("Product ID"));
        assert!(!table.contains("Project ID"));
        assert!(table.contains("adf675fb-2e92-4c5b-b367-74710f048b2a"));
        assert!(!table.contains("2d910f43-9133-4f05-a8e2-ef2f7ac86c8e"));
        assert!(table.contains("小聆"));
        assert!(table.contains("2026-04-02 11:05"));
        assert!(table.contains("page 1/1"));
        assert!(table.contains("Use --json for raw output."));
    }

    #[test]
    fn renders_project_inspect_as_summary() {
        let prompt_tones = (0..9).map(|_| serde_json::json!({})).collect::<Vec<_>>();
        let value = serde_json::json!({
            "data": {
                "id": "5a53b748-c4e7-4cfc-96b2-450cbc939c35",
                "name": "0526小聆测试",
                "deploy_type": "config",
                "product_id": "adf675fb-2e92-4c5b-b367-74710f048b2a",
                "status": "available",
                "product": {
                    "secret": "4bffecaf-3119-4e24-add2-284228c3f845",
                    "previewSecret": "4bffe*******3f845"
                },
                "apps": [{
                    "id": "da3062bf",
                    "serverless_type": 4,
                    "framework_config": {"app_type": "custom"},
                    "config": {
                        "llm_roles": [
                            {"name": "小聆老师", "is_default": true},
                            {"name": "管家大叔", "is_default": false}
                        ],
                        "llm_feature": {
                            "agent_version": "2.0.0",
                            "knowledge": [],
                            "hotwords": [],
                            "long_memory_enable": true,
                            "vpr_enable": true,
                            "search_enable": true,
                            "text2img_enable": true,
                            "img2text_enable": true
                        },
                        "prompt_tone_texts": prompt_tones
                    }
                }]
            }
        });

        let summary = render_project_inspect(&value).unwrap();
        assert!(summary.contains("配置应用"));
        assert!(summary.contains("▸ 概览"));
        assert!(summary.contains("产品密钥"));
        assert!(summary.contains("4bffecaf-3119-4e24-add2-284228c3f845"));
        assert!(!summary.contains("4bffe*******3f845"));
        assert!(summary.contains("custom（自定义接入）"));
        assert!(summary.contains("小聆老师"));
        assert!(summary.contains("提示语"));
        assert!(summary.contains("9"));
        assert!(summary.contains("主模型"));
        assert!(summary.contains("ls-xiaoling"));
        assert!(!summary.contains("▸ 能力"));
        assert!(!summary.contains("图片内容理解"));
        assert!(summary.contains("Use --json for the full raw response."));
    }

    #[test]
    fn inspect_uses_resolved_mcp_count_and_does_not_invent_zero() {
        let value = serde_json::json!({
            "data": {
                "name": "测试应用",
                "apps": [{"config": {"llm_feature": {}}}]
            }
        });

        let unresolved = render_project_inspect(&value).unwrap();
        assert!(unresolved.contains("│ MCP 服务器 │ -"));

        let resolved = render_project_inspect_with_mcp_count(&value, Some(9)).unwrap();
        assert!(resolved.contains("│ MCP 服务器 │ 9"));
    }

    #[test]
    fn inspect_treats_missing_server_app_type_as_managed() {
        let value = serde_json::json!({
            "data": {
                "name": "托管应用",
                "apps": [{"config": {"llm_feature": {}}}]
            }
        });

        let rendered = render_project_inspect(&value).unwrap();
        assert!(rendered.contains("managed（托管接入）"));
    }

    #[test]
    fn inspect_secret_prefers_full_value_and_falls_back_to_preview() {
        let full = serde_json::json!({
            "secret": "full-product-secret",
            "previewSecret": "full-*******-secret"
        });
        assert_eq!(product_secret(Some(&full)), "full-product-secret");

        let preview = serde_json::json!({"preview_secret": "preview-*******-secret"});
        assert_eq!(product_secret(Some(&preview)), "preview-*******-secret");
    }

    #[test]
    fn product_project_page_filters_before_paginating() {
        let projects = vec![
            serde_json::json!({"id": "project-api", "product_id": ""}),
            serde_json::json!({"id": "project-1", "product_id": "product-1"}),
            serde_json::json!({"id": "project-api-2"}),
            serde_json::json!({"id": "project-2", "product": {"id": "product-2"}}),
        ];
        let page = product_projects_page(projects, 1, 1).unwrap();
        assert_eq!(page["total"], 2);
        assert_eq!(page["data"][0]["id"], "project-1");

        let page = product_projects_page(
            vec![
                serde_json::json!({"id": "project-api", "product_id": ""}),
                serde_json::json!({"id": "project-1", "product_id": "product-1"}),
                serde_json::json!({"id": "project-2", "product": {"id": "product-2"}}),
            ],
            2,
            1,
        )
        .unwrap();
        assert_eq!(page["data"][0]["id"], "project-2");
    }

    #[test]
    fn project_list_url_always_targets_device_apps() {
        let url = projects_url("https://api.listenai.com/base", 2, 50).unwrap();
        assert_eq!(
            url.as_str(),
            "https://api.listenai.com/base/v1/projects?page=2&pageSize=50&service_type=device"
        );
    }

    #[test]
    fn project_list_rejects_out_of_range_pagination() {
        for page in [0, 1001, u32::MAX] {
            assert!(projects_url("https://api.listenai.com", page, 20).is_err());
            assert!(product_projects_page(Vec::new(), page, 20).is_err());
        }
        for page_size in [0, 101, u32::MAX] {
            assert!(projects_url("https://api.listenai.com", 1, page_size).is_err());
            assert!(product_projects_page(Vec::new(), 1, page_size).is_err());
        }
    }

    #[test]
    fn reads_all_supported_project_identifiers() {
        let project = serde_json::json!({
            "id": "project-1",
            "app_id": "app-1",
            "product_id": "product-1"
        });
        assert_eq!(project_id(&project).as_deref(), Some("project-1"));
        assert_eq!(project_app_id(&project).as_deref(), Some("app-1"));
        assert_eq!(project_product_id(&project).as_deref(), Some("product-1"));
    }
}
