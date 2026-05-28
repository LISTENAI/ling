use anyhow::{Context, Result};
use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, CONTROLS};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Serialize)]
pub struct WikiSearchOutput {
    pub title: String,
    pub url: String,
}

#[derive(Debug, Serialize)]
pub struct WikiSearchGroup {
    pub keyword: String,
    pub results: Vec<WikiSearchOutput>,
}

const MAX_RENDERED_SEARCH_RESULTS: usize = 20;
const MAX_RENDERED_GROUP_RESULTS: usize = 5;

const PATH_SEGMENT_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}')
    .add(b'[')
    .add(b']')
    .add(b'\\')
    .add(b'^')
    .add(b'|');

#[derive(Debug)]
struct MergedResult {
    output: WikiSearchOutput,
    matched_keywords: HashSet<String>,
    first_seen: usize,
}

#[derive(Debug, Deserialize)]
struct GraphqlEnvelope {
    data: Option<GraphqlData>,
    errors: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct GraphqlData {
    pages: Pages,
}

#[derive(Debug, Deserialize)]
struct Pages {
    search: PageSearch,
}

#[derive(Debug, Deserialize)]
struct PageSearch {
    results: Vec<PageSearchResult>,
}

#[derive(Debug, Deserialize)]
struct PageSearchResult {
    id: String,
    title: String,
    path: String,
    locale: String,
}

pub async fn search(
    graphql_url: &str,
    docs_base_url: &str,
    keywords: &[String],
) -> Result<Vec<WikiSearchOutput>> {
    if keywords.is_empty() {
        anyhow::bail!("请至少提供一个关键词，例如：ling wiki search 标准API 获取密钥");
    }

    let client = Client::builder()
        .user_agent(concat!("ling/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let mut merged: HashMap<String, MergedResult> = HashMap::new();
    let mut order = 0usize;

    for keyword in keywords {
        let keyword = keyword.trim();
        if keyword.is_empty() {
            continue;
        }
        let results = search_one(&client, graphql_url, keyword).await?;
        for result in results {
            let key = if result.path.is_empty() {
                result.id.clone()
            } else {
                format!("{}:{}", result.locale, result.path)
            };
            let entry = merged.entry(key).or_insert_with(|| {
                let output = search_output(docs_base_url, &result);
                let current = MergedResult {
                    output,
                    matched_keywords: HashSet::new(),
                    first_seen: order,
                };
                order += 1;
                current
            });
            entry.matched_keywords.insert(keyword.to_string());
        }
    }

    let mut values = merged.into_values().collect::<Vec<_>>();
    values.sort_by(|a, b| {
        b.matched_keywords
            .len()
            .cmp(&a.matched_keywords.len())
            .then_with(|| a.first_seen.cmp(&b.first_seen))
    });

    Ok(values.into_iter().map(|item| item.output).collect())
}

pub async fn search_grouped(
    graphql_url: &str,
    docs_base_url: &str,
    keywords: &[String],
) -> Result<Vec<WikiSearchGroup>> {
    if keywords.is_empty() {
        anyhow::bail!("请至少提供一个关键词，例如：ling wiki search 标准API 获取密钥");
    }

    let client = Client::builder()
        .user_agent(concat!("ling/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let mut groups = Vec::new();

    for keyword in keywords {
        let keyword = keyword.trim();
        if keyword.is_empty() {
            continue;
        }

        let results = search_one(&client, graphql_url, keyword)
            .await?
            .into_iter()
            .map(|result| search_output(docs_base_url, &result))
            .collect();
        groups.push(WikiSearchGroup {
            keyword: keyword.to_owned(),
            results,
        });
    }

    Ok(groups)
}

pub fn render_search_results(results: &[WikiSearchOutput]) -> String {
    if results.is_empty() {
        return "未找到相关文档。使用 --json 输出 JSON。".to_owned();
    }

    let display_count = results.len().min(MAX_RENDERED_SEARCH_RESULTS);
    let mut output = if results.len() > MAX_RENDERED_SEARCH_RESULTS {
        format!(
            "找到 {} 条文档，展示前 {} 条：",
            results.len(),
            display_count
        )
    } else {
        format!("找到 {} 条文档：", results.len())
    };
    for (index, result) in results.iter().take(display_count).enumerate() {
        output.push_str(&format!(
            "\n{}. {}\n   {}",
            index + 1,
            result.title,
            decode_url_for_display(&result.url)
        ));
    }
    if results.len() > MAX_RENDERED_SEARCH_RESULTS {
        output.push_str("\n\n使用 --json 输出全部 JSON。");
    } else {
        output.push_str("\n\n使用 --json 输出 JSON。");
    }
    output
}

pub fn render_search_groups(groups: &[WikiSearchGroup]) -> String {
    if groups.is_empty() || groups.iter().all(|group| group.results.is_empty()) {
        return "未找到相关文档。使用 --json 输出 JSON。".to_owned();
    }

    let mut output = format!(
        "按 {} 个搜索词展示相关文档，每个最多展示 {} 条：",
        groups.len(),
        MAX_RENDERED_GROUP_RESULTS
    );
    for group in groups {
        let display_count = group.results.len().min(MAX_RENDERED_GROUP_RESULTS);
        let section_title = if group.results.len() > MAX_RENDERED_GROUP_RESULTS {
            format!(
                "\n\n=== {}（{} 条，展示前 {} 条） ===",
                group.keyword,
                group.results.len(),
                display_count
            )
        } else {
            format!(
                "\n\n=== {}（{} 条） ===",
                group.keyword,
                group.results.len()
            )
        };
        output.push_str(&section_title);

        if group.results.is_empty() {
            output.push_str("\n未找到相关文档。");
            continue;
        }

        for (index, result) in group.results.iter().take(display_count).enumerate() {
            output.push_str(&format!(
                "\n{}. {}\n   {}",
                index + 1,
                result.title,
                decode_url_for_display(&result.url)
            ));
        }
    }
    output.push_str("\n\n使用 --json 输出合并去重后的完整 JSON。");
    output
}

async fn search_one(
    client: &Client,
    graphql_url: &str,
    keyword: &str,
) -> Result<Vec<PageSearchResult>> {
    let query = "query ($query: String!) { pages { search(query: $query) { results { id title description path locale content } suggestions totalHits } } }";
    let body = serde_json::json!([{
        "operationName": serde_json::Value::Null,
        "variables": { "query": keyword },
        "extensions": {},
        "query": query
    }]);

    let response = client
        .post(graphql_url)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("docs2 GraphQL 搜索失败：HTTP {status} {text}");
    }

    let mut envelopes: Vec<GraphqlEnvelope> =
        serde_json::from_str(&text).context("docs2 GraphQL 响应解析失败")?;
    let envelope = envelopes.pop().context("docs2 GraphQL 响应为空")?;
    if let Some(errors) = envelope.errors {
        anyhow::bail!("docs2 GraphQL 返回错误：{errors}");
    }
    Ok(envelope
        .data
        .context("docs2 GraphQL 响应缺少 data")?
        .pages
        .search
        .results)
}

fn docs_url(base_url: &str, locale: &str, path: &str) -> String {
    let base_url = base_url.trim_end_matches('/');
    let locale = encode_segment(locale);
    let path = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(encode_segment)
        .collect::<Vec<_>>()
        .join("/");
    format!("{base_url}/{locale}/{path}")
}

fn encode_segment(segment: &str) -> String {
    utf8_percent_encode(segment, PATH_SEGMENT_ENCODE_SET).to_string()
}

fn search_output(docs_base_url: &str, result: &PageSearchResult) -> WikiSearchOutput {
    WikiSearchOutput {
        title: strip_html_tags(&result.title),
        url: docs_url(docs_base_url, &result.locale, &result.path),
    }
}

fn decode_url_for_display(url: &str) -> String {
    percent_decode_str(url).decode_utf8_lossy().into_owned()
}

fn strip_html_tags(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => output.push(ch),
            _ => {}
        }
    }
    output
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_em_tags() {
        assert_eq!(strip_html_tags("<em>标准</em>API"), "标准API");
    }

    #[test]
    fn builds_encoded_docs_url() {
        assert_eq!(
            docs_url("https://docs2.listenai.com/", "zh", "大模型开发/API接口/标准API"),
            "https://docs2.listenai.com/zh/%E5%A4%A7%E6%A8%A1%E5%9E%8B%E5%BC%80%E5%8F%91/API%E6%8E%A5%E5%8F%A3/%E6%A0%87%E5%87%86API"
        );
        assert_eq!(
            docs_url("https://docs2.listenai.com", "zh", "自定义MCP应用-基础教程"),
            "https://docs2.listenai.com/zh/%E8%87%AA%E5%AE%9A%E4%B9%89MCP%E5%BA%94%E7%94%A8-%E5%9F%BA%E7%A1%80%E6%95%99%E7%A8%8B"
        );
    }

    #[test]
    fn renders_search_results_with_decoded_url() {
        let results = vec![WikiSearchOutput {
            title: "接入局域网本地大模型".to_owned(),
            url: docs_url(
                "https://docs2.listenai.com",
                "zh",
                "云端开发/LSPlatform/编排应用介绍/接入第三方大模型/接入局域网本地大模型",
            ),
        }];

        let output = render_search_results(&results);
        assert!(output.contains("找到 1 条文档"));
        assert!(output.contains("https://docs2.listenai.com/zh/云端开发/LSPlatform/编排应用介绍/接入第三方大模型/接入局域网本地大模型"));
        assert!(!output.trim_start().starts_with('['));
        assert!(output.contains("使用 --json 输出 JSON。"));
    }

    #[test]
    fn renders_at_most_twenty_search_results() {
        let results = (1..=21)
            .map(|index| WikiSearchOutput {
                title: format!("文档{index}"),
                url: format!("https://docs2.listenai.com/zh/%E6%96%87%E6%A1%A3{index}"),
            })
            .collect::<Vec<_>>();

        let output = render_search_results(&results);
        assert!(output.contains("找到 21 条文档，展示前 20 条："));
        assert!(output.contains("20. 文档20"));
        assert!(!output.contains("21. 文档21"));
        assert!(output.contains("使用 --json 输出全部 JSON。"));
    }

    #[test]
    fn renders_grouped_search_results_with_five_items_per_keyword() {
        let groups = vec![
            WikiSearchGroup {
                keyword: "第三方".to_owned(),
                results: (1..=6)
                    .map(|index| WikiSearchOutput {
                        title: format!("第三方文档{index}"),
                        url: format!(
                            "https://docs2.listenai.com/zh/%E7%AC%AC%E4%B8%89%E6%96%B9{index}"
                        ),
                    })
                    .collect(),
            },
            WikiSearchGroup {
                keyword: "CSK".to_owned(),
                results: vec![WikiSearchOutput {
                    title: "CSK文档".to_owned(),
                    url: "https://docs2.listenai.com/zh/CSK".to_owned(),
                }],
            },
        ];

        let output = render_search_groups(&groups);
        assert!(output.contains("按 2 个搜索词展示相关文档，每个最多展示 5 条："));
        assert!(output.contains("=== 第三方（6 条，展示前 5 条） ==="));
        assert!(output.contains("5. 第三方文档5"));
        assert!(!output.contains("6. 第三方文档6"));
        assert!(output.contains("https://docs2.listenai.com/zh/第三方1"));
        assert!(output.contains("=== CSK（1 条） ==="));
        assert!(output.contains("1. CSK文档"));
    }
}
