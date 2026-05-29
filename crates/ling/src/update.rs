use anyhow::{anyhow, bail, ensure, Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, USER_AGENT};
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const BIN: &str = "ling";
const DEFAULT_REPO: &str = "LISTENAI/ling";
const GITHUB_API_VERSION: &str = "2022-11-28";

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Deserialize)]
struct ReleaseAsset {
    name: String,
    url: String,
    #[serde(default)]
    browser_download_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReleaseTarget {
    triple: String,
    archive_ext: &'static str,
    bin_ext: &'static str,
}

pub async fn run() -> Result<()> {
    let repo = env::var("LING_REPO").unwrap_or_else(|_| DEFAULT_REPO.to_owned());
    let token = github_token();
    let client = github_client()?;
    let release = fetch_latest_release(&client, token.as_deref(), &repo).await?;
    let current_version = parse_version(env!("CARGO_PKG_VERSION"))?;
    let latest_version = parse_release_version(&release.tag_name)?;

    if !update_needed(&current_version, &latest_version) {
        println!("ling 已是最新版本 ({})", release.tag_name);
        return Ok(());
    }

    let target = detect_target()?;
    let asset_name = release_asset_name(&release.tag_name, &target);
    let asset = release
        .asset(&asset_name)
        .ok_or_else(|| anyhow!("release asset not found: {asset_name}"))?;
    let checksum_asset = release
        .asset("SHA256SUMS")
        .ok_or_else(|| anyhow!("release asset not found: SHA256SUMS"))?;

    println!(
        "Updating ling {} -> {} ({})",
        env!("CARGO_PKG_VERSION"),
        release.tag_name,
        target.triple
    );
    println!("Downloading {asset_name}");

    let archive = download_asset(&client, token.as_deref(), asset).await?;
    let checksums = download_asset(&client, token.as_deref(), checksum_asset).await?;
    verify_archive_checksum(&archive, &checksums, &asset_name)?;
    println!("Checksum verified");

    let temp_dir = tempfile::tempdir().context("failed to create temporary directory")?;
    let new_binary = extract_archive(&archive, &target, &release.tag_name, temp_dir.path())?;
    make_executable(&new_binary)?;
    verify_binary(&new_binary)?;

    let current_exe = replace_current_exe(&new_binary)?;
    println!(
        "Updated ling to {} at {}",
        release.tag_name,
        current_exe.display()
    );
    Ok(())
}

impl Release {
    fn asset(&self, name: &str) -> Option<&ReleaseAsset> {
        self.assets.iter().find(|asset| asset.name == name)
    }
}

fn github_client() -> Result<reqwest::Client> {
    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_str(&format!("ling/{}", env!("CARGO_PKG_VERSION")))?,
    );
    headers.insert(
        "X-GitHub-Api-Version",
        HeaderValue::from_static(GITHUB_API_VERSION),
    );

    reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .context("failed to create HTTP client")
}

fn github_token() -> Option<String> {
    env::var("GH_TOKEN")
        .ok()
        .filter(|token| !token.trim().is_empty())
        .or_else(|| {
            env::var("GITHUB_TOKEN")
                .ok()
                .filter(|token| !token.trim().is_empty())
        })
}

async fn fetch_latest_release(
    client: &reqwest::Client,
    token: Option<&str>,
    repo: &str,
) -> Result<Release> {
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let mut request = client
        .get(url)
        .header(ACCEPT, "application/vnd.github+json");
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }

    let response = request
        .send()
        .await
        .with_context(|| format!("failed to fetch latest release for {repo}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read latest release response")?;
    ensure!(
        status.is_success(),
        "failed to fetch latest release for {repo}: {}",
        http_error(status, &body)
    );
    serde_json::from_str(&body).context("failed to parse latest release response")
}

async fn download_asset(
    client: &reqwest::Client,
    token: Option<&str>,
    asset: &ReleaseAsset,
) -> Result<Vec<u8>> {
    let url = if token.is_some() {
        &asset.url
    } else if !asset.browser_download_url.is_empty() {
        &asset.browser_download_url
    } else {
        &asset.url
    };

    let mut request = client.get(url);
    if let Some(token) = token {
        request = request
            .bearer_auth(token)
            .header(ACCEPT, "application/octet-stream");
    }

    let response = request
        .send()
        .await
        .with_context(|| format!("failed to download release asset {}", asset.name))?;
    let status = response.status();
    let bytes = response
        .bytes()
        .await
        .with_context(|| format!("failed to read release asset {}", asset.name))?;
    ensure!(
        status.is_success(),
        "failed to download release asset {}: {}",
        asset.name,
        http_error(status, bytes_as_lossy_text(&bytes).as_ref())
    );
    Ok(bytes.to_vec())
}

fn http_error(status: reqwest::StatusCode, body: &str) -> String {
    let message = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("message")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| compact_body(body));
    if message.is_empty() {
        format!("HTTP {status}")
    } else {
        format!("HTTP {status}: {message}")
    }
}

fn compact_body(body: &str) -> String {
    const MAX_LEN: usize = 300;
    let compact = body.split_whitespace().collect::<Vec<_>>().join(" ");
    let truncated = compact.chars().take(MAX_LEN).collect::<String>();
    if truncated.len() < compact.len() {
        format!("{truncated}...")
    } else {
        compact
    }
}

fn bytes_as_lossy_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn detect_target() -> Result<ReleaseTarget> {
    let libc = env::var("LING_LIBC").ok();
    target_for(env::consts::OS, env::consts::ARCH, libc.as_deref())
}

fn target_for(os: &str, arch: &str, libc: Option<&str>) -> Result<ReleaseTarget> {
    let arch = normalize_arch(arch)?;
    match os {
        "macos" => Ok(ReleaseTarget {
            triple: format!("{arch}-apple-darwin"),
            archive_ext: "tar.gz",
            bin_ext: "",
        }),
        "linux" => {
            let libc = libc.unwrap_or("musl");
            ensure!(
                matches!(libc, "musl" | "gnu"),
                "unsupported LING_LIBC={libc}; expected musl or gnu"
            );
            Ok(ReleaseTarget {
                triple: format!("{arch}-unknown-linux-{libc}"),
                archive_ext: "tar.gz",
                bin_ext: "",
            })
        }
        "windows" => Ok(ReleaseTarget {
            triple: format!("{arch}-pc-windows-msvc"),
            archive_ext: "zip",
            bin_ext: ".exe",
        }),
        other => bail!("unsupported OS: {other}"),
    }
}

fn normalize_arch(arch: &str) -> Result<&'static str> {
    match arch {
        "x86_64" | "amd64" | "x64" => Ok("x86_64"),
        "aarch64" | "arm64" => Ok("aarch64"),
        other => bail!("unsupported CPU architecture: {other}"),
    }
}

fn release_asset_name(tag: &str, target: &ReleaseTarget) -> String {
    format!("{BIN}-{tag}-{}.{}", target.triple, target.archive_ext)
}

fn parse_version(version: &str) -> Result<Version> {
    Version::parse(version).with_context(|| format!("invalid version: {version}"))
}

fn parse_release_version(tag: &str) -> Result<Version> {
    parse_version(tag.trim_start_matches('v'))
}

fn update_needed(current: &Version, latest: &Version) -> bool {
    latest > current
}

fn expected_checksum(checksums: &[u8], asset_name: &str) -> Result<String> {
    let checksums = std::str::from_utf8(checksums).context("SHA256SUMS is not valid UTF-8 text")?;
    for line in checksums.lines() {
        let mut parts = line.split_whitespace();
        let Some(hash) = parts.next() else {
            continue;
        };
        let Some(name) = parts.next() else {
            continue;
        };
        if name.trim_start_matches('*') == asset_name {
            let hash = hash.to_ascii_lowercase();
            ensure!(
                hash.len() == 64 && hash.chars().all(|ch| ch.is_ascii_hexdigit()),
                "invalid SHA256 checksum for {asset_name}"
            );
            return Ok(hash);
        }
    }
    bail!("checksum for {asset_name} not found in SHA256SUMS")
}

fn verify_archive_checksum(archive: &[u8], checksums: &[u8], asset_name: &str) -> Result<()> {
    let expected = expected_checksum(checksums, asset_name)?;
    let actual = sha256_hex(archive);
    ensure!(
        expected == actual,
        "checksum mismatch for {asset_name}: expected {expected}, got {actual}"
    );
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn extract_archive(
    archive: &[u8],
    target: &ReleaseTarget,
    tag: &str,
    temp_dir: &Path,
) -> Result<PathBuf> {
    match target.archive_ext {
        "tar.gz" => extract_tar_gz(archive, temp_dir)?,
        "zip" => extract_zip(archive, temp_dir)?,
        other => bail!("unsupported archive type: {other}"),
    }

    let binary = temp_dir
        .join(format!("{BIN}-{tag}-{}", target.triple))
        .join(format!("{BIN}{}", target.bin_ext));
    ensure!(
        binary.is_file(),
        "binary not found in archive: {}",
        binary.display()
    );
    Ok(binary)
}

fn extract_tar_gz(archive: &[u8], temp_dir: &Path) -> Result<()> {
    let decoder = flate2::read::GzDecoder::new(Cursor::new(archive));
    let mut archive = tar::Archive::new(decoder);
    archive
        .unpack(temp_dir)
        .context("failed to extract tar.gz release asset")
}

#[cfg(windows)]
fn extract_zip(archive: &[u8], temp_dir: &Path) -> Result<()> {
    let reader = Cursor::new(archive);
    let mut archive = zip::ZipArchive::new(reader).context("failed to open zip release asset")?;
    archive
        .extract(temp_dir)
        .context("failed to extract zip release asset")
}

#[cfg(not(windows))]
fn extract_zip(_archive: &[u8], _temp_dir: &Path) -> Result<()> {
    bail!("zip release assets are only supported on Windows")
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)
        .with_context(|| format!("failed to read permissions for {}", path.display()))?
        .permissions();
    permissions.set_mode(permissions.mode() | 0o755);
    fs::set_permissions(path, permissions)
        .with_context(|| format!("failed to mark {} executable", path.display()))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}

fn verify_binary(path: &Path) -> Result<()> {
    let status = Command::new(path)
        .arg("--help")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| format!("failed to run downloaded binary {}", path.display()))?;
    ensure!(
        status.success(),
        "downloaded binary failed to run: {}",
        path.display()
    );
    Ok(())
}

#[cfg(not(windows))]
fn replace_current_exe(new_binary: &Path) -> Result<PathBuf> {
    let current_exe = env::current_exe().context("failed to locate current executable")?;
    let parent = current_exe
        .parent()
        .ok_or_else(|| anyhow!("current executable has no parent directory"))?;
    let staging = parent.join(format!(".{BIN}-update-{}", std::process::id()));

    let result = (|| {
        fs::copy(new_binary, &staging).with_context(|| {
            format!(
                "failed to stage update from {} to {}",
                new_binary.display(),
                staging.display()
            )
        })?;
        let permissions = fs::metadata(new_binary)
            .with_context(|| format!("failed to read permissions for {}", new_binary.display()))?
            .permissions();
        fs::set_permissions(&staging, permissions)
            .with_context(|| format!("failed to set permissions for {}", staging.display()))?;
        fs::rename(&staging, &current_exe).with_context(|| {
            format!(
                "failed to replace current executable {}",
                current_exe.display()
            )
        })?;
        Ok(current_exe)
    })();

    if result.is_err() {
        let _ = fs::remove_file(&staging);
    }
    result
}

#[cfg(windows)]
fn replace_current_exe(new_binary: &Path) -> Result<PathBuf> {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const DETACHED_PROCESS: u32 = 0x0000_0008;

    let current_exe = env::current_exe().context("failed to locate current executable")?;
    let parent = current_exe
        .parent()
        .ok_or_else(|| anyhow!("current executable has no parent directory"))?;
    let pid = std::process::id();
    let staged = parent.join(format!(".{BIN}-update-{pid}.exe"));
    let helper = parent.join(format!(".{BIN}-update-{pid}.cmd"));

    fs::copy(new_binary, &staged).with_context(|| {
        format!(
            "failed to stage update from {} to {}",
            new_binary.display(),
            staged.display()
        )
    })?;
    fs::write(&helper, windows_helper_script())
        .with_context(|| format!("failed to write update helper {}", helper.display()))?;

    Command::new("cmd")
        .arg("/C")
        .arg(&helper)
        .arg(pid.to_string())
        .arg(&staged)
        .arg(&current_exe)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
        .spawn()
        .with_context(|| format!("failed to start update helper {}", helper.display()))?;

    println!("Windows 已安排在当前进程退出后完成替换。");
    Ok(current_exe)
}

#[cfg(windows)]
fn windows_helper_script() -> &'static str {
    r#"@echo off
setlocal
set "LING_UPDATE_PID=%~1"
set "LING_UPDATE_SRC=%~2"
set "LING_UPDATE_DST=%~3"
:wait
tasklist /FI "PID eq %LING_UPDATE_PID%" 2>NUL | findstr /R /C:"[ ]%LING_UPDATE_PID%[ ]" >NUL
if not errorlevel 1 (
  timeout /T 1 /NOBREAK >NUL
  goto wait
)
move /Y "%LING_UPDATE_SRC%" "%LING_UPDATE_DST%" >NUL
del "%~f0" >NUL 2>NUL
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_macos_arm64_target() {
        let target = target_for("macos", "arm64", None).unwrap();
        assert_eq!(target.triple, "aarch64-apple-darwin");
        assert_eq!(target.archive_ext, "tar.gz");
        assert_eq!(target.bin_ext, "");
    }

    #[test]
    fn resolves_macos_x86_64_target() {
        let target = target_for("macos", "x86_64", None).unwrap();
        assert_eq!(target.triple, "x86_64-apple-darwin");
    }

    #[test]
    fn resolves_linux_targets() {
        let musl = target_for("linux", "aarch64", None).unwrap();
        assert_eq!(musl.triple, "aarch64-unknown-linux-musl");

        let gnu = target_for("linux", "amd64", Some("gnu")).unwrap();
        assert_eq!(gnu.triple, "x86_64-unknown-linux-gnu");
    }

    #[test]
    fn resolves_windows_targets() {
        let target = target_for("windows", "x64", None).unwrap();
        assert_eq!(target.triple, "x86_64-pc-windows-msvc");
        assert_eq!(target.archive_ext, "zip");
        assert_eq!(target.bin_ext, ".exe");
    }

    #[test]
    fn rejects_unsupported_linux_libc() {
        let error = target_for("linux", "x86_64", Some("uclibc")).unwrap_err();
        assert!(error.to_string().contains("unsupported LING_LIBC"));
    }

    #[test]
    fn builds_release_asset_name() {
        let target = ReleaseTarget {
            triple: "aarch64-apple-darwin".to_owned(),
            archive_ext: "tar.gz",
            bin_ext: "",
        };
        assert_eq!(
            release_asset_name("v0.2.0", &target),
            "ling-v0.2.0-aarch64-apple-darwin.tar.gz"
        );
    }

    #[test]
    fn parses_release_versions_with_optional_v_prefix() {
        assert_eq!(
            parse_release_version("v0.2.0").unwrap(),
            Version::new(0, 2, 0)
        );
        assert_eq!(
            parse_release_version("0.2.0").unwrap(),
            Version::new(0, 2, 0)
        );
    }

    #[test]
    fn compares_current_and_latest_versions() {
        let current = Version::new(0, 2, 0);
        assert!(update_needed(&current, &Version::new(0, 2, 1)));
        assert!(!update_needed(&current, &Version::new(0, 2, 0)));
        assert!(!update_needed(&current, &Version::new(0, 1, 9)));
    }

    #[test]
    fn parses_checksum_for_asset() {
        let checksum = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
        let checksums = format!(
            "{checksum}  ling-v0.2.0-aarch64-apple-darwin.tar.gz\n\
             0000000000000000000000000000000000000000000000000000000000000000  other\n"
        );
        assert_eq!(
            expected_checksum(
                checksums.as_bytes(),
                "ling-v0.2.0-aarch64-apple-darwin.tar.gz"
            )
            .unwrap(),
            checksum
        );
    }

    #[test]
    fn verifies_archive_checksum() {
        let asset = "ling-v0.2.0-aarch64-apple-darwin.tar.gz";
        let checksum = sha256_hex(b"hello");
        let checksums = format!("{checksum}  {asset}\n");
        verify_archive_checksum(b"hello", checksums.as_bytes(), asset).unwrap();
    }

    #[test]
    fn rejects_checksum_mismatch() {
        let asset = "ling-v0.2.0-aarch64-apple-darwin.tar.gz";
        let checksums = format!("{}  {asset}\n", "0".repeat(64));
        let error = verify_archive_checksum(b"hello", checksums.as_bytes(), asset).unwrap_err();
        assert!(error.to_string().contains("checksum mismatch"));
    }
}
