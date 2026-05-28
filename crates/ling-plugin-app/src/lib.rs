use anyhow::{anyhow, Context, Result};
use reqwest::{Client, StatusCode, Url};
use serde_json::Value;
use unicode_width::UnicodeWidthStr;

pub async fn list_projects(
    api_base_url: &str,
    api_key: &str,
    page: u32,
    page_size: u32,
    service_type: Option<&str>,
) -> Result<Value> {
    let mut url = api_url(api_base_url, "/v1/projects")?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("page", &page.to_string());
        pairs.append_pair("pageSize", &page_size.to_string());
        if let Some(service_type) = service_type {
            pairs.append_pair("service_type", service_type);
        }
    }

    get_json(url, api_key).await
}

pub async fn inspect_project(api_base_url: &str, api_key: &str, project_id: &str) -> Result<Value> {
    let mut url = api_url(api_base_url, "/v1/projects/")?;
    url.path_segments_mut()
        .map_err(|_| anyhow!("接口 URL 不支持 path segment 拼接"))?
        .pop_if_empty()
        .push(project_id);

    get_json(url, api_key).await
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
        "Project ID",
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
                field(project, "id"),
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
            ("产品 ID", field(project, "product_id")),
            ("密钥", product_secret(product)),
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
            ("MCP 服务器", mcp_server_count(config, feature).to_string()),
        ]),
    );

    append_section(&mut output, "能力", render_capabilities(feature));
    output.push_str("\n\nUse --json for the full raw response.");

    Ok(output.trim_end().to_owned())
}

async fn get_json(url: Url, api_key: &str) -> Result<Value> {
    let response = Client::builder()
        .user_agent(concat!("ling-plugin-app/", env!("CARGO_PKG_VERSION")))
        .build()?
        .get(url)
        .header("authorization", bearer(api_key))
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

fn api_url(api_base_url: &str, path: &str) -> Result<Url> {
    let base_url = Url::parse(api_base_url).context("LING_API_BASE_URL 不是合法 URL")?;
    base_url
        .join(path.trim_start_matches('/'))
        .context("接口 URL 拼接失败")
}

fn strip_bearer(api_key: &str) -> String {
    let api_key = api_key.trim();
    if api_key.to_ascii_lowercase().starts_with("bearer ") {
        api_key[7..].trim().to_owned()
    } else {
        api_key.to_owned()
    }
}

fn bearer(api_key: &str) -> String {
    format!("Bearer {}", strip_bearer(api_key))
}

fn field(value: &Value, key: &str) -> String {
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

fn render_table(headers: &[&str], rows: &[Vec<String>]) -> String {
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

fn string_field(value: Option<&Value>, key: &str) -> Option<String> {
    value
        .and_then(|value| value.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn array_len(value: Option<&Value>, key: &str) -> usize {
    value
        .and_then(|value| value.get(key))
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0)
}

fn bool_field(value: Option<&Value>, key: &str) -> bool {
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

fn mcp_server_count(config: Option<&Value>, feature: Option<&Value>) -> usize {
    [
        array_len(config, "mcp_servers"),
        array_len(config, "mcpServers"),
        array_len(feature, "mcp_servers"),
        array_len(feature, "mcpServers"),
    ]
    .into_iter()
    .max()
    .unwrap_or(0)
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

fn enabled_capabilities(feature: Option<&Value>) -> Vec<&'static str> {
    [
        ("long_memory_enable", "长期记忆"),
        ("vpr_enable", "声纹识别"),
        ("search_enable", "联网搜索"),
        ("text2img_enable", "文字生成图片"),
        ("img2text_enable", "图片内容理解"),
    ]
    .into_iter()
    .filter_map(|(key, label)| bool_field(feature, key).then_some(label))
    .collect()
}

fn render_capabilities(feature: Option<&Value>) -> String {
    let capabilities = enabled_capabilities(feature);
    if capabilities.is_empty() {
        "未开启能力".to_owned()
    } else {
        capabilities
            .into_iter()
            .map(|capability| format!("✓ {capability}"))
            .collect::<Vec<_>>()
            .join("  ")
    }
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
    first_non_empty(vec![
        string_field(product, "secret"),
        string_field(product, "previewSecret"),
    ])
    .unwrap_or_else(|| "-".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_project_id_as_single_path_segment() {
        let mut url = api_url("https://api.listenai.com", "/v1/projects/").unwrap();
        url.path_segments_mut()
            .unwrap()
            .pop_if_empty()
            .push("project/id");

        assert_eq!(
            url.as_str(),
            "https://api.listenai.com/v1/projects/project%2Fid"
        );
    }

    #[test]
    fn renders_project_list_as_table() {
        let value = serde_json::json!({
            "data": [{
                "name": "小聆",
                "id": "2d910f43-9133-4f05-a8e2-ef2f7ac86c8e",
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
        assert!(table.contains("Project ID"));
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
        assert!(summary.contains("4bffecaf-3119-4e24-add2-284228c3f845"));
        assert!(!summary.contains("4bffe*******3f845"));
        assert!(summary.contains("小聆老师"));
        assert!(summary.contains("提示语"));
        assert!(summary.contains("9"));
        assert!(summary.contains("主模型"));
        assert!(summary.contains("ls-xiaoling"));
        assert!(summary.contains("图片内容理解"));
        assert!(summary.contains("Use --json for the full raw response."));
    }
}
