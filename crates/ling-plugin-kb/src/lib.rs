use anyhow::{anyhow, bail, Context, Result};
use reqwest::{Method, StatusCode, Url};
use serde_json::{json, Value};
use unicode_width::UnicodeWidthStr;

/// 知识库列表（GET /v1/knowledge-bases）。
pub async fn list(api_base_url: &str, api_key: &str, page: u32, size: u32) -> Result<Value> {
    let mut url = ling_core::http_url(api_base_url, "/v1/knowledge-bases")?;
    url.query_pairs_mut()
        .append_pair("page", &page.to_string())
        .append_pair("size", &size.to_string());
    request(Method::GET, url, api_key, None).await
}

/// 创建知识库（POST /v1/knowledge-bases）。
pub async fn create(api_base_url: &str, api_key: &str, index_name: &str) -> Result<Value> {
    let url = ling_core::http_url(api_base_url, "/v1/knowledge-bases")?;
    request(
        Method::POST,
        url,
        api_key,
        Some(json!({"index_name": index_name})),
    )
    .await
}

/// 文档列表（GET /v1/knowledge-bases/{index_id}/documents）。
pub async fn list_documents(
    api_base_url: &str,
    api_key: &str,
    index_id: &str,
    page: u32,
    size: u32,
) -> Result<Value> {
    let mut url = kb_url(api_base_url, index_id, &["documents"])?;
    url.query_pairs_mut()
        .append_pair("page", &page.to_string())
        .append_pair("size", &size.to_string());
    request(Method::GET, url, api_key, None).await
}

/// 添加文档（POST /v1/knowledge-bases/{index_id}/documents）。
pub async fn add_document(
    api_base_url: &str,
    api_key: &str,
    index_id: &str,
    doc_name: &str,
    doc_url: &str,
) -> Result<Value> {
    let url = kb_url(api_base_url, index_id, &["documents"])?;
    request(
        Method::POST,
        url,
        api_key,
        Some(json!({
            "documents": [{"doc_name": doc_name, "doc_url": doc_url}],
            "text_splitter": {"method": "auto"}
        })),
    )
    .await
}

/// 知识库文本检索（GET /v1/knowledge-bases/{index_id}/query）。
pub async fn query(
    api_base_url: &str,
    api_key: &str,
    index_id: &str,
    content: &str,
    limit: Option<u32>,
    threshold: Option<f32>,
) -> Result<Value> {
    let mut url = kb_url(api_base_url, index_id, &["query"])?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("content", content);
        if let Some(limit) = limit {
            pairs.append_pair("limit", &limit.to_string());
        }
        if let Some(threshold) = threshold {
            pairs.append_pair("threshold", &threshold.to_string());
        }
    }
    request(Method::GET, url, api_key, None).await
}

pub fn render_list(value: &Value) -> Result<String> {
    let items = value
        .get("data")
        .and_then(Value::as_array)
        .context("知识库列表响应缺少 data 数组")?;
    if items.is_empty() {
        return Ok("暂无知识库。使用 `ling kb create <名称>` 创建。".to_owned());
    }
    let rows = items
        .iter()
        .map(|item| {
            vec![
                str_field(item, "index_name"),
                str_field(item, "index_id"),
                str_field(item, "doc_count"),
                format_time(&str_field(item, "created_at")),
            ]
        })
        .collect::<Vec<_>>();
    let total = value
        .get("total")
        .map(|total| match total {
            Value::String(text) => text.clone(),
            other => other.to_string(),
        })
        .unwrap_or_else(|| items.len().to_string());
    let mut output = render_table(&["名称", "知识库 ID", "文档数", "创建时间"], &rows);
    output.push_str(&format!(
        "\n共 {total} 个知识库。使用 --json 输出原始 JSON。"
    ));
    Ok(output)
}

pub fn render_documents(value: &Value) -> Result<String> {
    let items = value
        .get("data")
        .and_then(Value::as_array)
        .context("文档列表响应缺少 data 数组")?;
    if items.is_empty() {
        return Ok("该知识库暂无文档。".to_owned());
    }
    let rows = items
        .iter()
        .map(|item| {
            vec![
                str_field(item, "doc_name"),
                str_field(item, "doc_id"),
                doc_status(item),
                str_field(item, "content_length"),
                format_time(&str_field(item, "created_at")),
            ]
        })
        .collect::<Vec<_>>();
    let mut output = render_table(&["文档", "文档 ID", "状态", "长度", "创建时间"], &rows);
    output.push_str(&format!("\n共 {} 个文档。", rows.len()));
    Ok(output)
}

pub fn render_query(value: &Value) -> Result<String> {
    let items = value
        .get("data")
        .and_then(Value::as_array)
        .context("检索响应缺少 data 数组")?;
    if items.is_empty() {
        return Ok("未检索到相关知识点。".to_owned());
    }
    let mut output = String::new();
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            output.push_str("\n\n");
        }
        output.push_str(&format!(
            "{}. [{}] {} (score {})\n{}",
            index + 1,
            str_field(item, "doc_name"),
            str_field(item, "id"),
            str_field(item, "score"),
            item.get("text").and_then(Value::as_str).unwrap_or("-")
        ));
    }
    Ok(output)
}

fn doc_status(item: &Value) -> String {
    match item.get("status").and_then(Value::as_i64) {
        Some(1) => "处理成功".to_owned(),
        Some(3) => "处理中".to_owned(),
        Some(4) => "处理失败".to_owned(),
        Some(other) => other.to_string(),
        None => "-".to_owned(),
    }
}

async fn request(method: Method, url: Url, api_key: &str, body: Option<Value>) -> Result<Value> {
    let mut builder = ling_core::client()?
        .request(method, url)
        .header("authorization", ling_core::bearer(api_key));
    if let Some(body) = body {
        builder = builder.json(&body);
    }
    let response = builder.send().await?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if status == StatusCode::UNAUTHORIZED {
        bail!("API Key 鉴权失败：HTTP 401，请先执行 `ling login`");
    }
    if !status.is_success() {
        bail!("知识库接口请求失败：HTTP {status} {body}");
    }
    if body.trim().is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(&body).context("知识库接口响应不是合法 JSON")
}

fn kb_url(api_base_url: &str, index_id: &str, extra: &[&str]) -> Result<Url> {
    let mut url = ling_core::http_url(api_base_url, "/v1/knowledge-bases/")?;
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| anyhow!("接口 URL 不支持 path segment 拼接"))?;
        segments.pop_if_empty().push(index_id);
        segments.extend(extra);
    }
    Ok(url)
}

fn str_field(value: &Value, key: &str) -> String {
    match value.get(key) {
        Some(Value::String(text)) if !text.is_empty() => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        _ => "-".to_owned(),
    }
}

fn format_time(value: &str) -> String {
    if value.len() >= 16 {
        value[..16].replace('T', " ")
    } else {
        value.to_owned()
    }
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

    #[test]
    fn renders_kb_list() {
        let value = json!({
            "data": [{
                "index_id": "77143e5384f0480cbea3051b9063f6ca",
                "index_name": "测试知识库",
                "doc_count": "3",
                "created_at": "2026-07-03T05:12:24.391Z"
            }],
            "total": "1"
        });
        let out = render_list(&value).unwrap();
        assert!(out.contains("测试知识库"));
        assert!(out.contains("77143e5384f0480cbea3051b9063f6ca"));
        assert!(out.contains("2026-07-03 05:12"));
        assert!(out.contains("共 1 个知识库"));
    }

    #[test]
    fn renders_documents_with_status() {
        let value = json!({
            "data": [{
                "doc_name": "说明书.txt",
                "doc_id": "abc",
                "status": 1,
                "content_length": 9880,
                "created_at": "2023-08-16T06:49:55.820Z"
            }]
        });
        let out = render_documents(&value).unwrap();
        assert!(out.contains("说明书.txt"));
        assert!(out.contains("处理成功"));
    }

    #[test]
    fn kb_url_escapes_index_id() {
        let url = kb_url("https://api.listenai.com", "a/b", &["documents"]).unwrap();
        assert_eq!(
            url.as_str(),
            "https://api.listenai.com/v1/knowledge-bases/a%2Fb/documents"
        );
    }
}
