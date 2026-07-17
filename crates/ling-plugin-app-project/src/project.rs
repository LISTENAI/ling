//! 本地项目 listenai.toml 的读写。
//!
//! 只维护顶层的 `product_id = "..."` 字段，尽量保留文件中已有内容。

use anyhow::{Context, Result};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub const PROJECT_MANIFEST: &str = "listenai.toml";

pub fn manifest_path(dir: &Path) -> PathBuf {
    dir.join(PROJECT_MANIFEST)
}

/// 从目录下的 listenai.toml 读取 product_id。
pub fn read_product_id(dir: &Path) -> Option<String> {
    let content = fs::read_to_string(manifest_path(dir)).ok()?;
    parse_product_id(&content)
}

/// 把 product_id 写入目录下的 listenai.toml；文件不存在则创建，
/// 已存在则替换顶层 product_id 行或插入到文件头部（任何 [section] 之前）。
pub fn write_product_id(dir: &Path, product_id: &str) -> Result<()> {
    let path = manifest_path(dir);
    let line = format!("product_id = \"{product_id}\"");
    let content = match fs::read_to_string(&path) {
        Ok(existing) => replace_or_insert_product_id(&existing, &line),
        Err(_) => format!("{line}\n"),
    };
    fs::write(&path, content).with_context(|| format!("写入 {} 失败", path.display()))
}

fn parse_product_id(content: &str) -> Option<String> {
    for line in top_level_lines(content) {
        if let Some(value) = parse_assignment(line, "product_id") {
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

/// 迭代 [section] 之前的顶层行。
fn top_level_lines(content: &str) -> impl Iterator<Item = &str> {
    content
        .lines()
        .take_while(|line| !line.trim_start().starts_with('['))
}

fn parse_assignment(line: &str, key: &str) -> Option<String> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix(key)?.trim_start();
    let rest = rest.strip_prefix('=')?.trim();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_owned())
}

fn replace_or_insert_product_id(existing: &str, line: &str) -> String {
    let mut lines: Vec<String> = existing.lines().map(str::to_owned).collect();
    let mut replaced = false;
    for item in &mut lines {
        if item.trim_start().starts_with('[') {
            break;
        }
        if parse_assignment(item, "product_id").is_some() {
            *item = line.to_owned();
            replaced = true;
            break;
        }
    }
    if !replaced {
        lines.insert(0, line.to_owned());
    }
    let mut output = lines.join("\n");
    if !output.ends_with('\n') {
        output.push('\n');
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_top_level_product_id() {
        let content = "# 项目配置\nproduct_id = \"abc-123\"\n\n[build]\nentry = \"agent.ts\"\n";
        assert_eq!(parse_product_id(content).unwrap(), "abc-123");
    }

    #[test]
    fn ignores_product_id_inside_section() {
        let content = "[other]\nproduct_id = \"abc\"\n";
        assert!(parse_product_id(content).is_none());
    }

    #[test]
    fn replaces_existing_line() {
        let content = "name = \"demo\"\nproduct_id = \"old\"\n";
        let updated = replace_or_insert_product_id(content, "product_id = \"new\"");
        assert!(updated.contains("product_id = \"new\""));
        assert!(!updated.contains("old"));
        assert!(updated.contains("name = \"demo\""));
    }

    #[test]
    fn inserts_before_sections() {
        let content = "[build]\nentry = \"agent.ts\"\n";
        let updated = replace_or_insert_product_id(content, "product_id = \"new\"");
        assert!(updated.starts_with("product_id = \"new\"\n[build]"));
    }

    #[test]
    fn writes_and_reads_roundtrip() {
        let dir = std::env::temp_dir().join(format!("ling-project-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        write_product_id(&dir, "prod-1").unwrap();
        assert_eq!(read_product_id(&dir).unwrap(), "prod-1");
        write_product_id(&dir, "prod-2").unwrap();
        assert_eq!(read_product_id(&dir).unwrap(), "prod-2");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
