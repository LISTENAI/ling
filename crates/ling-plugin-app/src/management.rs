use anyhow::{anyhow, Context, Result};
use reqwest::{
    multipart::{Form, Part},
    Method, StatusCode, Url,
};
use serde_json::{json, Value};
use std::path::Path;

pub async fn resolve_project_id(
    api_base_url: &str,
    api_key: &str,
    product_id: &str,
) -> Result<String> {
    let mut url = endpoint(api_base_url, &["v1", "projects", "project-id"])?;
    url.query_pairs_mut().append_pair("product_id", product_id);
    let value = send_json(Method::GET, url, api_key, None).await?;
    value
        .get("project_id")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .context("Project ID 解析响应缺少 project_id")
}

pub async fn create_project(
    api_base_url: &str,
    api_key: &str,
    name: &str,
    description: Option<&str>,
) -> Result<Value> {
    let mut body = json!({"name": name});
    if let Some(description) = description {
        body["description"] = Value::String(description.to_owned());
    }
    request(
        api_base_url,
        api_key,
        Method::POST,
        &["v1", "projects"],
        Some(body),
    )
    .await
}

pub async fn get_project(api_base_url: &str, api_key: &str, project_id: &str) -> Result<Value> {
    request(
        api_base_url,
        api_key,
        Method::GET,
        &["v1", "projects", project_id],
        None,
    )
    .await
}

pub async fn update_project(
    api_base_url: &str,
    api_key: &str,
    project_id: &str,
    body: Value,
) -> Result<Value> {
    request(
        api_base_url,
        api_key,
        Method::PUT,
        &["v1", "projects", project_id],
        Some(body),
    )
    .await
}

pub async fn get_framework_agent_version(
    api_base_url: &str,
    api_key: &str,
    app_id: &str,
) -> Result<Value> {
    request(
        api_base_url,
        api_key,
        Method::GET,
        &["v1", "framework", "agents", app_id, "version"],
        None,
    )
    .await
}

pub async fn set_framework_agent_version(
    api_base_url: &str,
    api_key: &str,
    app_id: &str,
    version: Option<&str>,
) -> Result<Value> {
    request(
        api_base_url,
        api_key,
        Method::PUT,
        &["v1", "framework", "agents", app_id, "version"],
        Some(json!({"version": version.unwrap_or_default()})),
    )
    .await
}

pub async fn list_framework_agent_versions(
    api_base_url: &str,
    api_key: &str,
    app_id: &str,
    page: u32,
    page_size: u32,
) -> Result<Value> {
    let url = framework_agent_versions_url(api_base_url, app_id, page, page_size)?;
    send_json(Method::GET, url, api_key, None).await
}

pub async fn require_cli_capability(
    api_base_url: &str,
    api_key: &str,
    capability: &str,
    feature: &str,
) -> Result<()> {
    let url = endpoint(api_base_url, &["v1", "xiaoling", "cli", "capabilities"])?;
    let response = ling_core::client()?
        .get(url)
        .header("authorization", ling_core::bearer(api_key))
        .send()
        .await?;
    if response.status() == StatusCode::NOT_FOUND {
        anyhow::bail!("当前服务端不支持{feature}");
    }
    let value = parse_response(response).await?;
    if !has_cli_capability(&value, capability) {
        anyhow::bail!("当前服务端不支持{feature}");
    }
    Ok(())
}

fn has_cli_capability(value: &Value, capability: &str) -> bool {
    value
        .pointer(&format!("/data/capabilities/{capability}"))
        .and_then(Value::as_u64)
        .is_some_and(|version| version > 0)
}

fn framework_agent_versions_url(
    api_base_url: &str,
    app_id: &str,
    page: u32,
    page_size: u32,
) -> Result<Url> {
    let mut url = endpoint(
        api_base_url,
        &["v1", "framework", "agents", app_id, "versions"],
    )?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("page", &page.to_string());
        query.append_pair("page_size", &page_size.to_string());
    }
    Ok(url)
}

pub async fn list_resource(
    api_base_url: &str,
    api_key: &str,
    project_id: &str,
    segments: &[&str],
    page: u32,
    page_size: u32,
) -> Result<Value> {
    let mut path = vec!["v1", "projects", project_id];
    path.extend_from_slice(segments);
    let mut url = endpoint(api_base_url, &path)?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("page", &page.to_string());
        query.append_pair("pageSize", &page_size.to_string());
    }
    send_json(Method::GET, url, api_key, None).await
}

pub async fn list_all_resource(
    api_base_url: &str,
    api_key: &str,
    project_id: &str,
    segments: &[&str],
) -> Result<Vec<Value>> {
    let mut page = 1;
    let mut items = Vec::new();
    loop {
        let output = list_resource(api_base_url, api_key, project_id, segments, page, 100).await?;
        let batch = output
            .get("data")
            .and_then(Value::as_array)
            .context("分页响应缺少 data 数组")?;
        let batch_len = batch.len();
        items.extend(batch.iter().cloned());
        let total = output
            .get("total")
            .and_then(Value::as_u64)
            .unwrap_or(items.len() as u64);
        if batch_len == 0 || items.len() as u64 >= total {
            break;
        }
        page += 1;
    }
    Ok(items)
}

pub async fn get_resource(
    api_base_url: &str,
    api_key: &str,
    project_id: &str,
    segments: &[&str],
) -> Result<Value> {
    let mut path = vec!["v1", "projects", project_id];
    path.extend_from_slice(segments);
    request(api_base_url, api_key, Method::GET, &path, None).await
}

pub async fn create_resource(
    api_base_url: &str,
    api_key: &str,
    project_id: &str,
    segments: &[&str],
    body: Value,
) -> Result<Value> {
    mutate_resource(
        api_base_url,
        api_key,
        Method::POST,
        project_id,
        segments,
        Some(body),
    )
    .await
}

pub async fn update_resource(
    api_base_url: &str,
    api_key: &str,
    project_id: &str,
    segments: &[&str],
    body: Value,
) -> Result<Value> {
    mutate_resource(
        api_base_url,
        api_key,
        Method::PUT,
        project_id,
        segments,
        Some(body),
    )
    .await
}

pub async fn delete_resource(
    api_base_url: &str,
    api_key: &str,
    project_id: &str,
    segments: &[&str],
) -> Result<Value> {
    mutate_resource(
        api_base_url,
        api_key,
        Method::DELETE,
        project_id,
        segments,
        None,
    )
    .await
}

pub async fn action_resource(
    api_base_url: &str,
    api_key: &str,
    project_id: &str,
    segments: &[&str],
    body: Option<Value>,
) -> Result<Value> {
    mutate_resource(
        api_base_url,
        api_key,
        Method::POST,
        project_id,
        segments,
        body,
    )
    .await
}

pub async fn upload_device_file(
    api_base_url: &str,
    api_key: &str,
    project_id: &str,
    file: &Path,
) -> Result<Value> {
    let bytes =
        std::fs::read(file).with_context(|| format!("读取设备文件失败：{}", file.display()))?;
    if bytes.is_empty() {
        anyhow::bail!("设备文件为空：{}", file.display());
    }
    let name = file
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("devices.txt")
        .to_owned();
    let form = Form::new().part(
        "file",
        Part::bytes(bytes)
            .file_name(name)
            .mime_str("text/plain")
            .context("构建设备文件上传请求失败")?,
    );
    send_multipart(
        api_base_url,
        api_key,
        Method::POST,
        project_id,
        &["devices", "import-by-file"],
        form,
    )
    .await
}

#[derive(Debug, Default)]
pub struct OtaForm<'a> {
    pub file: Option<&'a Path>,
    pub version: Option<&'a str>,
    pub version_number: Option<u64>,
    pub ota_mode: Option<&'a str>,
    pub description: Option<&'a str>,
}

pub async fn upload_ota(
    api_base_url: &str,
    api_key: &str,
    project_id: &str,
    package_id: Option<&str>,
    fields: OtaForm<'_>,
) -> Result<Value> {
    let mut form = Form::new();
    if let Some(file) = fields.file {
        let bytes =
            std::fs::read(file).with_context(|| format!("读取固件失败：{}", file.display()))?;
        if bytes.is_empty() {
            anyhow::bail!("固件文件为空：{}", file.display());
        }
        let name = file
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("firmware.bin")
            .to_owned();
        form = form.part(
            "file",
            Part::bytes(bytes)
                .file_name(name)
                .mime_str("application/octet-stream")
                .context("构建固件上传请求失败")?,
        );
    }
    if let Some(value) = fields.version {
        form = form.text("version", value.to_owned());
    }
    if let Some(value) = fields.version_number {
        form = form.text("version_number", value.to_string());
    }
    if let Some(value) = fields.ota_mode {
        form = form.text("ota_mode", value.to_owned());
    }
    if let Some(value) = fields.description {
        form = form.text("description", value.to_owned());
    }

    let mut segments = vec!["ota", "packages"];
    let method = if let Some(package_id) = package_id {
        segments.push(package_id);
        Method::PUT
    } else {
        Method::POST
    };
    send_multipart(api_base_url, api_key, method, project_id, &segments, form).await
}

async fn mutate_resource(
    api_base_url: &str,
    api_key: &str,
    method: Method,
    project_id: &str,
    segments: &[&str],
    body: Option<Value>,
) -> Result<Value> {
    let mut path = vec!["v1", "projects", project_id];
    path.extend_from_slice(segments);
    request(api_base_url, api_key, method, &path, body).await
}

async fn request(
    api_base_url: &str,
    api_key: &str,
    method: Method,
    segments: &[&str],
    body: Option<Value>,
) -> Result<Value> {
    let url = endpoint(api_base_url, segments)?;
    send_json(method, url, api_key, body).await
}

async fn send_json(method: Method, url: Url, api_key: &str, body: Option<Value>) -> Result<Value> {
    let client = ling_core::client()?;
    let mut request = client
        .request(method, url)
        .header("authorization", ling_core::bearer(api_key));
    if let Some(body) = body {
        request = request.json(&body);
    }
    parse_response(request.send().await?).await
}

async fn send_multipart(
    api_base_url: &str,
    api_key: &str,
    method: Method,
    project_id: &str,
    segments: &[&str],
    form: Form,
) -> Result<Value> {
    let mut path = vec!["v1", "projects", project_id];
    path.extend_from_slice(segments);
    let url = endpoint(api_base_url, &path)?;
    let response = ling_core::client()?
        .request(method, url)
        .header("authorization", ling_core::bearer(api_key))
        .multipart(form)
        .send()
        .await?;
    parse_response(response).await
}

async fn parse_response(response: reqwest::Response) -> Result<Value> {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if status == StatusCode::UNAUTHORIZED {
        anyhow::bail!(
            "API Key 鉴权失败：HTTP 401，请先确认 `ling login` 使用的是 /keys 页面 API Key"
        );
    }
    if !status.is_success() {
        let message = serde_json::from_str::<Value>(&body)
            .ok()
            .and_then(|value| {
                value
                    .get("message")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| body.trim().to_owned());
        anyhow::bail!(
            "应用管理请求失败：HTTP {status}{}",
            if message.is_empty() {
                String::new()
            } else {
                format!("：{message}")
            }
        );
    }
    if body.trim().is_empty() {
        return Ok(json!({"code": "SUCCESS"}));
    }
    serde_json::from_str(&body).context("应用管理响应不是合法 JSON")
}

fn endpoint(api_base_url: &str, segments: &[&str]) -> Result<Url> {
    let mut url = ling_core::http_url(api_base_url, "/")?;
    {
        let mut path = url
            .path_segments_mut()
            .map_err(|_| anyhow!("接口 URL 不支持 path segment 拼接"))?;
        path.clear();
        for segment in segments {
            path.push(segment);
        }
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_encodes_resource_ids_as_single_segments() {
        let url = endpoint(
            "https://api.listenai.com/base",
            &["v1", "projects", "project/id", "roles", "role id"],
        )
        .unwrap();
        assert_eq!(
            url.as_str(),
            "https://api.listenai.com/v1/projects/project%2Fid/roles/role%20id"
        );
    }

    #[test]
    fn framework_versions_url_uses_app_id_and_snake_case_pagination() {
        let url =
            framework_agent_versions_url("https://api.listenai.com/base", "app/id", 2, 50).unwrap();
        assert_eq!(
            url.as_str(),
            "https://api.listenai.com/v1/framework/agents/app%2Fid/versions?page=2&page_size=50"
        );
    }

    #[test]
    fn detects_versioned_cli_capabilities() {
        let value = json!({
            "data": {
                "capabilities": {
                    "project.wakeup-word": 1,
                    "disabled": 0
                }
            }
        });
        assert!(has_cli_capability(&value, "project.wakeup-word"));
        assert!(!has_cli_capability(&value, "disabled"));
        assert!(!has_cli_capability(&value, "missing"));
    }
}
