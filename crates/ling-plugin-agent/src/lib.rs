use anyhow::{anyhow, bail, Context, Result};
use clap::Args;
use flate2::read::GzDecoder;
use regex::Regex;
use reqwest::Url;
use semver::Version;
use serde::Deserialize;
use std::{
    cmp::Ordering,
    env,
    ffi::{OsStr, OsString},
    fs::{self, File},
    io::{self, Cursor, IsTerminal, Write},
    path::{Component, Path, PathBuf},
    process::{Child, Command, ExitCode, Stdio},
    sync::OnceLock,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const MAX_DEPLOY_BUNDLE_BYTES: u64 = 10 * 1024 * 1024;
const MAX_SDK_ARCHIVE_BYTES: usize = 100 * 1024 * 1024;
const ESBUILD_VERSION: &str = "0.25.12";
const LOCAL_SDK_IMPORT: &str = "@listenai/agent-sdk";
const AGENT_PROJECT_VERSION_FILE: &str = ".version";
const DEFAULT_AGENT_TEMPLATE: &str = "listenai";
const DEFAULT_AGENT_TEMPLATE_PREFIX: &str = "flows-voice-chat";

#[derive(Debug, Args)]
pub struct CreateArgs {
    /// New project directory name.
    pub name: String,
    /// Template name.
    #[arg(long, default_value = "listenai")]
    pub template: String,
    /// Skip npm install after scaffolding.
    #[arg(long = "no-install")]
    pub no_install: bool,
}

#[derive(Debug, Args)]
pub struct BuildArgs {
    /// TypeScript entry file.
    #[arg(long, default_value = "agent.ts")]
    pub entry: String,
    /// Output JS bundle.
    #[arg(long, default_value = "dist/agent.js")]
    pub out: String,
    /// Minify and optimize for production.
    #[arg(long)]
    pub release: bool,
}

#[derive(Debug, Args)]
pub struct DeployArgs {
    /// Path to compiled JS bundle.
    #[arg(long, default_value = "dist/agent.js")]
    pub bundle: String,
    /// Product ID or App ID to deploy to.
    #[arg(long = "product-id")]
    pub product_id: String,
    /// Platform API endpoint. Defaults to the ling API base URL.
    #[arg(long)]
    pub endpoint: Option<String>,
    /// API key. Defaults to LING_API_KEY, saved ling config, or LISTENAI_API_KEY.
    #[arg(long = "api-key")]
    pub api_key: Option<String>,
    /// Agent version vX.Y.Z. Required. Plain X.Y.Z is accepted and normalized for upload.
    #[arg(long, required = true)]
    pub version: Option<String>,
    /// Agent version display name. Defaults to "<version> 版本".
    #[arg(long = "version-name")]
    pub version_name: Option<String>,
    /// Agent version description.
    #[arg(long)]
    pub description: Option<String>,
    /// Agent SDK version. Defaults to .version when present.
    #[arg(long = "sdk-version")]
    pub sdk_version: Option<String>,
    /// Publisher identity.
    #[arg(long = "published-by")]
    pub published_by: Option<String>,
    /// Print what would be deployed without uploading.
    #[arg(long = "dry-run")]
    pub dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DeployOptions {
    bundle: PathBuf,
    product_id: String,
    endpoint: String,
    api_key: Option<String>,
    version: String,
    version_name: Option<String>,
    description: Option<String>,
    sdk_version: Option<String>,
    published_by: Option<String>,
    dry_run: bool,
}

#[derive(Debug, Deserialize)]
struct DeployResponse {
    code: i64,
    #[serde(default)]
    message: String,
    data: Option<DeployData>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct DeployData {
    status: String,
    #[serde(rename = "appId")]
    app_id: String,
    version: String,
    #[serde(rename = "versionName")]
    version_name: String,
    description: String,
    #[serde(rename = "ossBucket")]
    oss_bucket: String,
    #[serde(rename = "ossPath")]
    oss_path: String,
    #[serde(rename = "fileSize")]
    file_size: u64,
    #[serde(rename = "fileHash")]
    file_hash: String,
}

#[derive(Debug, Deserialize)]
struct FrameworkSdkLatestResponse {
    code: String,
    #[serde(default)]
    message: String,
    data: Option<FrameworkSdkRecord>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct FrameworkSdkRecord {
    version: String,
    sdk: String,
    #[serde(default)]
    description: String,
}

struct CommandSpec {
    program: OsString,
    prefix_args: Vec<OsString>,
}

#[derive(Debug, Clone, Default)]
pub struct AgentContext {
    pub api_base_url: String,
    pub saved_api_key: Option<String>,
}

pub async fn create_command(ctx: &AgentContext, args: CreateArgs) -> Result<ExitCode> {
    let project_dir = scaffold_project(ctx, &args.name, &args.template).await?;
    if !args.no_install {
        install_project_deps(&project_dir)?;
    }
    Ok(ExitCode::SUCCESS)
}

pub async fn build_command(ctx: &AgentContext, args: BuildArgs) -> Result<ExitCode> {
    maybe_check_agent_project_version(ctx).await?;
    let entry = default_if_empty(&args.entry, "agent.ts");
    let out = default_if_empty(&args.out, "dist/agent.js");
    let status = run_esbuild(&entry, &out, args.release, false)?;
    if !status.success() {
        return Ok(exit_code(status.code().unwrap_or(1)));
    }

    let size = fs::metadata(&out).map(|st| st.len()).unwrap_or(0);
    println!("built {} ({} bytes)", out.display(), size);
    Ok(ExitCode::SUCCESS)
}

pub async fn dev_command(ctx: &AgentContext) -> Result<ExitCode> {
    maybe_check_agent_project_version(ctx).await?;
    run_dev()
}

pub async fn deploy_command(ctx: &AgentContext, args: DeployArgs) -> Result<ExitCode> {
    maybe_check_agent_project_version(ctx).await?;
    let opts = resolve_deploy_options(ctx, args)?;
    validate_deploy_bundle(&opts.bundle)?;

    println!(
        "Deploying {} -> product:{} version:{} via {}",
        opts.bundle.display(),
        opts.product_id,
        opts.version,
        opts.endpoint
    );

    if opts.dry_run {
        print_deploy_dry_run(&opts)?;
        return Ok(ExitCode::SUCCESS);
    }

    print_deploy_metadata(&opts);

    let api_key = opts
        .api_key
        .as_deref()
        .filter(|key| !key.trim().is_empty())
        .ok_or_else(|| {
            anyhow!("API key not set - provide --api-key, run `ling login`, or set LING_API_KEY")
        })?;
    let bundle = fs::read(&opts.bundle)
        .with_context(|| format!("read bundle: {}", opts.bundle.display()))?;
    if bundle.len() as u64 > MAX_DEPLOY_BUNDLE_BYTES {
        bail!(
            "bundle too large: {} is {} bytes, max is {} bytes",
            opts.bundle.display(),
            bundle.len(),
            MAX_DEPLOY_BUNDLE_BYTES
        );
    }

    let response = upload_agent_bundle(&opts, api_key, bundle).await?;
    print_deploy_success(response.data.as_ref().unwrap_or(&DeployData::default()));
    Ok(ExitCode::SUCCESS)
}

async fn scaffold_project(ctx: &AgentContext, name: &str, template: &str) -> Result<PathBuf> {
    let name = name.trim();
    if name.is_empty() {
        bail!("project name must not be empty");
    }
    let template = template.trim();
    if template.is_empty() {
        bail!("template name must not be empty");
    }

    let sdk = fetch_required_latest_framework_sdk(ctx).await?;
    println!("downloading Framework SDK {}...", sdk.version);
    let archive = download_framework_sdk_archive(ctx, &sdk).await?;
    let dest = PathBuf::from(name);
    extract_project_template_from_sdk_archive(&archive, template, &dest)?;
    normalize_scaffolded_project(&dest)?;
    write_agent_project_version(&dest.join(AGENT_PROJECT_VERSION_FILE), &sdk.version)?;
    println!(
        "created {} from template {}",
        dest.display(),
        display_template_name(template)
    );
    Ok(dest)
}

fn resolve_template_name(template: &str) -> &str {
    if template == DEFAULT_AGENT_TEMPLATE {
        DEFAULT_AGENT_TEMPLATE_PREFIX
    } else {
        template
    }
}

fn display_template_name(template: &str) -> &str {
    if template == DEFAULT_AGENT_TEMPLATE_PREFIX {
        DEFAULT_AGENT_TEMPLATE
    } else {
        template
    }
}

async fn maybe_check_agent_project_version(ctx: &AgentContext) -> Result<()> {
    let cwd = env::current_dir().context("failed to resolve current directory")?;
    if !is_agent_project_dir(&cwd) {
        return Ok(());
    }

    let marker = cwd.join(AGENT_PROJECT_VERSION_FILE);
    let latest_sdk = match fetch_latest_framework_sdk(ctx).await {
        Ok(Some(sdk)) => sdk,
        Ok(None) => {
            eprintln!("Warning: skip agent project version check: no Framework SDK release found.");
            return Ok(());
        }
        Err(err) => {
            eprintln!("Warning: skip agent project version check: {err:#}");
            return Ok(());
        }
    };
    let current = read_or_init_agent_project_version(&marker, &latest_sdk.version)?;
    match compare_project_versions(&current, &latest_sdk.version) {
        Ok(Ordering::Less) => {
            prompt_agent_project_update(ctx, &cwd, &marker, &current, &latest_sdk).await?
        }
        Ok(_) => {}
        Err(err) => {
            eprintln!("Warning: skip agent project version check: {err}");
        }
    }
    Ok(())
}

fn is_agent_project_dir(dir: &Path) -> bool {
    dir.join("listenai.toml").is_file()
        || dir.join("agent.ts").is_file()
        || dir.join("sdk/src/index.ts").is_file()
}

fn read_or_init_agent_project_version(marker: &Path, default_version: &str) -> Result<String> {
    if marker.is_file() {
        let version = fs::read_to_string(marker)
            .with_context(|| format!("read version marker: {}", marker.display()))?
            .trim()
            .to_string();
        if !version.is_empty() {
            return Ok(version);
        }
    }

    write_agent_project_version(marker, default_version)?;
    Ok(default_version.trim().to_string())
}

fn write_agent_project_version(marker: &Path, version: &str) -> Result<()> {
    fs::write(marker, format!("{}\n", version.trim()))
        .with_context(|| format!("write version marker: {}", marker.display()))
}

async fn fetch_required_latest_framework_sdk(ctx: &AgentContext) -> Result<FrameworkSdkRecord> {
    let url = framework_sdk_latest_url(ctx)?;
    fetch_latest_framework_sdk(ctx)
        .await?
        .ok_or_else(|| anyhow!("no Framework SDK release found from {url}"))
}

async fn fetch_latest_framework_sdk(ctx: &AgentContext) -> Result<Option<FrameworkSdkRecord>> {
    let url = framework_sdk_latest_url(ctx)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("build HTTP client")?;
    let mut request = client.get(url.clone()).header(
        reqwest::header::USER_AGENT,
        concat!("ling/", env!("CARGO_PKG_VERSION")),
    );
    if let Some(api_key) = framework_sdk_api_key(ctx) {
        request = request.bearer_auth(api_key);
    }

    let response = request
        .send()
        .await
        .with_context(|| format!("fetch latest Framework SDK from {url}"))?;

    let status = response.status();
    let body = response
        .bytes()
        .await
        .context("read latest Framework SDK response body")?;
    let body_text = String::from_utf8_lossy(&body).trim().to_string();
    if !status.is_success() {
        bail!(
            "latest Framework SDK request failed: HTTP {} {}",
            status.as_u16(),
            body_text
        );
    }

    let output: FrameworkSdkLatestResponse = serde_json::from_slice(&body)
        .with_context(|| format!("decode latest Framework SDK response: {body_text}"))?;
    if output.code != "SUCCESS" {
        let message = if output.message.trim().is_empty() {
            body_text
        } else {
            output.message
        };
        bail!(
            "latest Framework SDK request failed: code={} {}",
            output.code,
            message
        );
    }

    Ok(output.data.and_then(|sdk| {
        let version = sdk.version.trim();
        let url = sdk.sdk.trim();
        if version.is_empty() || url.is_empty() {
            None
        } else {
            Some(FrameworkSdkRecord {
                version: version.to_string(),
                sdk: url.to_string(),
                description: sdk.description,
            })
        }
    }))
}

fn framework_sdk_latest_url(ctx: &AgentContext) -> Result<Url> {
    let endpoint = format!("{}/", ctx.api_base_url.trim_end_matches('/'));
    let mut url = Url::parse(&endpoint)
        .with_context(|| format!("invalid API base URL: {}", ctx.api_base_url))?;
    {
        let mut segments = url.path_segments_mut().map_err(|_| {
            anyhow!(
                "invalid API base URL cannot be used for SDK lookup: {}",
                ctx.api_base_url
            )
        })?;
        segments.pop_if_empty();
        segments.extend(["external", "framework", "sdk", "latest"]);
    }
    Ok(url)
}

async fn download_framework_sdk_archive(
    ctx: &AgentContext,
    sdk: &FrameworkSdkRecord,
) -> Result<Vec<u8>> {
    let url = Url::parse(sdk.sdk.trim())
        .with_context(|| format!("invalid Framework SDK URL: {}", sdk.sdk))?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .context("build HTTP client")?;
    let mut request = client.get(url.clone()).header(
        reqwest::header::USER_AGENT,
        concat!("ling/", env!("CARGO_PKG_VERSION")),
    );
    if let Some(api_key) = framework_sdk_api_key(ctx) {
        request = request.bearer_auth(api_key);
    }

    let response = request
        .send()
        .await
        .with_context(|| format!("download Framework SDK from {url}"))?;

    let status = response.status();
    let bytes = response
        .bytes()
        .await
        .with_context(|| format!("read Framework SDK archive from {url}"))?;
    if !status.is_success() {
        let body = String::from_utf8_lossy(&bytes).trim().to_string();
        bail!(
            "download Framework SDK failed: HTTP {} {}",
            status.as_u16(),
            body
        );
    }
    if bytes.len() > MAX_SDK_ARCHIVE_BYTES {
        bail!(
            "Framework SDK archive too large: {} bytes, max is {} bytes",
            bytes.len(),
            MAX_SDK_ARCHIVE_BYTES
        );
    }
    Ok(bytes.to_vec())
}

async fn prompt_agent_project_update(
    ctx: &AgentContext,
    project_dir: &Path,
    marker: &Path,
    current: &str,
    latest: &FrameworkSdkRecord,
) -> Result<()> {
    let message = format!(
        "Agent project version {current} is older than latest {}. Update now? [y/N]: ",
        latest.version
    );

    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        eprintln!(
            "Agent project version {current} is older than latest {}; skip update prompt in non-interactive shell.",
            latest.version
        );
        return Ok(());
    }

    print!("{message}");
    io::stdout().flush().context("flush update prompt")?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("read update prompt input")?;
    match input.trim().to_ascii_lowercase().as_str() {
        "y" | "yes" => update_agent_project(ctx, project_dir, marker, latest).await,
        "n" | "no" | "" => {
            println!("skip agent project update");
            Ok(())
        }
        _ => {
            println!("skip agent project update");
            Ok(())
        }
    }
}

async fn update_agent_project(
    ctx: &AgentContext,
    project_dir: &Path,
    marker: &Path,
    latest: &FrameworkSdkRecord,
) -> Result<()> {
    println!("downloading Framework SDK {}...", latest.version);
    let archive = download_framework_sdk_archive(ctx, latest).await?;
    update_project_sdk_from_archive(&archive, project_dir)?;
    write_agent_project_version(marker, &latest.version)?;
    println!("agent project SDK updated to {}", latest.version);
    Ok(())
}

fn compare_project_versions(current: &str, latest: &str) -> Result<Ordering> {
    let current = parse_project_version(current)?;
    let latest = parse_project_version(latest)?;
    Ok(current.cmp(&latest))
}

fn parse_project_version(version: &str) -> Result<Version> {
    let version = version.trim().trim_start_matches('v');
    Version::parse(version).with_context(|| format!("invalid project version: {version:?}"))
}

fn extract_project_template_from_sdk_archive(
    archive: &[u8],
    template: &str,
    dest: &Path,
) -> Result<()> {
    let prefix = choose_project_template_prefix(archive, template)?;
    extract_archive_prefix(archive, prefix.as_deref(), dest)?;
    validate_scaffolded_project(dest)?;
    Ok(())
}

fn update_project_sdk_from_archive(archive: &[u8], project_dir: &Path) -> Result<()> {
    let prefix = choose_sdk_prefix(archive)?;
    let sdk_dir = project_dir.join("sdk");
    if sdk_dir.exists() {
        fs::remove_dir_all(&sdk_dir)
            .with_context(|| format!("remove old SDK directory: {}", sdk_dir.display()))?;
    }
    extract_archive_prefix(archive, Some(&prefix), &sdk_dir)?;
    if !sdk_dir.join("src/index.ts").is_file() {
        bail!(
            "Framework SDK archive did not provide sdk/src/index.ts after extraction to {}",
            sdk_dir.display()
        );
    }
    Ok(())
}

fn choose_project_template_prefix(archive: &[u8], template: &str) -> Result<Option<PathBuf>> {
    let paths = list_archive_paths(archive)?;
    let internal = resolve_template_name(template);
    let requested = Path::new(internal);
    if archive_has_project_at_prefix(&paths, Some(requested)) {
        return Ok(Some(requested.to_path_buf()));
    }

    if internal == DEFAULT_AGENT_TEMPLATE_PREFIX {
        let default = Path::new(DEFAULT_AGENT_TEMPLATE);
        if archive_has_project_at_prefix(&paths, Some(default)) {
            return Ok(Some(default.to_path_buf()));
        }
    } else {
        bail!("template {template} not found in downloaded Framework SDK archive");
    }

    if archive_has_project_at_prefix(&paths, None) {
        return Ok(None);
    }

    let candidates = project_prefixes(&paths);
    if candidates.len() == 1 {
        return Ok(candidates.into_iter().next().flatten());
    }

    bail!("template {template} not found in downloaded Framework SDK archive")
}

fn choose_sdk_prefix(archive: &[u8]) -> Result<PathBuf> {
    let paths = list_archive_paths(archive)?;
    for prefix in [
        PathBuf::from(DEFAULT_AGENT_TEMPLATE_PREFIX).join("sdk"),
        PathBuf::from(DEFAULT_AGENT_TEMPLATE).join("sdk"),
        PathBuf::from("sdk"),
        PathBuf::from("sdk-ts"),
    ] {
        if archive_has_sdk_at_prefix(&paths, &prefix) {
            return Ok(prefix);
        }
    }

    for prefix in project_prefixes(&paths).into_iter().flatten() {
        let sdk_prefix = prefix.join("sdk");
        if archive_has_sdk_at_prefix(&paths, &sdk_prefix) {
            return Ok(sdk_prefix);
        }
    }

    bail!("Framework SDK archive does not contain sdk/src/index.ts")
}

fn list_archive_paths(archive: &[u8]) -> Result<Vec<PathBuf>> {
    let decoder = GzDecoder::new(Cursor::new(archive));
    let mut archive = tar::Archive::new(decoder);
    let mut paths = Vec::new();
    for entry in archive
        .entries()
        .context("read Framework SDK archive entries")?
    {
        let entry = entry.context("read Framework SDK archive entry")?;
        let raw = entry
            .path()
            .context("read Framework SDK archive entry path")?;
        if let Some(path) = sanitize_archive_path(&raw) {
            paths.push(path);
        }
    }
    Ok(paths)
}

fn project_prefixes(paths: &[PathBuf]) -> Vec<Option<PathBuf>> {
    let mut prefixes = Vec::new();
    for path in paths {
        if path.file_name() != Some(OsStr::new("agent.ts")) {
            continue;
        }
        let prefix = path.parent().and_then(|parent| {
            if parent.as_os_str().is_empty() {
                None
            } else {
                Some(parent.to_path_buf())
            }
        });
        if !prefixes.contains(&prefix) && archive_has_project_at_prefix(paths, prefix.as_deref()) {
            prefixes.push(prefix);
        }
    }
    prefixes
}

fn archive_has_project_at_prefix(paths: &[PathBuf], prefix: Option<&Path>) -> bool {
    ["agent.ts", "package.json", "sdk/src/index.ts"]
        .iter()
        .all(|path| archive_has_path(paths, prefix, Path::new(path)))
}

fn archive_has_sdk_at_prefix(paths: &[PathBuf], prefix: &Path) -> bool {
    archive_has_path(paths, Some(prefix), Path::new("src/index.ts"))
}

fn archive_has_path(paths: &[PathBuf], prefix: Option<&Path>, relative: &Path) -> bool {
    let expected = prefix
        .filter(|prefix| !prefix.as_os_str().is_empty())
        .map(|prefix| prefix.join(relative))
        .unwrap_or_else(|| relative.to_path_buf());
    paths.iter().any(|path| path == &expected)
}

fn extract_archive_prefix(archive: &[u8], prefix: Option<&Path>, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest).with_context(|| format!("create directory: {}", dest.display()))?;
    let decoder = GzDecoder::new(Cursor::new(archive));
    let mut archive = tar::Archive::new(decoder);
    for entry in archive
        .entries()
        .context("read Framework SDK archive entries")?
    {
        let mut entry = entry.context("read Framework SDK archive entry")?;
        let raw = entry
            .path()
            .context("read Framework SDK archive entry path")?;
        let Some(path) = sanitize_archive_path(&raw) else {
            continue;
        };
        let relative = match prefix.filter(|prefix| !prefix.as_os_str().is_empty()) {
            Some(prefix) => match path.strip_prefix(prefix) {
                Ok(relative) if !relative.as_os_str().is_empty() => relative.to_path_buf(),
                _ => continue,
            },
            None => path,
        };
        let output = dest.join(relative);
        let entry_type = entry.header().entry_type();
        if entry_type.is_dir() {
            fs::create_dir_all(&output)
                .with_context(|| format!("create directory: {}", output.display()))?;
        } else if entry_type.is_file() {
            if let Some(parent) = output.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("create directory: {}", parent.display()))?;
            }
            let mut file = File::create(&output)
                .with_context(|| format!("write file: {}", output.display()))?;
            io::copy(&mut entry, &mut file)
                .with_context(|| format!("extract file: {}", output.display()))?;
        }
    }
    Ok(())
}

fn sanitize_archive_path(path: &Path) -> Option<PathBuf> {
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => return None,
        }
    }
    if clean.as_os_str().is_empty() {
        None
    } else {
        Some(clean)
    }
}

fn normalize_scaffolded_project(project_dir: &Path) -> Result<()> {
    validate_scaffolded_project(project_dir)?;
    let package_json = project_dir.join("package.json");
    let package = fs::read_to_string(&package_json)
        .with_context(|| format!("read package.json: {}", package_json.display()))?;
    let mut value: serde_json::Value =
        serde_json::from_str(&package).context("decode generated package.json")?;

    let object = value
        .as_object_mut()
        .ok_or_else(|| anyhow!("generated package.json must be a JSON object"))?;
    let scripts = object
        .entry("scripts")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow!("generated package.json scripts must be a JSON object"))?;
    scripts.insert("build".to_string(), serde_json::json!("ling build"));
    scripts.insert("dev".to_string(), serde_json::json!("ling dev"));

    let dev_dependencies = object
        .entry("devDependencies")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow!("generated package.json devDependencies must be a JSON object"))?;
    dev_dependencies
        .entry("esbuild".to_string())
        .or_insert_with(|| serde_json::json!(format!("^{ESBUILD_VERSION}")));
    dev_dependencies
        .entry("typescript".to_string())
        .or_insert_with(|| serde_json::json!("^5.3.0"));

    fs::write(
        &package_json,
        format!("{}\n", serde_json::to_string_pretty(&value)?),
    )
    .with_context(|| format!("write package.json: {}", package_json.display()))?;
    Ok(())
}

fn validate_scaffolded_project(project_dir: &Path) -> Result<()> {
    for path in ["agent.ts", "package.json", "sdk/src/index.ts"] {
        let full = project_dir.join(path);
        if !full.is_file() {
            bail!(
                "Framework SDK template is missing required file {} after extraction",
                full.display()
            );
        }
    }
    Ok(())
}

fn install_project_deps(project_dir: &Path) -> Result<()> {
    let package_json = project_dir.join("package.json");
    if !package_json.is_file() {
        bail!(
            "project created at {}, but package.json was not found; skip dependency install",
            project_dir.display()
        );
    }

    let npm = find_on_path_candidates("npm").ok_or_else(|| {
        anyhow!(
            "project created at {}, but npm was not found. Install Node.js/npm, then run `cd {} && npm install`",
            project_dir.display(),
            project_dir.display()
        )
    })?;

    println!("installing dependencies in {}...", project_dir.display());
    let mut command = Command::new(&npm);
    set_command_path(&mut command, npm.as_os_str());
    let status = command
        .arg("install")
        .current_dir(project_dir)
        .status()
        .with_context(|| format!("failed to run npm install in {}", project_dir.display()))?;
    if !status.success() {
        bail!(
            "project created at {}, but npm install failed. Retry with `cd {} && npm install`",
            project_dir.display(),
            project_dir.display()
        );
    }
    println!("dependencies installed");
    Ok(())
}

fn run_esbuild(
    entry: &Path,
    out: &Path,
    release: bool,
    watch: bool,
) -> Result<std::process::ExitStatus> {
    prepare_build_inputs(entry, out)?;
    let spec = resolve_esbuild_command()?;
    let args = esbuild_args(entry, out, release, watch)?;
    let mut command = Command::new(&spec.program);
    set_command_path(&mut command, &spec.program);
    command.args(spec.prefix_args).args(args);
    command.status().with_context(|| {
        format!(
            "failed to run esbuild via {}. Install Node.js/npm, run `npm install`, or set LING_ESBUILD_BIN",
            spec.program.to_string_lossy()
        )
    })
}

fn spawn_esbuild_watch(entry: &Path, out: &Path) -> Result<Child> {
    prepare_build_inputs(entry, out)?;
    let spec = resolve_esbuild_command()?;
    let args = esbuild_args(entry, out, false, true)?;
    let mut command = Command::new(&spec.program);
    set_command_path(&mut command, &spec.program);
    command
        .args(spec.prefix_args)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    command.spawn().with_context(|| {
        format!(
            "failed to start esbuild watch via {}. Install Node.js/npm, run `npm install`, or set LING_ESBUILD_BIN",
            spec.program.to_string_lossy()
        )
    })
}

fn prepare_build_inputs(entry: &Path, out: &Path) -> Result<()> {
    if !entry.exists() {
        bail!(
            "entry not found: {} - run `ling create` first",
            entry.display()
        );
    }
    let sdk = env::current_dir()
        .context("failed to resolve current directory")?
        .join("sdk/src/index.ts");
    if !sdk.exists() {
        bail!(
            "local SDK not found: {} - run `ling create` or restore sdk/src/index.ts",
            sdk.display()
        );
    }
    if let Some(parent) = out.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .with_context(|| format!("create output dir: {}", parent.display()))?;
    }
    Ok(())
}

fn resolve_esbuild_command() -> Result<CommandSpec> {
    if let Ok(path) = env::var("LING_ESBUILD_BIN") {
        if !path.trim().is_empty() {
            return Ok(CommandSpec {
                program: OsString::from(path),
                prefix_args: vec![],
            });
        }
    }

    if let Some(local) = find_local_node_bin("esbuild") {
        return Ok(CommandSpec {
            program: local.into_os_string(),
            prefix_args: vec![],
        });
    }

    if let Some(path) = find_on_path_candidates("esbuild") {
        return Ok(CommandSpec {
            program: path.into_os_string(),
            prefix_args: vec![],
        });
    }

    if let Some(path) = find_on_path_candidates("npx") {
        return Ok(CommandSpec {
            program: path.into_os_string(),
            prefix_args: vec![
                OsString::from("--yes"),
                OsString::from(format!("esbuild@{ESBUILD_VERSION}")),
            ],
        });
    }

    if let Some(path) = find_on_path_candidates("npm") {
        return Ok(CommandSpec {
            program: path.into_os_string(),
            prefix_args: vec![
                OsString::from("exec"),
                OsString::from("--yes"),
                OsString::from(format!("esbuild@{ESBUILD_VERSION}")),
                OsString::from("--"),
            ],
        });
    }

    bail!("esbuild not found. Install Node.js/npm and run again, or set LING_ESBUILD_BIN=/path/to/esbuild")
}

fn esbuild_args(entry: &Path, out: &Path, release: bool, watch: bool) -> Result<Vec<OsString>> {
    let sdk = env::current_dir()
        .context("failed to resolve current directory")?
        .join("sdk/src/index.ts");
    let mut args = vec![
        entry.as_os_str().to_os_string(),
        OsString::from("--bundle"),
        OsString::from("--format=iife"),
        OsString::from("--target=es2017"),
        OsString::from(format!("--outfile={}", out.display())),
        OsString::from(format!("--alias:{LOCAL_SDK_IMPORT}={}", sdk.display())),
        OsString::from("--log-level=info"),
    ];
    if release {
        args.push(OsString::from("--minify"));
    } else {
        args.push(OsString::from("--sourcemap"));
    }
    if watch {
        args.push(OsString::from("--watch=forever"));
    }
    Ok(args)
}

fn run_dev() -> Result<ExitCode> {
    let cwd = env::current_dir().context("failed to resolve current directory")?;
    let entry = cwd.join("agent.ts");
    if !entry.exists() {
        bail!(
            "agent.ts not found in {} - run `ling create` first",
            cwd.display()
        );
    }

    let node = find_on_path_candidates("node")
        .ok_or_else(|| anyhow!("node not found. Install Node.js to use `ling dev`"))?;
    let tmp = temp_dir("ling-dev")?;
    let bundle = tmp.join("agent.js");
    let harness = tmp.join("dev-harness.cjs");
    fs::write(&harness, DEV_HARNESS)
        .with_context(|| format!("write dev harness: {}", harness.display()))?;

    let mut watch = spawn_esbuild_watch(&entry, &bundle)?;
    println!("ling dev: watching {}", entry.display());
    println!("Type a message and press ENTER to send a mock ASR final frame. Ctrl+C to exit.");

    let status = Command::new(node)
        .arg(&harness)
        .arg(&bundle)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("failed to run Node.js dev harness")?;

    let _ = watch.kill();
    let _ = watch.wait();
    let _ = fs::remove_dir_all(tmp);

    if status.success() {
        Ok(ExitCode::SUCCESS)
    } else {
        Ok(exit_code(status.code().unwrap_or(1)))
    }
}

fn resolve_deploy_options(ctx: &AgentContext, args: DeployArgs) -> Result<DeployOptions> {
    let product_id = args.product_id.trim().to_string();
    if product_id.is_empty() {
        bail!("product-id required");
    }

    let endpoint = args
        .endpoint
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&ctx.api_base_url)
        .to_string();
    let raw_version = args.version.as_deref().unwrap_or_default().trim();
    if raw_version.is_empty() {
        bail!("version required: pass --version vX.Y.Z");
    }
    let version = resolve_deploy_version(raw_version)?;
    let version_name =
        clean_optional(args.version_name).or_else(|| Some(default_version_name(raw_version)));
    let sdk_version = clean_optional(args.sdk_version).or_else(read_agent_sdk_version_from_marker);
    let api_key = if args.dry_run {
        None
    } else {
        Some(resolve_deploy_api_key(
            args.api_key.as_deref(),
            ctx.saved_api_key.clone(),
        )?)
    };

    Ok(DeployOptions {
        bundle: default_if_empty(&args.bundle, "dist/agent.js"),
        product_id,
        endpoint,
        api_key,
        version,
        version_name,
        description: clean_optional(args.description),
        sdk_version,
        published_by: clean_optional(args.published_by),
        dry_run: args.dry_run,
    })
}

fn validate_deploy_bundle(bundle: &Path) -> Result<()> {
    let st = fs::metadata(bundle).with_context(|| {
        format!(
            "bundle not found: {} - run `ling build` first",
            bundle.display()
        )
    })?;
    if !st.is_file() {
        bail!("bundle is not a file: {}", bundle.display());
    }
    if st.len() > MAX_DEPLOY_BUNDLE_BYTES {
        bail!(
            "bundle too large: {} is {} bytes, max is {} bytes",
            bundle.display(),
            st.len(),
            MAX_DEPLOY_BUNDLE_BYTES
        );
    }
    Ok(())
}

fn resolve_deploy_api_key(flag: Option<&str>, config_key: Option<String>) -> Result<String> {
    choose_deploy_api_key(
        flag,
        env::var("LING_API_KEY").ok(),
        config_key,
        env::var("LISTENAI_API_KEY").ok(),
    )
}

fn choose_deploy_api_key(
    flag: Option<&str>,
    ling_env: Option<String>,
    config_key: Option<String>,
    legacy_env: Option<String>,
) -> Result<String> {
    for candidate in [
        flag.map(ToOwned::to_owned),
        ling_env,
        config_key,
        legacy_env,
    ]
    .into_iter()
    .flatten()
    {
        let key = strip_bearer(&candidate);
        if !key.trim().is_empty() {
            return Ok(key);
        }
    }

    bail!("API key not set - provide --api-key, run `ling login`, or set LING_API_KEY")
}

fn framework_sdk_api_key(ctx: &AgentContext) -> Option<String> {
    choose_deploy_api_key(
        None,
        env::var("LING_API_KEY").ok(),
        ctx.saved_api_key.clone(),
        env::var("LISTENAI_API_KEY").ok(),
    )
    .ok()
}

fn strip_bearer(api_key: &str) -> String {
    let api_key = api_key.trim();
    if api_key.to_ascii_lowercase().starts_with("bearer ") {
        api_key[7..].trim().to_owned()
    } else {
        api_key.to_owned()
    }
}

fn resolve_deploy_version(version: &str) -> Result<String> {
    let mut version = version.trim().to_string();
    if plain_version_regex().is_match(&version) {
        version = format!("v{version}");
    }
    if !agent_version_regex().is_match(&version) {
        bail!("version must match vX.Y.Z, got {version:?}");
    }
    Ok(version)
}

fn default_version_name(raw_version: &str) -> String {
    format!("{} 版本", raw_version.trim())
}

fn read_agent_sdk_version_from_marker() -> Option<String> {
    fs::read_to_string(AGENT_PROJECT_VERSION_FILE)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn agent_version_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"^v\d+\.\d+\.\d+$").expect("valid regex"))
}

fn plain_version_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"^\d+\.\d+\.\d+$").expect("valid regex"))
}

fn agent_deploy_url(opts: &DeployOptions) -> Result<Url> {
    let endpoint = format!("{}/", opts.endpoint.trim_end_matches('/'));
    let mut url =
        Url::parse(&endpoint).with_context(|| format!("invalid endpoint: {}", opts.endpoint))?;
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| anyhow!("invalid endpoint cannot be a base URL: {}", opts.endpoint))?;
        segments.pop_if_empty();
        segments.extend(["v1", "framework", "agents", opts.product_id.as_str()]);
    }
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("version", &opts.version);
        for (key, value) in [
            ("version_name", opts.version_name.as_deref()),
            ("description", opts.description.as_deref()),
            ("sdk_version", opts.sdk_version.as_deref()),
            ("published_by", opts.published_by.as_deref()),
        ] {
            if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
                q.append_pair(key, value);
            }
        }
    }
    Ok(url)
}

async fn upload_agent_bundle(
    opts: &DeployOptions,
    api_key: &str,
    bundle: Vec<u8>,
) -> Result<DeployResponse> {
    let url = agent_deploy_url(opts)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .context("build HTTP client")?;
    let response = client
        .put(url.clone())
        .bearer_auth(api_key.trim())
        .header(reqwest::header::CONTENT_TYPE, "application/javascript")
        .header(
            reqwest::header::USER_AGENT,
            concat!("ling/", env!("CARGO_PKG_VERSION")),
        )
        .body(bundle)
        .send()
        .await
        .with_context(|| format!("upload agent bundle to {url}"))?;

    let status = response.status();
    let body = response
        .bytes()
        .await
        .context("read deploy response body")?;
    let body_text = String::from_utf8_lossy(&body).trim().to_string();
    if !status.is_success() {
        bail!("deploy failed: HTTP {} {}", status.as_u16(), body_text);
    }

    let output: DeployResponse = serde_json::from_slice(&body)
        .with_context(|| format!("decode deploy response: {body_text}"))?;
    if output.code != 0 {
        let message = if output.message.trim().is_empty() {
            body_text
        } else {
            output.message.clone()
        };
        bail!("deploy failed: code={} {}", output.code, message);
    }
    Ok(output)
}

fn print_deploy_dry_run(opts: &DeployOptions) -> Result<()> {
    let url = agent_deploy_url(opts)?;
    println!("Dry run - skipping upload.");
    println!("URL: {url}");
    print_deploy_metadata(opts);
    Ok(())
}

fn print_deploy_metadata(opts: &DeployOptions) {
    println!("Bundle: {}", opts.bundle.display());
    println!("Product/App ID: {}", opts.product_id);
    println!("Version: {}", opts.version);
    if let Some(value) = &opts.version_name {
        println!("Version name: {value}");
    }
    if let Some(value) = &opts.description {
        println!("Description: {value}");
    }
    if let Some(value) = &opts.sdk_version {
        println!("SDK version: {value}");
    }
    if let Some(value) = &opts.published_by {
        println!("Published by: {value}");
    }
}

fn print_deploy_success(data: &DeployData) {
    println!("Deploy succeeded.");
    if !data.status.is_empty() {
        println!("Status: {}", data.status);
    }
    if !data.app_id.is_empty() {
        println!("App ID: {}", data.app_id);
    }
    if !data.version.is_empty() {
        println!("Version: {}", data.version);
    }
    if !data.version_name.is_empty() {
        println!("Version name: {}", data.version_name);
    }
    if !data.description.is_empty() {
        println!("Description: {}", data.description);
    }
    if data.file_size > 0 {
        println!("File size: {} bytes", data.file_size);
    }
    if !data.file_hash.is_empty() {
        println!("File hash: {}", data.file_hash);
    }
    if !data.oss_bucket.is_empty() || !data.oss_path.is_empty() {
        println!("OSS: {}/{}", data.oss_bucket, data.oss_path);
    }
}

fn default_if_empty(value: &str, default: &str) -> PathBuf {
    let value = value.trim();
    PathBuf::from(if value.is_empty() { default } else { value })
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn exit_code(code: i32) -> ExitCode {
    if code == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(code.clamp(1, u8::MAX as i32) as u8)
    }
}

fn executable_candidates(name: &str) -> Vec<OsString> {
    if cfg!(windows) {
        windows_executable_candidates(name)
    } else {
        vec![OsString::from(name)]
    }
}

fn windows_executable_candidates(name: &str) -> Vec<OsString> {
    [
        format!("{name}.cmd"),
        format!("{name}.exe"),
        format!("{name}.bat"),
        name.to_string(),
    ]
    .into_iter()
    .map(OsString::from)
    .collect()
}

fn find_local_node_bin(name: &str) -> Option<PathBuf> {
    executable_candidates(name)
        .into_iter()
        .map(|candidate| PathBuf::from("node_modules").join(".bin").join(candidate))
        .find(|candidate| candidate.is_file())
}

fn find_on_path_candidates(name: &str) -> Option<PathBuf> {
    executable_candidates(name)
        .into_iter()
        .find_map(find_on_path)
}

fn find_on_path(name: impl AsRef<OsStr>) -> Option<PathBuf> {
    let name = name.as_ref();
    if let Some(path) = env::var_os("PATH") {
        if let Some(found) = env::split_paths(&path)
            .map(|dir| dir.join(name))
            .find(|candidate| candidate.is_file())
        {
            return Some(found);
        }
    }

    if cfg!(windows) {
        return None;
    }

    ["/opt/homebrew/bin", "/usr/local/bin"]
        .into_iter()
        .map(|dir| Path::new(dir).join(name))
        .find(|candidate| candidate.is_file())
}

fn set_command_path(command: &mut Command, program: &OsStr) {
    let mut paths = Vec::new();
    if let Some(parent) = Path::new(program)
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        paths.push(parent.to_path_buf());
    }
    if !cfg!(windows) {
        for dir in ["/opt/homebrew/bin", "/usr/local/bin"] {
            let path = PathBuf::from(dir);
            if path.is_dir() {
                paths.push(path);
            }
        }
    }
    if let Some(existing) = env::var_os("PATH") {
        paths.extend(env::split_paths(&existing));
    }
    if let Ok(path) = env::join_paths(paths) {
        command.env("PATH", path);
    }
}

fn temp_dir(prefix: &str) -> Result<PathBuf> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock before Unix epoch")?
        .as_nanos();
    let dir = env::temp_dir().join(format!("{prefix}-{}-{now}", std::process::id()));
    fs::create_dir_all(&dir).with_context(|| format!("create temp dir: {}", dir.display()))?;
    Ok(dir)
}

const DEV_HARNESS: &str = r#"
const fs = require('fs');
const vm = require('vm');
const readline = require('readline');

const bundle = process.argv[2];
const sessionId = 'dev-session-1';
const deviceId = 'mock';
const productId = 'prod_dev_local';
let streamSeq = 0;
const llmStreams = new Map();
const ttsStreams = new Map();
const textStreams = new Map();
const history = new Map();
const kv = new Map();

function nextId(prefix) { streamSeq += 1; return `${prefix}-${streamSeq}`; }
function print(prefix, value) {
  if (typeof value === 'string') console.log(`${prefix}${value}`);
  else console.log(`${prefix}${JSON.stringify(value)}`);
}
function sleep(ms) {
  const sab = new SharedArrayBuffer(4);
  Atomics.wait(new Int32Array(sab), 0, 0, Math.max(0, Number(ms) || 0));
}
function mockChunks(req) {
  const last = Array.isArray(req?.messages) ? req.messages[req.messages.length - 1] : null;
  const text = last?.content ? String(last.content) : 'hello';
  return [
    { delta: `mock: ${text} `, done: false },
    { delta: 'world', done: false },
    { delta: '', done: true },
  ];
}
function emitPayload(sessionId, payload) {
  if (payload?.tag === 'text') print('[device] ', payload.val);
  else print('[device] ', { sessionId, payload });
}

globalThis.host = {
  llmChat(req) { const id = nextId('llm'); llmStreams.set(id, mockChunks(req)); return id; },
  llmPoll(id) { const q = llmStreams.get(id); if (!q) return { delta: '', done: true }; return q.shift() ?? null; },
  llmSubmitToolResult() {},
  llmStart(req, handlers = {}) {
    const id = nextId('llm');
    for (const chunk of mockChunks(req)) {
      if (chunk.done) handlers.onDone?.();
      else handlers.onChunk?.(chunk);
    }
    return id;
  },
  ttsSynthesize(req) { const id = nextId('tts'); ttsStreams.set(id, [{ audio: new Uint8Array(), codec: 'pcm16le-16k', done: true }]); return id; },
  ttsPoll(id) { const q = ttsStreams.get(id); if (!q) return { audio: new Uint8Array(), codec: 'pcm16le-16k', done: true }; return q.shift() ?? null; },
  ttsOpen() { const id = nextId('tts'); ttsStreams.set(id, []); return { id, url: `mock://tts/${id}` }; },
  ttsSend(id, text) { print('[tts] ', { id, text }); },
  ttsClose(id) { print('[tts] ', { id, closed: true }); },
  ttsCloseActive() { print('[tts] ', { closedActive: true }); },
  textStreamOpen() { const id = nextId('text'); textStreams.set(id, []); return { id, url: `mock://text/${id}` }; },
  textStreamSend(id, text) { print('[text-stream] ', { id, text }); },
  textStreamClose(id) { print('[text-stream] ', { id, closed: true }); },
  textStreamCloseActive() { print('[text-stream] ', { closedActive: true }); },
  knowledgeQuery(req) { return JSON.stringify({ data: [], query: req?.content ?? '' }); },
  xiaolingAgentChat(req) { const id = nextId('xiaoling'); llmStreams.set(id, mockChunks(req).map(c => ({ delta: c.delta, answer: c.delta, done: c.done, isStoreChat: false, isAction: false }))); return id; },
  xiaolingAgentPoll(id) { const q = llmStreams.get(id); if (!q) return { delta: '', answer: '', done: true, isStoreChat: false, isAction: false }; return q.shift() ?? null; },
  aiuiSkill(req) { return { raw: '', nlp: '', text: req?.text ?? '', accepted: false, rc4: false, unsupportedSkill: true }; },
  historyGet(sessionId, limit) { const rows = history.get(sessionId) ?? []; return limit ? rows.slice(-limit) : rows.slice(); },
  historyAppend(sessionId, messages) { const rows = history.get(sessionId) ?? []; history.set(sessionId, rows.concat(messages)); },
  historyClear(sessionId) { history.delete(sessionId); },
  kvGet(key) { return kv.has(key) ? kv.get(key) : null; },
  kvSet(key, value) { kv.set(key, String(value)); },
  emit: emitPayload,
  httpRequest(method, url) { print('[http] ', { method, url, skipped: true }); return ''; },
  log(level, message, fields = {}) { print('[log] ', { level, message, fields }); },
  httpStream() { return nextId('http-stream'); },
  httpStreamPoll() { return { status: 200, headers: [], bodyPart: new Uint8Array(), done: true }; },
  wsConnect(url) { print('[ws] ', { url, skipped: true }); return nextId('ws'); },
  wsSend() {},
  wsPoll() { return { tag: 'closed', val: { code: 1000, reason: 'mock' } }; },
  wsClose() {},
  sleep,
};

async function callMaybe(fn, arg) {
  const out = fn?.(arg);
  if (out && typeof out.then === 'function') await out;
}
async function loadBundle(reason) {
  try {
    const code = fs.readFileSync(bundle, 'utf8');
    delete globalThis.guest;
    vm.runInThisContext(code, { filename: bundle });
    if (!globalThis.guest || typeof globalThis.guest.onMessage !== 'function') {
      throw new Error('agent did not register globalThis.guest.onMessage');
    }
    await callMaybe(globalThis.guest.onConnect, {
      sessionId, deviceId, productId,
      connectedAt: new Date().toISOString(),
      isReconnect: reason !== 'initial', reconnectCount: reason === 'initial' ? 0 : 1,
    });
    console.log(`reload: ready (${reason})`);
  } catch (err) {
    console.error('reload failed:', err && err.stack ? err.stack : err);
  }
}
async function waitForInitialBuild() {
  while (!fs.existsSync(bundle)) {
    await new Promise(resolve => setTimeout(resolve, 100));
  }
}
async function main() {
  await waitForInitialBuild();
  await loadBundle('initial');
  fs.watchFile(bundle, { interval: 250 }, async (cur, prev) => {
    if (cur.mtimeMs !== prev.mtimeMs) await loadBundle('change');
  });
  const rl = readline.createInterface({ input: process.stdin, output: process.stdout });
  rl.on('line', async line => {
    if (line.trim() === '.reload') return loadBundle('manual');
    try {
      await callMaybe(globalThis.guest?.onMessage, {
        sessionId, deviceId, productId,
        content: { tag: 'text', val: line },
        isLast: true,
        params: [],
        timestampMs: Date.now(),
      });
    } catch (err) {
      console.error('agent error:', err && err.stack ? err.stack : err);
    }
  });
  rl.on('close', () => {
    fs.unwatchFile(bundle);
    process.exit(0);
  });
}
main().catch(err => { console.error(err && err.stack ? err.stack : err); process.exitCode = 1; });
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{write::GzEncoder, Compression};
    use std::sync::Mutex;

    #[test]
    fn create_scaffolds_project_from_sdk_archive() {
        let dir = temp_dir("ling-create-test").expect("temp dir");
        let project = dir.join("my-agent");
        let archive = fixture_sdk_archive();
        extract_project_template_from_sdk_archive(&archive, "listenai", &project)
            .expect("extract template");
        normalize_scaffolded_project(&project).expect("normalize");
        write_agent_project_version(&project.join(AGENT_PROJECT_VERSION_FILE), "0.1.0-mvp.0")
            .expect("write version");

        assert!(project.join("agent.ts").is_file());
        assert!(project.join("listenai.toml").is_file());
        assert_eq!(
            fs::read_to_string(project.join(AGENT_PROJECT_VERSION_FILE))
                .expect("version marker")
                .trim(),
            "0.1.0-mvp.0"
        );
        assert!(project.join("sdk/src/index.ts").is_file());
        let package = fs::read_to_string(project.join("package.json")).expect("package");
        assert!(package.contains("ling build"));
        assert!(package.contains("ling dev"));
        assert!(package.contains("esbuild"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn create_rejects_unknown_template() {
        let dir = temp_dir("ling-missing-template-test").expect("temp dir");
        let archive = fixture_sdk_archive();
        let err = extract_project_template_from_sdk_archive(&archive, "missing-template", &dir)
            .expect_err("missing template");
        assert!(format!("{err:?}").contains("template missing-template not found"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn listenai_template_alias_uses_voice_chat_files() {
        assert_eq!(resolve_template_name("listenai"), "flows-voice-chat");
        assert_eq!(display_template_name("flows-voice-chat"), "listenai");
        assert_eq!(display_template_name("sdk-showcase"), "sdk-showcase");
    }

    #[test]
    fn compares_project_versions_semantically() {
        assert_eq!(
            compare_project_versions("0.1.0", "0.1.1").expect("compare"),
            Ordering::Less
        );
        assert_eq!(
            compare_project_versions("v1.10.0", "v1.2.0").expect("compare"),
            Ordering::Greater
        );
        assert_eq!(
            compare_project_versions("1.0.0", "v1.0.0").expect("compare"),
            Ordering::Equal
        );
        assert_eq!(
            compare_project_versions("0.1.0-mvp.0", "0.1.0").expect("compare"),
            Ordering::Less
        );
        assert!(compare_project_versions("1.0", "1.0.0").is_err());
    }

    #[test]
    fn read_or_init_project_version_writes_marker() {
        let dir = temp_dir("ling-project-version-test").expect("temp dir");
        let marker = dir.join(AGENT_PROJECT_VERSION_FILE);
        let version = read_or_init_agent_project_version(&marker, "0.1.0-mvp.0").expect("version");

        assert_eq!(version, "0.1.0-mvp.0");
        assert_eq!(
            fs::read_to_string(&marker).expect("marker").trim(),
            "0.1.0-mvp.0"
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn update_project_sdk_from_archive_replaces_sdk_files() {
        let dir = temp_dir("ling-project-update-test").expect("temp dir");
        let project = dir.join("my-agent");
        fs::create_dir_all(project.join("sdk/src")).expect("create old sdk");
        fs::write(project.join("sdk/src/index.ts"), "old").expect("write old sdk");

        let archive = fixture_sdk_archive();
        update_project_sdk_from_archive(&archive, &project).expect("update sdk");

        assert_eq!(
            fs::read_to_string(project.join("sdk/src/index.ts")).expect("sdk"),
            "export const sdk = true;\n"
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn esbuild_args_include_local_sdk_alias_and_release_flags() {
        let _guard = cwd_lock().lock().expect("cwd lock");
        let dir = temp_dir("ling-build-args-test").expect("temp dir");
        let old = env::current_dir().expect("cwd");
        env::set_current_dir(&dir).expect("set cwd");
        let args = esbuild_args(
            Path::new("agent.ts"),
            Path::new("dist/agent.js"),
            true,
            false,
        )
        .expect("args");
        env::set_current_dir(old).expect("restore cwd");

        let args = args
            .into_iter()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        assert!(args.contains(&"--format=iife".to_string()));
        assert!(args.contains(&"--target=es2017".to_string()));
        assert!(args.contains(&"--minify".to_string()));
        assert!(args
            .iter()
            .any(|arg| arg.starts_with("--alias:@listenai/agent-sdk=")));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn deploy_url_contains_version_metadata() {
        let opts = DeployOptions {
            bundle: PathBuf::from("dist/agent.js"),
            product_id: "prod_dev_local".to_string(),
            endpoint: "https://api.listenai.com".to_string(),
            api_key: None,
            version: "v1.0.0".to_string(),
            version_name: Some("首次发布".to_string()),
            description: Some("支持基础语音对话".to_string()),
            sdk_version: Some("0.1.0".to_string()),
            published_by: Some("tester".to_string()),
            dry_run: true,
        };

        let url = agent_deploy_url(&opts).expect("url");
        assert_eq!(url.path(), "/v1/framework/agents/prod_dev_local");
        let query = url.query().expect("query");
        assert!(query.contains("version=v1.0.0"));
        assert!(query.contains("version_name="));
        assert!(query.contains("sdk_version=0.1.0"));
        assert!(query.contains("published_by=tester"));
    }

    #[test]
    fn deploy_options_default_version_name_and_sdk_version_from_marker() {
        let _guard = cwd_lock().lock().expect("cwd lock");
        let dir = temp_dir("ling-deploy-options-test").expect("temp dir");
        let old = env::current_dir().expect("cwd");
        fs::write(dir.join(AGENT_PROJECT_VERSION_FILE), "0.1.0\n").expect("write version");
        env::set_current_dir(&dir).expect("set cwd");
        let opts = resolve_deploy_options(
            &AgentContext {
                api_base_url: "https://api.listenai.com".to_string(),
                saved_api_key: None,
            },
            DeployArgs {
                bundle: "dist/agent.js".to_string(),
                product_id: "prod_dev_local".to_string(),
                endpoint: None,
                api_key: None,
                version: Some("0.2.0".to_string()),
                version_name: None,
                description: None,
                sdk_version: None,
                published_by: None,
                dry_run: true,
            },
        )
        .expect("deploy options");
        env::set_current_dir(old).expect("restore cwd");

        assert_eq!(opts.version, "v0.2.0");
        assert_eq!(opts.version_name.as_deref(), Some("0.2.0 版本"));
        assert_eq!(opts.sdk_version.as_deref(), Some("0.1.0"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn deploy_version_rejects_invalid_version() {
        let err = resolve_deploy_version("1.2").expect_err("invalid version");
        assert!(format!("{err:?}").contains("version must match vX.Y.Z"));
    }

    #[test]
    fn deploy_sdk_version_marker_missing_is_omitted() {
        let _guard = cwd_lock().lock().expect("cwd lock");
        let dir = temp_dir("ling-sdk-version-test").expect("temp dir");
        let old = env::current_dir().expect("cwd");
        env::set_current_dir(&dir).expect("set cwd");
        let sdk_version = read_agent_sdk_version_from_marker();
        env::set_current_dir(old).expect("restore cwd");

        assert_eq!(sdk_version, None);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn deploy_reports_missing_bundle() {
        let dir = temp_dir("ling-deploy-test").expect("temp dir");
        let err = validate_deploy_bundle(&dir.join("missing.js"))
            .expect_err("missing bundle should fail");

        assert!(format!("{err:?}").contains("bundle not found"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn deploy_api_key_precedence_matches_public_interface() {
        let key = choose_deploy_api_key(
            Some("Bearer flag-key"),
            Some("ling-env-key".to_string()),
            Some("config-key".to_string()),
            Some("legacy-key".to_string()),
        )
        .expect("flag key");
        assert_eq!(key, "flag-key");

        let key = choose_deploy_api_key(
            None,
            Some("ling-env-key".to_string()),
            Some("config-key".to_string()),
            Some("legacy-key".to_string()),
        )
        .expect("ling env key");
        assert_eq!(key, "ling-env-key");

        let key = choose_deploy_api_key(
            None,
            None,
            Some("config-key".to_string()),
            Some("legacy-key".to_string()),
        )
        .expect("config key");
        assert_eq!(key, "config-key");

        let key = choose_deploy_api_key(None, None, None, Some("legacy-key".to_string()))
            .expect("legacy key");
        assert_eq!(key, "legacy-key");
    }

    #[test]
    fn framework_sdk_api_key_uses_env_then_saved_then_legacy() {
        let _guard = cwd_lock().lock().expect("env lock");
        let old_ling = env::var("LING_API_KEY").ok();
        let old_legacy = env::var("LISTENAI_API_KEY").ok();

        env::remove_var("LING_API_KEY");
        env::remove_var("LISTENAI_API_KEY");
        let ctx = AgentContext {
            api_base_url: "https://api.listenai.com".to_string(),
            saved_api_key: Some("Bearer saved-key".to_string()),
        };
        assert_eq!(framework_sdk_api_key(&ctx).as_deref(), Some("saved-key"));

        env::set_var("LING_API_KEY", "Bearer env-key");
        assert_eq!(framework_sdk_api_key(&ctx).as_deref(), Some("env-key"));

        env::remove_var("LING_API_KEY");
        env::set_var("LISTENAI_API_KEY", "legacy-key");
        let ctx = AgentContext {
            api_base_url: "https://api.listenai.com".to_string(),
            saved_api_key: None,
        };
        assert_eq!(framework_sdk_api_key(&ctx).as_deref(), Some("legacy-key"));

        restore_env("LING_API_KEY", old_ling);
        restore_env("LISTENAI_API_KEY", old_legacy);
    }

    #[test]
    fn windows_executable_candidates_include_cmd_shims() {
        let candidates = windows_executable_candidates("esbuild")
            .into_iter()
            .map(|value| value.to_string_lossy().to_string())
            .collect::<Vec<_>>();

        assert_eq!(
            candidates,
            vec!["esbuild.cmd", "esbuild.exe", "esbuild.bat", "esbuild"]
        );
    }

    #[test]
    fn deploy_api_key_rejects_empty_candidates() {
        let err = choose_deploy_api_key(
            Some("  "),
            Some("\t".to_string()),
            Some("".to_string()),
            None,
        )
        .expect_err("empty keys should fail");

        assert!(format!("{err:?}").contains("API key not set"));
    }

    fn fixture_sdk_archive() -> Vec<u8> {
        let encoder = GzEncoder::new(Vec::new(), Compression::default());
        let mut builder = tar::Builder::new(encoder);
        append_archive_file(
            &mut builder,
            "flows-voice-chat/agent.ts",
            "import { register } from \"@listenai/agent-sdk\";\nregister({ onMessage() {} });\n",
        );
        append_archive_file(
            &mut builder,
            "flows-voice-chat/listenai.toml",
            "[project]\nname = \"flows-voice-chat\"\n",
        );
        append_archive_file(
            &mut builder,
            "flows-voice-chat/package.json",
            r#"{"name":"flows-voice-chat","scripts":{"build":"listenai build","dev":"listenai dev"},"dependencies":{"@listenai/agent-sdk":"file:./sdk"},"devDependencies":{"typescript":"^5.3.0"}}"#,
        );
        append_archive_file(
            &mut builder,
            "flows-voice-chat/sdk/package.json",
            r#"{"name":"@listenai/agent-sdk","version":"0.1.0-mvp.0"}"#,
        );
        append_archive_file(
            &mut builder,
            "flows-voice-chat/sdk/src/index.ts",
            "export const sdk = true;\n",
        );

        let encoder = builder.into_inner().expect("finish tar");
        encoder.finish().expect("finish gzip")
    }

    fn append_archive_file(
        builder: &mut tar::Builder<GzEncoder<Vec<u8>>>,
        path: &str,
        content: &str,
    ) {
        let bytes = content.as_bytes();
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, path, bytes)
            .expect("append archive file");
    }

    fn restore_env(key: &str, value: Option<String>) {
        match value {
            Some(value) => env::set_var(key, value),
            None => env::remove_var(key),
        }
    }

    fn cwd_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }
}
