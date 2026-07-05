mod api_key;
mod config;
mod secret_prompt;
mod terminal;
mod v1_api;

use anyhow::{anyhow, Context, Result};
use clap::{Args, Parser, Subcommand};
use ling_plugin_app::config_view;
use ling_plugin_app::request::{RequestEvent, RequestInput, RequestOptions};
use serde_json::Value;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Debug, Parser)]
#[command(name = "ling", version, about = "ListenAI local CLI")]
struct Cli {
    #[arg(
        long,
        env = "LING_API_BASE_URL",
        default_value = ling_core::DEFAULT_API_BASE_URL
    )]
    api_base_url: String,

    #[arg(
        long,
        env = "LING_PLATFORM_BASE_URL",
        default_value = ling_core::DEFAULT_PLATFORM_BASE_URL
    )]
    platform_base_url: String,

    #[arg(
        long,
        env = "LING_DOCS_GRAPHQL_URL",
        default_value = "https://docs2.listenai.com/graphql"
    )]
    docs_graphql_url: String,

    #[arg(
        long,
        env = "LING_DOCS_BASE_URL",
        default_value = "https://docs2.listenai.com"
    )]
    docs_base_url: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Login with an API Key from platform.listenai.com/keys.
    Login(LoginArgs),
    /// Show the current API account.
    Account {
        /// Print the raw JSON response.
        #[arg(long)]
        json: bool,
    },
    /// Basic AI abilities: models, chat, TTS, ASR, wakeword.
    Ai(AiArgs),
    /// Platform app management and local agent project workflow.
    App(AppArgs),
    /// Account-level knowledge base management.
    Kb(KbArgs),
    /// Search ListenAI documentation center.
    Wiki(WikiArgs),
}

#[derive(Debug, Args)]
struct LoginArgs {
    /// API Key from platform.listenai.com/keys. If omitted, ling prompts for it.
    #[arg(long = "api-key", env = "LING_API_KEY")]
    api_key: Option<String>,
}

// ---------------------------------------------------------------------------
// ling ai
// ---------------------------------------------------------------------------

#[derive(Debug, Args)]
struct AiArgs {
    #[command(subcommand)]
    command: AiCommand,
}

#[derive(Debug, Subcommand)]
enum AiCommand {
    /// List available v1 models.
    Models {
        /// Print the raw JSON response.
        #[arg(long)]
        json: bool,
    },
    /// Send a prompt to /v1/chat/completions.
    Chat(ChatArgs),
    /// Synthesize speech, print the audio URL (wss /v1/tts/stream).
    Tts(TtsArgs),
    /// Recognize speech from a PCM/WAV file (wss /v1/asr).
    Asr(AsrArgs),
    /// Generate a wakeword resource (ROMFS bin).
    Wakeword(WakewordArgs),
}

#[derive(Debug, Args)]
struct ChatArgs {
    /// User prompt. Multiple words are joined with spaces.
    #[arg(required = true)]
    prompt: Vec<String>,
    /// Chat model id.
    #[arg(long, default_value = "doubao-seed-1.6-flash")]
    model: String,
    /// Optional system prompt.
    #[arg(long)]
    system: Option<String>,
    /// Stream assistant text to stdout.
    #[arg(long, conflicts_with = "json")]
    stream: bool,
    /// Print the raw JSON response.
    #[arg(long)]
    json: bool,
    /// Sampling temperature.
    #[arg(long)]
    temperature: Option<f32>,
    /// Nucleus sampling top_p.
    #[arg(long = "top-p")]
    top_p: Option<f32>,
    /// Maximum output tokens.
    #[arg(long = "max-tokens")]
    max_tokens: Option<u32>,
}

#[derive(Debug, Args)]
struct TtsArgs {
    /// Text to synthesize. Multiple words are joined with spaces.
    #[arg(required_unless_present = "list_vcn")]
    text: Vec<String>,
    /// Voice (VCN), e.g. x5_lingyuzhao_flow. Use --list-vcn to browse.
    #[arg(long)]
    vcn: Option<String>,
    /// Audio format.
    #[arg(long, value_parser = ["mp3", "pcm"])]
    format: Option<String>,
    /// Sample rate in Hz.
    #[arg(long = "sample-rate", value_parser = ["8000", "16000", "24000"])]
    sample_rate: Option<String>,
    /// Speed, 1-100 (default 50).
    #[arg(long)]
    speed: Option<u32>,
    /// Volume, 1-100 (default 50).
    #[arg(long)]
    volume: Option<u32>,
    /// Pitch, 1-100 (default 50).
    #[arg(long)]
    pitch: Option<u32>,
    /// smartTTS emotion (e.g. cheerful, sad, auto).
    #[arg(long)]
    emotion: Option<String>,
    /// smartTTS emotion scale, -20..=20.
    #[arg(long = "emotion-scale")]
    emotion_scale: Option<i32>,
    /// smartTTS style.
    #[arg(long)]
    style: Option<String>,
    /// Also download the audio into a file.
    #[arg(short = 'o', long)]
    output: Option<PathBuf>,
    /// List supported voices (VCN) instead of synthesizing.
    #[arg(long = "list-vcn")]
    list_vcn: bool,
    /// Print the result as JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct AsrArgs {
    /// Audio file: raw PCM (16k 16bit LE mono) or WAV in the same format.
    file: PathBuf,
    /// VAD trailing-silence endpoint in ms.
    #[arg(long = "vad-eos")]
    vad_eos: Option<u32>,
    /// ASR engine (ent), e.g. home-va.
    #[arg(long)]
    ent: Option<String>,
    /// Enable LLM-based VAD.
    #[arg(long = "asr-vad")]
    asr_vad: bool,
    /// Print the result as JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct WakewordArgs {
    /// Wakeword text, e.g. 小聆小聆.
    word: String,
    /// Sensitivity.
    #[arg(long, value_parser = ["low", "middle", "high"], default_value = "middle")]
    sensitive: String,
    /// Output file for the generated ROMFS bin resource.
    #[arg(short = 'o', long, required = true)]
    output: PathBuf,
}

// ---------------------------------------------------------------------------
// ling app
// ---------------------------------------------------------------------------

#[derive(Debug, Args)]
struct AppArgs {
    /// Target app (product id). Defaults to product_id in listenai.toml.
    /// Can be placed right after `app` or after the action.
    #[arg(long = "product-id", global = true)]
    product_id: Option<String>,
    #[command(subcommand)]
    command: AppCommand,
}

#[derive(Debug, Subcommand)]
enum AppCommand {
    /// List platform apps.
    List {
        #[arg(long, default_value_t = 1)]
        page: u32,
        #[arg(long = "page-size", default_value_t = 20)]
        page_size: u32,
        #[arg(long = "service-type", value_parser = ["device", "api"])]
        service_type: Option<String>,
        /// Print the raw JSON response.
        #[arg(long)]
        json: bool,
    },
    /// Create a platform app.
    Create {
        /// App name.
        name: String,
        /// Access mode: managed (official pipeline) or custom.
        #[arg(long, value_parser = ["managed", "custom"], default_value = "managed")]
        mode: String,
    },
    /// Scaffold a local agent project and link it to a platform app.
    Init(InitArgs),
    /// Bundle the agent project to a single JS file.
    Build(ling_plugin_app_project::BuildArgs),
    /// Run the agent locally with hot reload and a mock device REPL.
    Dev,
    /// Preview or upload an agent bundle to the platform.
    Deploy(ling_plugin_app_project::DeployArgs),
    /// Inspect an app. Product id defaults to listenai.toml.
    Inspect {
        /// Positional product id (same as --product-id).
        #[arg(value_name = "product_id", id = "product_id_pos")]
        positional_product_id: Option<String>,
        /// Print the raw JSON response.
        #[arg(long)]
        json: bool,
    },
    /// Send a simulated request through the cloud link and print all frames.
    Request(RequestArgs),
    /// Look up an existing request record by SID.
    Trace {
        /// SID printed by `ling app request` or found in link frames.
        sid: String,
        /// How many hours back to search (default 24).
        #[arg(long, default_value_t = 24)]
        hours: u32,
        /// Show the full request context and tool details.
        #[arg(long)]
        full: bool,
        /// Print the raw matching records as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Device management.
    Device(DeviceArgs),
    /// Firmware OTA management.
    Ota(OtaArgs),
    /// Role management.
    Role(RoleArgs),
    /// Get or set the default wake interaction mode.
    #[command(name = "interact-mode")]
    InteractMode {
        /// Target mode. Omit to show the current mode.
        #[arg(value_parser = ["oneshot", "half-duplex", "full-duplex"])]
        mode: Option<String>,
    },
    /// App-linked knowledge bases.
    Kb(AppKbArgs),
    /// Domain lexicon (hotwords) management.
    Lexicon(LexiconArgs),
    /// Device prompt tone texts.
    Tone(ToneArgs),
    /// MCP server configuration.
    Mcp(McpArgs),
}

#[derive(Debug, Args)]
struct InitArgs {
    #[command(flatten)]
    create: ling_plugin_app_project::CreateArgs,
}

#[derive(Debug, Args)]
struct RequestArgs {
    /// Send a text utterance.
    #[arg(long, conflicts_with = "file", required_unless_present = "file")]
    text: Option<String>,
    /// Send an audio file (raw PCM or 16k 16bit LE mono WAV).
    #[arg(long)]
    file: Option<PathBuf>,
    /// Device id (auth_id) used for the interaction.
    #[arg(long = "device-id", default_value = "ling-cli")]
    device_id: String,
    /// App id for multi-app products (llm_app).
    #[arg(long = "llm-app")]
    llm_app: Option<String>,
}

#[derive(Debug, Args)]
struct DeviceArgs {
    #[command(subcommand)]
    command: DeviceCommand,
}

#[derive(Debug, Subcommand)]
enum DeviceCommand {
    /// Show device quota (total / used / whitelist enforcement).
    Quota {
        /// Print the raw JSON response.
        #[arg(long)]
        json: bool,
    },
    /// List imported device ids.
    List,
    /// Import a device id.
    Add { device_id: String },
    /// Check whether a device id is authorized.
    Query { device_id: String },
    /// Show or toggle whitelist enforcement.
    Enforce {
        #[arg(value_parser = ["on", "off"])]
        state: Option<String>,
    },
}

#[derive(Debug, Args)]
struct OtaArgs {
    #[command(subcommand)]
    command: OtaCommand,
}

#[derive(Debug, Subcommand)]
enum OtaCommand {
    /// List OTA packages.
    List,
    /// Upload an OTA package.
    Upload { file: PathBuf },
    /// Show an OTA package.
    Get { package_id: String },
    /// Edit an OTA package.
    Edit { package_id: String },
    /// Publish an OTA package.
    Publish { package_id: String },
    /// Delete an OTA package.
    Delete { package_id: String },
    /// Manage the OTA test whitelist.
    Whitelist {
        #[command(subcommand)]
        command: OtaWhitelistCommand,
    },
}

#[derive(Debug, Subcommand)]
enum OtaWhitelistCommand {
    List,
    Add { device_id: String },
    Delete { device_id: String },
}

#[derive(Debug, Args)]
struct RoleArgs {
    #[command(subcommand)]
    command: RoleCommand,
}

#[derive(Debug, Subcommand)]
enum RoleCommand {
    /// List roles of the app.
    List {
        /// Print the raw JSON response.
        #[arg(long)]
        json: bool,
    },
    /// Add a role.
    Add { name: String },
    /// Edit a role.
    Edit { role_id: String },
    /// Delete a role.
    Delete { role_id: String },
    /// Set the default role.
    #[command(name = "set-default")]
    SetDefault { role_id: String },
}

#[derive(Debug, Args)]
struct AppKbArgs {
    #[command(subcommand)]
    command: AppKbCommand,
}

#[derive(Debug, Subcommand)]
enum AppKbCommand {
    /// List knowledge bases linked to the app.
    List {
        /// Print the raw JSON response.
        #[arg(long)]
        json: bool,
    },
    /// Link a knowledge base to the app.
    Link { index_id: String },
    /// Unlink a knowledge base from the app.
    Unlink { index_id: String },
}

#[derive(Debug, Args)]
struct LexiconArgs {
    #[command(subcommand)]
    command: LexiconCommand,
}

#[derive(Debug, Subcommand)]
enum LexiconCommand {
    /// List domain lexicon entries.
    List {
        /// Print the raw JSON response.
        #[arg(long)]
        json: bool,
    },
    /// Add a lexicon entry.
    Add { word: String },
    /// Edit a lexicon entry.
    Edit { word: String },
    /// Delete a lexicon entry.
    Delete { word: String },
}

#[derive(Debug, Args)]
struct ToneArgs {
    #[command(subcommand)]
    command: ToneCommand,
}

#[derive(Debug, Subcommand)]
enum ToneCommand {
    /// Show the prompt tone table.
    Show {
        /// Print the raw JSON response.
        #[arg(long)]
        json: bool,
    },
    /// Edit prompt tones: --set key="text" [--set ...] [--reset key ...].
    Edit {
        #[arg(long = "set", value_name = "key=text")]
        set: Vec<String>,
        #[arg(long = "reset", value_name = "key")]
        reset: Vec<String>,
    },
}

#[derive(Debug, Args)]
struct McpArgs {
    #[command(subcommand)]
    command: McpCommand,
}

#[derive(Debug, Subcommand)]
enum McpCommand {
    /// List MCP servers.
    List,
    /// Add an MCP server.
    Add { name: String },
    /// Edit an MCP server.
    Edit { server_id: String },
    /// Delete an MCP server.
    Delete { server_id: String },
    /// Enable an MCP server.
    Enable { server_id: String },
    /// Disable an MCP server.
    Disable { server_id: String },
}

// ---------------------------------------------------------------------------
// ling kb
// ---------------------------------------------------------------------------

#[derive(Debug, Args)]
struct KbArgs {
    #[command(subcommand)]
    command: KbCommand,
}

#[derive(Debug, Subcommand)]
enum KbCommand {
    /// List knowledge bases.
    List {
        #[arg(long, default_value_t = 1)]
        page: u32,
        #[arg(long, default_value_t = 20)]
        size: u32,
        /// Print the raw JSON response.
        #[arg(long)]
        json: bool,
    },
    /// Create a knowledge base.
    Create {
        name: String,
        /// Print the raw JSON response.
        #[arg(long)]
        json: bool,
    },
    /// Delete a knowledge base.
    Delete {
        index_id: String,
        /// Skip the confirmation prompt.
        #[arg(long)]
        yes: bool,
    },
    /// Manage documents inside a knowledge base.
    Doc {
        index_id: String,
        #[command(subcommand)]
        command: KbDocCommand,
    },
    /// Retrieve knowledge points by text query.
    Query {
        index_id: String,
        #[arg(required = true)]
        text: Vec<String>,
        #[arg(long)]
        limit: Option<u32>,
        #[arg(long)]
        threshold: Option<f32>,
        /// Print the raw JSON response.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum KbDocCommand {
    /// List documents.
    List {
        #[arg(long, default_value_t = 1)]
        page: u32,
        #[arg(long, default_value_t = 20)]
        size: u32,
        /// Print the raw JSON response.
        #[arg(long)]
        json: bool,
    },
    /// Add a document from a fetchable URL.
    Add {
        /// Document name, e.g. 说明书.txt.
        #[arg(long)]
        name: String,
        /// Document URL the platform can fetch.
        #[arg(long)]
        url: String,
    },
    /// Delete documents by id.
    Delete {
        #[arg(required = true)]
        doc_ids: Vec<String>,
    },
}

// ---------------------------------------------------------------------------
// ling wiki
// ---------------------------------------------------------------------------

#[derive(Debug, Args)]
struct WikiArgs {
    #[command(subcommand)]
    command: WikiCommand,
}

#[derive(Debug, Subcommand)]
enum WikiCommand {
    /// Search docs2 by one or more independent keywords.
    Search {
        /// Print JSON output.
        #[arg(long)]
        json: bool,
        keywords: Vec<String>,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    let _terminal_encoding = terminal::init();
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => {
            let code = err.exit_code();
            if err.print().is_err() {
                eprintln!("Error: failed to print command-line error");
            }
            return exit_code(code);
        }
    };

    match run(cli).await {
        Ok(code) => code,
        Err(err) => {
            eprintln!("Error: {err:?}");
            ExitCode::FAILURE
        }
    }
}

/// 各 handler 共用的运行时上下文（从 Cli 顶层参数解构而来）。
struct Ctx {
    api_base_url: String,
    platform_base_url: String,
}

async fn run(cli: Cli) -> Result<ExitCode> {
    let Cli {
        api_base_url,
        platform_base_url,
        docs_graphql_url,
        docs_base_url,
        command,
    } = cli;
    let ctx = Ctx {
        api_base_url,
        platform_base_url,
    };

    match command {
        Command::Login(args) => {
            login(ctx.api_base_url, args).await?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Account { json } => {
            account_command(ctx.api_base_url, json).await?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Ai(args) => ai_command(&ctx, args).await,
        Command::App(args) => app_command(&ctx, args).await,
        Command::Kb(args) => {
            kb_command(&ctx.api_base_url, args).await?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Wiki(args) => {
            wiki_command(docs_graphql_url, docs_base_url, args).await?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

pub(crate) fn exit_code(code: i32) -> ExitCode {
    if code == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(code.clamp(1, u8::MAX as i32) as u8)
    }
}

async fn login(api_base_url: String, args: LoginArgs) -> Result<()> {
    let api_key = match args.api_key {
        Some(api_key) => api_key,
        None => secret_prompt::prompt_api_key()?,
    };

    let output = api_key::login_with_api_key(&api_base_url, &api_key).await?;

    let mut cfg = config::LingConfig::load()?;
    cfg.api_key = Some(api_key::strip_bearer(&api_key));
    cfg.save()?;

    print_json(&output)
}

async fn account_command(api_base_url: String, json: bool) -> Result<()> {
    let api_key = resolve_api_key()?;
    let output = v1_api::account(&api_base_url, &api_key).await?;
    if json {
        print_json(&output)
    } else {
        println!("{}", v1_api::render_account(&output)?);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// ling ai handlers
// ---------------------------------------------------------------------------

async fn ai_command(cli: &Ctx, args: AiArgs) -> Result<ExitCode> {
    match args.command {
        AiCommand::Models { json } => {
            let api_key = resolve_api_key()?;
            let output = v1_api::models(&cli.api_base_url, &api_key).await?;
            if json {
                print_json(&output)?;
            } else {
                println!("{}", v1_api::render_models(&output)?);
            }
        }
        AiCommand::Chat(args) => chat_command(&cli.api_base_url, args).await?,
        AiCommand::Tts(args) => tts_command(cli, args).await?,
        AiCommand::Asr(args) => asr_command(&cli.api_base_url, args).await?,
        AiCommand::Wakeword(_) => {
            return Err(platform_write_unavailable("唤醒词资源生成"));
        }
    }
    Ok(ExitCode::SUCCESS)
}

async fn chat_command(api_base_url: &str, args: ChatArgs) -> Result<()> {
    let api_key = resolve_api_key()?;
    let request = v1_api::ChatRequest {
        model: args.model,
        prompt: args.prompt.join(" "),
        system: args.system,
        stream: args.stream,
        temperature: args.temperature,
        top_p: args.top_p,
        max_tokens: args.max_tokens,
    };

    if request.stream {
        v1_api::chat_completion_stream(api_base_url, &api_key, &request).await
    } else {
        let output = v1_api::chat_completion(api_base_url, &api_key, &request).await?;
        if args.json {
            print_json(&output)
        } else {
            println!("{}", v1_api::render_chat_completion(&output)?);
            Ok(())
        }
    }
}

async fn tts_command(cli: &Ctx, args: TtsArgs) -> Result<()> {
    let api_key = resolve_api_key()?;

    if args.list_vcn {
        let output = ling_plugin_ai::list_vcns(&cli.platform_base_url, &api_key).await?;
        if args.json {
            return print_json(&output);
        }
        println!("{}", ling_plugin_ai::render_vcns(&output)?);
        return Ok(());
    }

    let opts = ling_plugin_ai::TtsOptions {
        vcn: args.vcn,
        format: args.format,
        sample_rate: args
            .sample_rate
            .as_deref()
            .and_then(|rate| rate.parse().ok()),
        speed: args.speed,
        volume: args.volume,
        pitch: args.pitch,
        emotion: args.emotion,
        emotion_scale: args.emotion_scale,
        style: args.style,
    };
    let text = args.text.join(" ");
    let outcome = ling_plugin_ai::tts(
        &cli.api_base_url,
        &api_key,
        &text,
        &opts,
        args.output.as_deref(),
    )
    .await?;

    if args.json {
        let mut value = serde_json::json!({"url": outcome.url});
        if let Some((path, bytes)) = &outcome.saved {
            value["output"] = serde_json::json!({"path": path, "bytes": bytes});
        }
        return print_json(&value);
    }
    println!("{}", outcome.url);
    if let Some((path, bytes)) = &outcome.saved {
        eprintln!("已保存音频：{path}（{bytes} 字节）");
    } else {
        eprintln!("提示：音频 URL 有限时效，请及时拉取；使用 -o <file> 可直接保存到文件。");
    }
    Ok(())
}

async fn asr_command(api_base_url: &str, args: AsrArgs) -> Result<()> {
    let api_key = resolve_api_key()?;
    let audio = ling_plugin_ai::load_pcm_audio(&args.file)?;
    if audio.is_empty() {
        anyhow::bail!("音频文件为空：{}", args.file.display());
    }
    let opts = ling_plugin_ai::AsrOptions {
        vad_eos: args.vad_eos,
        ent: args.ent,
        asr_vad: args.asr_vad,
    };
    let show_partial = io::stderr().is_terminal();
    let text = ling_plugin_ai::asr(api_base_url, &api_key, &audio, &opts, |partial| {
        if show_partial {
            eprintln!("… {partial}");
        }
    })
    .await?;

    if args.json {
        print_json(&serde_json::json!({"text": text}))
    } else {
        println!("{text}");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// ling app handlers
// ---------------------------------------------------------------------------

async fn app_command(cli: &Ctx, args: AppArgs) -> Result<ExitCode> {
    let product = args.product_id;
    match args.command {
        AppCommand::List {
            page,
            page_size,
            service_type,
            json,
        } => {
            let api_key = resolve_api_key()?;
            let output = ling_plugin_app::list_projects(
                &cli.api_base_url,
                &api_key,
                page,
                page_size,
                service_type.as_deref(),
            )
            .await?;
            if json {
                print_json(&output)?;
            } else {
                println!("{}", ling_plugin_app::render_project_list(&output)?);
            }
        }
        AppCommand::Create { .. } => {
            return Err(platform_write_unavailable("创建平台应用"));
        }
        AppCommand::Init(args) => return init_command(cli, args, product).await,
        AppCommand::Build(args) => {
            let ctx = agent_context(cli)?;
            return ling_plugin_app_project::build_command(&ctx, args).await;
        }
        AppCommand::Dev => {
            let ctx = agent_context(cli)?;
            return ling_plugin_app_project::dev_command(&ctx).await;
        }
        AppCommand::Deploy(mut args) => {
            args.product_id = product;
            let saved_api_key = if args.dry_run {
                None
            } else {
                config::LingConfig::load()?.api_key
            };
            let ctx = ling_plugin_app_project::AgentContext {
                api_base_url: cli.api_base_url.clone(),
                saved_api_key,
            };
            return ling_plugin_app_project::deploy_command(&ctx, args).await;
        }
        AppCommand::Inspect {
            positional_product_id,
            json,
        } => {
            let api_key = resolve_api_key()?;
            let product_id = resolve_product_id(positional_product_id.or(product))?;
            let output =
                ling_plugin_app::inspect_product(&cli.api_base_url, &api_key, &product_id).await?;
            if json {
                print_json(&output)?;
            } else {
                println!("{}", ling_plugin_app::render_project_inspect(&output)?);
            }
        }
        AppCommand::Request(args) => request_command(cli, args, product).await?,
        AppCommand::Trace {
            sid,
            hours,
            full,
            json,
        } => trace_command(cli, &sid, hours, full, json).await?,
        AppCommand::Device(args) => device_command(cli, args, product).await?,
        AppCommand::Ota(args) => {
            let feature = match args.command {
                OtaCommand::Whitelist { .. } => "OTA 测试白名单管理",
                _ => "OTA 固件管理",
            };
            return Err(platform_write_unavailable(feature));
        }
        AppCommand::Role(args) => role_command(cli, args, product).await?,
        AppCommand::InteractMode { mode } => match mode {
            None => {
                let detail = fetch_project_detail(cli, product).await?;
                println!(
                    "{}",
                    ling_plugin_app::config_view::render_interact_mode(&detail)?
                );
            }
            Some(_) => return Err(platform_write_unavailable("设置交互模式")),
        },
        AppCommand::Kb(args) => match args.command {
            AppKbCommand::List { json } => {
                let detail = fetch_project_detail(cli, product).await?;
                if json {
                    print_json(&config_fragment(&detail, "/llm_feature/knowledge"))?;
                } else {
                    println!("{}", config_view::render_app_kb_list(&detail)?);
                }
            }
            AppKbCommand::Link { .. } | AppKbCommand::Unlink { .. } => {
                return Err(platform_write_unavailable("应用知识库关联"));
            }
        },
        AppCommand::Lexicon(args) => match args.command {
            LexiconCommand::List { json } => {
                let detail = fetch_project_detail(cli, product).await?;
                if json {
                    print_json(&config_fragment(&detail, "/llm_feature/hotwords"))?;
                } else {
                    println!("{}", config_view::render_lexicon_list(&detail)?);
                }
            }
            _ => return Err(platform_write_unavailable("专业词汇管理")),
        },
        AppCommand::Tone(args) => match args.command {
            ToneCommand::Show { json } => {
                let detail = fetch_project_detail(cli, product).await?;
                if json {
                    print_json(&config_fragment(&detail, "/prompt_tone_texts"))?;
                } else {
                    println!("{}", config_view::render_tone_show(&detail)?);
                }
            }
            ToneCommand::Edit { .. } => {
                return Err(platform_write_unavailable("提示语编辑"));
            }
        },
        AppCommand::Mcp(_) => {
            return Err(platform_write_unavailable("MCP 服务器配置"));
        }
    }
    Ok(ExitCode::SUCCESS)
}

async fn init_command(cli: &Ctx, args: InitArgs, product: Option<String>) -> Result<ExitCode> {
    let ctx = agent_context(cli)?;
    let project_dir = ling_plugin_app_project::init_project(&ctx, &args.create).await?;

    let product_id = match product {
        Some(product_id) => Some(product_id),
        None => select_product_interactively(cli).await?,
    };

    match product_id {
        Some(product_id) => {
            ling_plugin_app_project::project::write_product_id(&project_dir, &product_id)?;
            println!(
                "已关联应用 product_id={product_id}（写入 {}）",
                ling_plugin_app_project::project::manifest_path(&project_dir).display()
            );
        }
        None => {
            println!(
                "未关联平台应用。可稍后在 {} 中设置 product_id，或重新运行 `ling app init`。",
                ling_plugin_app_project::project::manifest_path(&project_dir).display()
            );
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// 交互式选择一个平台应用；非交互环境或无 API Key 时返回 None。
async fn select_product_interactively(cli: &Ctx) -> Result<Option<String>> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        eprintln!("非交互环境，跳过应用关联（可用 --product-id 指定）。");
        return Ok(None);
    }
    let Some(api_key) = resolve_optional_api_key()? else {
        eprintln!("未找到 API Key，跳过应用关联（先 `ling login` 或用 --product-id 指定）。");
        return Ok(None);
    };

    let output = ling_plugin_app::list_projects(&cli.api_base_url, &api_key, 1, 50, None).await?;
    let projects = output
        .get("data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if projects.is_empty() {
        eprintln!("平台上暂无应用，跳过关联。");
        return Ok(None);
    }

    println!("选择要关联的平台应用：");
    for (index, project) in projects.iter().enumerate() {
        println!(
            "  {}. {} ({})",
            index + 1,
            project.get("name").and_then(Value::as_str).unwrap_or("-"),
            project
                .get("product_id")
                .and_then(Value::as_str)
                .unwrap_or("-")
        );
    }
    print!("输入编号（回车跳过）: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim();
    if input.is_empty() {
        return Ok(None);
    }
    let index: usize = input.parse().context("请输入有效的编号")?;
    let project = projects
        .get(index.checked_sub(1).context("编号从 1 开始")?)
        .context("编号超出范围")?;
    Ok(project
        .get("product_id")
        .and_then(Value::as_str)
        .map(str::to_owned))
}

async fn request_command(cli: &Ctx, args: RequestArgs, product: Option<String>) -> Result<()> {
    let product_id = resolve_product_id(product)?;
    let detail = fetch_project_detail_by_id(cli, &product_id).await?;
    let secret = config_view::product_secret(&detail)
        .context("无法从应用详情获取云云对接密钥（product secret）")?;

    let input = if let Some(text) = args.text {
        RequestInput::Text(text)
    } else {
        let file = args.file.expect("clap 保证 text/file 至少一个");
        RequestInput::Audio(ling_plugin_ai::load_pcm_audio(&file)?)
    };
    let opts = RequestOptions {
        device_id: args.device_id,
        llm_app: args.llm_app,
    };

    let mut sid: Option<String> = None;
    ling_plugin_app::request::interaction_request(
        &cli.api_base_url,
        &product_id,
        &secret,
        &input,
        &opts,
        |event| match event {
            RequestEvent::Frame(frame) => {
                if sid.is_none() {
                    sid = serde_json::from_str::<Value>(&frame)
                        .ok()
                        .and_then(|value| {
                            value
                                .get("sid")
                                .and_then(Value::as_str)
                                .filter(|sid| !sid.is_empty())
                                .map(str::to_owned)
                        });
                }
                println!("{frame}");
            }
            RequestEvent::Binary(bytes) => eprintln!("[binary] {bytes} bytes"),
        },
    )
    .await?;

    if let Some(sid) = sid {
        eprintln!("sid: {sid}");
        eprintln!("可用 `ling app trace {sid}` 查询该请求的记录。");
    }
    Ok(())
}

async fn trace_command(cli: &Ctx, sid: &str, hours: u32, full: bool, json: bool) -> Result<()> {
    let api_key = resolve_api_key()?;
    let outcome =
        ling_plugin_app::records::find_by_sid(&cli.api_base_url, &api_key, sid, hours).await?;
    // SID 是请求唯一标识：要么恰好一条，要么未命中（报错退出，便于脚本判断）
    let Some(record) = outcome.record else {
        anyhow::bail!(ling_plugin_app::records::miss_message(
            sid,
            hours,
            outcome.truncated
        ));
    };
    if json {
        print_json(&record)
    } else {
        println!("{}", ling_plugin_app::records::render_record(&record, full));
        Ok(())
    }
}

async fn device_command(cli: &Ctx, args: DeviceArgs, product: Option<String>) -> Result<()> {
    match args.command {
        DeviceCommand::Quota { json } => {
            let detail = fetch_project_detail(cli, product).await?;
            if json {
                let product = config_view::project_data(&detail)
                    .get("product")
                    .cloned()
                    .unwrap_or(Value::Null);
                print_json(&serde_json::json!({
                    "assignedDeviceQuota": product.get("assignedDeviceQuota"),
                    "consumedDeviceQuota": product.get("consumedDeviceQuota"),
                    "deviceAuthCheck": product.get("deviceAuthCheck"),
                }))?;
            } else {
                println!("{}", config_view::render_device_quota(&detail)?);
            }
        }
        DeviceCommand::Query { device_id } => {
            let api_key = resolve_api_key()?;
            let product_id = resolve_product_id(product)?;
            let output =
                ling_plugin_app::device_query(&cli.api_base_url, &api_key, &product_id, &device_id)
                    .await?;
            let valid = output
                .get("is_valid")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if valid {
                println!("设备 {device_id} 已授权（is_valid=true）。");
            } else {
                println!("设备 {device_id} 未授权（is_valid=false）。");
            }
        }
        DeviceCommand::Enforce { state } => match state {
            None => {
                let detail = fetch_project_detail(cli, product).await?;
                match config_view::device_auth_check(&detail) {
                    Some(true) => println!("强制白名单模式：开启"),
                    Some(false) => println!("强制白名单模式：关闭"),
                    None => println!("强制白名单模式：未知"),
                }
            }
            Some(_) => return Err(platform_write_unavailable("切换强制白名单模式")),
        },
        DeviceCommand::List => {
            return Err(platform_write_unavailable("设备列表查询"));
        }
        DeviceCommand::Add { .. } => {
            return Err(platform_write_unavailable("导入设备"));
        }
    }
    Ok(())
}

async fn role_command(cli: &Ctx, args: RoleArgs, product: Option<String>) -> Result<()> {
    match args.command {
        RoleCommand::List { json } => {
            let detail = fetch_project_detail(cli, product).await?;
            if json {
                print_json(&config_fragment(&detail, "/llm_roles"))?;
            } else {
                println!("{}", config_view::render_role_list(&detail)?);
            }
            Ok(())
        }
        RoleCommand::Add { .. }
        | RoleCommand::Edit { .. }
        | RoleCommand::Delete { .. }
        | RoleCommand::SetDefault { .. } => Err(platform_write_unavailable("角色编辑")),
    }
}

// ---------------------------------------------------------------------------
// ling kb handlers
// ---------------------------------------------------------------------------

async fn kb_command(api_base_url: &str, args: KbArgs) -> Result<()> {
    let api_key = resolve_api_key()?;
    match args.command {
        KbCommand::List { page, size, json } => {
            let output = ling_plugin_kb::list(api_base_url, &api_key, page, size).await?;
            if json {
                print_json(&output)
            } else {
                println!("{}", ling_plugin_kb::render_list(&output)?);
                Ok(())
            }
        }
        KbCommand::Create { name, json } => {
            let output = ling_plugin_kb::create(api_base_url, &api_key, &name).await?;
            if json {
                print_json(&output)
            } else {
                let index_id = output
                    .pointer("/data/index_id")
                    .and_then(Value::as_str)
                    .unwrap_or("-");
                println!("已创建知识库「{name}」，index_id: {index_id}");
                Ok(())
            }
        }
        KbCommand::Delete { index_id, yes } => {
            if !yes && !confirm(&format!("确定删除知识库 {index_id} 吗？此操作不可恢复"))?
            {
                println!("已取消。");
                return Ok(());
            }
            ling_plugin_kb::delete(api_base_url, &api_key, &index_id).await?;
            println!("已删除知识库 {index_id}。");
            Ok(())
        }
        KbCommand::Doc { index_id, command } => match command {
            KbDocCommand::List { page, size, json } => {
                let output =
                    ling_plugin_kb::list_documents(api_base_url, &api_key, &index_id, page, size)
                        .await?;
                if json {
                    print_json(&output)
                } else {
                    println!("{}", ling_plugin_kb::render_documents(&output)?);
                    Ok(())
                }
            }
            KbDocCommand::Add { name, url } => {
                let output =
                    ling_plugin_kb::add_document(api_base_url, &api_key, &index_id, &name, &url)
                        .await?;
                print_json(&output)
            }
            KbDocCommand::Delete { doc_ids } => {
                ling_plugin_kb::delete_documents(api_base_url, &api_key, &index_id, &doc_ids)
                    .await?;
                println!("已删除 {} 个文档。", doc_ids.len());
                Ok(())
            }
        },
        KbCommand::Query {
            index_id,
            text,
            limit,
            threshold,
            json,
        } => {
            let content = text.join(" ");
            let output = ling_plugin_kb::query(
                api_base_url,
                &api_key,
                &index_id,
                &content,
                limit,
                threshold,
            )
            .await?;
            if json {
                print_json(&output)
            } else {
                println!("{}", ling_plugin_kb::render_query(&output)?);
                Ok(())
            }
        }
    }
}

async fn wiki_command(
    docs_graphql_url: String,
    docs_base_url: String,
    args: WikiArgs,
) -> Result<()> {
    match args.command {
        WikiCommand::Search { keywords, json } => {
            let keyword_count = keywords
                .iter()
                .filter(|keyword| !keyword.trim().is_empty())
                .count();
            if json {
                let output =
                    ling_plugin_wiki::search(&docs_graphql_url, &docs_base_url, &keywords).await?;
                print_json(&output)
            } else if keyword_count > 1 {
                let groups =
                    ling_plugin_wiki::search_grouped(&docs_graphql_url, &docs_base_url, &keywords)
                        .await?;
                println!("{}", ling_plugin_wiki::render_search_groups(&groups));
                Ok(())
            } else {
                let output =
                    ling_plugin_wiki::search(&docs_graphql_url, &docs_base_url, &keywords).await?;
                println!("{}", ling_plugin_wiki::render_search_results(&output));
                Ok(())
            }
        }
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn agent_context(cli: &Ctx) -> Result<ling_plugin_app_project::AgentContext> {
    Ok(ling_plugin_app_project::AgentContext {
        api_base_url: cli.api_base_url.clone(),
        saved_api_key: resolve_optional_api_key()?,
    })
}

async fn fetch_project_detail(cli: &Ctx, product_id: Option<String>) -> Result<Value> {
    let product_id = resolve_product_id(product_id)?;
    fetch_project_detail_by_id(cli, &product_id).await
}

async fn fetch_project_detail_by_id(cli: &Ctx, product_id: &str) -> Result<Value> {
    let api_key = resolve_api_key()?;
    ling_plugin_app::inspect_product(&cli.api_base_url, &api_key, product_id).await
}

/// 提取应用配置片段（相对 apps[0].config 的 JSON Pointer）。
fn config_fragment(detail: &Value, pointer: &str) -> Value {
    config_view::project_data(detail)
        .pointer(&format!("/apps/0/config{pointer}"))
        .cloned()
        .unwrap_or(Value::Null)
}

fn resolve_product_id(flag: Option<String>) -> Result<String> {
    let flag = flag
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    if let Some(product_id) = flag {
        return Ok(product_id);
    }
    std::env::current_dir()
        .ok()
        .and_then(|cwd| ling_plugin_app_project::project::read_product_id(&cwd))
        .ok_or_else(|| {
            anyhow!(
                "未指定应用：请传 --product-id，或在含 product_id 的 listenai.toml 项目目录内执行"
            )
        })
}

fn platform_write_unavailable(feature: &str) -> anyhow::Error {
    anyhow!(
        "「{feature}」的平台开放 API 尚未上线，请暂时在平台网页端操作：https://platform.listenai.com\n平台打通 API Key 授权链路后，ling 将在后续版本启用此命令。"
    )
}

fn confirm(message: &str) -> Result<bool> {
    if !io::stdin().is_terminal() {
        anyhow::bail!("非交互环境，请追加 --yes 确认执行");
    }
    print!("{message} [y/N]: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(matches!(
        input.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

fn print_json<T: serde::Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn resolve_api_key() -> Result<String> {
    resolve_optional_api_key()?
        .ok_or_else(|| anyhow::anyhow!("未找到 API Key，请先执行 `ling login` 或设置 LING_API_KEY"))
}

fn resolve_optional_api_key() -> Result<Option<String>> {
    if let Ok(api_key) = std::env::var("LING_API_KEY") {
        let api_key = api_key::strip_bearer(&api_key);
        if !api_key.is_empty() {
            return Ok(Some(api_key));
        }
    }

    let cfg = config::LingConfig::load()?;
    Ok(cfg
        .api_key
        .filter(|api_key| !api_key.trim().is_empty())
        .map(|api_key| api_key::strip_bearer(&api_key)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, Parser};

    #[test]
    fn parses_app_build_defaults() {
        let cli = Cli::try_parse_from(["ling", "app", "build"]).expect("parse app build");

        match cli.command {
            Command::App(app) => match app.command {
                AppCommand::Build(build) => {
                    assert_eq!(build.entry, "agent.ts");
                    assert_eq!(build.out, "dist/agent.js");
                    assert!(!build.release);
                }
                other => panic!("expected app build command, got {other:?}"),
            },
            other => panic!("expected app command, got {other:?}"),
        }
    }

    #[test]
    fn app_deploy_product_id_is_optional() {
        let cli =
            Cli::try_parse_from(["ling", "app", "deploy", "--version", "v1.0.0", "--dry-run"])
                .expect("parse app deploy");
        match cli.command {
            Command::App(app) => match app.command {
                AppCommand::Deploy(deploy) => assert!(deploy.product_id.is_none()),
                other => panic!("expected app deploy command, got {other:?}"),
            },
            other => panic!("expected app command, got {other:?}"),
        }
    }

    #[test]
    fn app_deploy_requires_version() {
        let err = Cli::try_parse_from(["ling", "app", "deploy", "--product-id", "prod_dev_local"])
            .expect_err("version should be required");
        assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn parses_app_init_defaults() {
        let cli = Cli::try_parse_from(["ling", "app", "init", "my-agent"]).expect("parse app init");

        match cli.command {
            Command::App(app) => match app.command {
                AppCommand::Init(init) => {
                    assert_eq!(init.create.name, "my-agent");
                    assert_eq!(init.create.template, "listenai");
                    assert!(!init.create.no_install);
                }
                other => panic!("expected app init command, got {other:?}"),
            },
            other => panic!("expected app command, got {other:?}"),
        }
    }

    #[test]
    fn app_product_id_works_before_and_after_action() {
        for argv in [
            vec!["ling", "app", "--product-id", "pid-1", "inspect"],
            vec!["ling", "app", "inspect", "--product-id", "pid-1"],
            vec!["ling", "app", "--product-id", "pid-1", "device", "quota"],
            vec!["ling", "app", "tone", "show", "--product-id", "pid-1"],
        ] {
            let cli = Cli::try_parse_from(argv.clone()).expect("parse app with product id");
            match cli.command {
                Command::App(app) => {
                    assert_eq!(app.product_id.as_deref(), Some("pid-1"), "argv: {argv:?}")
                }
                other => panic!("expected app command, got {other:?}"),
            }
        }
    }

    #[test]
    fn parses_ai_tts_options() {
        let cli = Cli::try_parse_from([
            "ling",
            "ai",
            "tts",
            "--vcn",
            "x5_lingyuzhao_flow",
            "--format",
            "pcm",
            "-o",
            "out.pcm",
            "你好",
            "世界",
        ])
        .expect("parse ai tts");
        match cli.command {
            Command::Ai(ai) => match ai.command {
                AiCommand::Tts(tts) => {
                    assert_eq!(tts.text, vec!["你好", "世界"]);
                    assert_eq!(tts.vcn.as_deref(), Some("x5_lingyuzhao_flow"));
                    assert_eq!(tts.format.as_deref(), Some("pcm"));
                    assert_eq!(tts.output.as_deref(), Some(std::path::Path::new("out.pcm")));
                }
                other => panic!("expected ai tts command, got {other:?}"),
            },
            other => panic!("expected ai command, got {other:?}"),
        }
    }

    #[test]
    fn ai_tts_list_vcn_needs_no_text() {
        let cli = Cli::try_parse_from(["ling", "ai", "tts", "--list-vcn"]).expect("parse");
        match cli.command {
            Command::Ai(ai) => match ai.command {
                AiCommand::Tts(tts) => assert!(tts.list_vcn),
                other => panic!("expected ai tts command, got {other:?}"),
            },
            other => panic!("expected ai command, got {other:?}"),
        }
    }

    #[test]
    fn ai_tts_requires_text_without_list_vcn() {
        let err = Cli::try_parse_from(["ling", "ai", "tts"]).expect_err("text required");
        assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn app_request_requires_text_or_file() {
        let err = Cli::try_parse_from(["ling", "app", "request"]).expect_err("input required");
        assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);

        let cli = Cli::try_parse_from(["ling", "app", "request", "--text", "你好"])
            .expect("parse request");
        match cli.command {
            Command::App(app) => match app.command {
                AppCommand::Request(request) => {
                    assert_eq!(request.text.as_deref(), Some("你好"));
                    assert_eq!(request.device_id, "ling-cli");
                }
                other => panic!("expected app request command, got {other:?}"),
            },
            other => panic!("expected app command, got {other:?}"),
        }
    }

    #[test]
    fn parses_interact_mode_values() {
        for mode in ["oneshot", "half-duplex", "full-duplex"] {
            let cli = Cli::try_parse_from(["ling", "app", "interact-mode", mode])
                .expect("parse interact-mode");
            match cli.command {
                Command::App(app) => match app.command {
                    AppCommand::InteractMode { mode: parsed, .. } => {
                        assert_eq!(parsed.as_deref(), Some(mode));
                    }
                    other => panic!("expected interact-mode command, got {other:?}"),
                },
                other => panic!("expected app command, got {other:?}"),
            }
        }
        assert!(Cli::try_parse_from(["ling", "app", "interact-mode", "bogus"]).is_err());
    }

    #[test]
    fn parses_kb_commands() {
        let cli = Cli::try_parse_from(["ling", "kb", "list"]).expect("parse kb list");
        assert!(matches!(
            cli.command,
            Command::Kb(KbArgs {
                command: KbCommand::List { .. }
            })
        ));

        let cli = Cli::try_parse_from([
            "ling",
            "kb",
            "doc",
            "idx-1",
            "add",
            "--name",
            "说明书.txt",
            "--url",
            "https://example.com/a.txt",
        ])
        .expect("parse kb doc add");
        match cli.command {
            Command::Kb(kb) => match kb.command {
                KbCommand::Doc { index_id, command } => {
                    assert_eq!(index_id, "idx-1");
                    assert!(matches!(command, KbDocCommand::Add { .. }));
                }
                other => panic!("expected kb doc command, got {other:?}"),
            },
            other => panic!("expected kb command, got {other:?}"),
        }
    }

    #[test]
    fn old_top_level_commands_are_gone() {
        for cmd in ["models", "chat", "create", "build", "dev", "deploy"] {
            assert!(
                Cli::try_parse_from(["ling", cmd]).is_err(),
                "`ling {cmd}` should no longer parse"
            );
        }
    }

    #[test]
    fn help_includes_new_command_groups() {
        let help = Cli::command().render_long_help().to_string();
        assert!(help.contains("ai"));
        assert!(help.contains("app"));
        assert!(help.contains("kb"));
        assert!(help.contains("wiki"));
    }
}
