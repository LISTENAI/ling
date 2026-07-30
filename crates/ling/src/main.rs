mod api_key;
mod config;
mod secret_prompt;
mod terminal;
#[cfg(test)]
mod test_support;
mod v1_api;

use anyhow::{anyhow, Context, Result};
use clap::{Args, CommandFactory, FromArgMatches, Parser, Subcommand};
use ling_plugin_app::request::{RequestDirection, RequestEvent, RequestInput, RequestOptions};
use ling_plugin_app::{config_view, management};
use serde_json::Value;
use std::collections::HashSet;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::time::Instant;

const DOCS_BASE_URL: &str = "https://docs2.listenai.com";
const PLATFORM_APP_CONFIG_URL: &str = "https://platform.listenai.com/appConfig";
const PLATFORM_APPLICATION_URL: &str = "https://platform.listenai.com/application";
const PLATFORM_CUSTOM_FIRMWARE_URL: &str = "https://platform.listenai.com/customFirmware";
const PLATFORM_KB_URL: &str = "https://platform.listenai.com/datasets";
const MAX_HOTWORD_CHARS: usize = 24;
const MAX_HOTWORDS_TOTAL_CHARS: usize = 1024;
const APP_CONFIG_EDITABLE_KEYS: [&str; 8] = [
    "name",
    "description",
    "interaction_mode",
    "system_prompt",
    "protocol",
    "endpoint",
    "model",
    "authorization",
];

#[derive(Debug, Parser)]
#[command(name = "ling", version, about = "ListenAI local CLI")]
struct Cli {
    #[arg(
        long,
        env = "LING_API_BASE_URL",
        default_value = ling_core::DEFAULT_API_BASE_URL
    )]
    api_base_url: String,

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
    /// Manage local CLI configuration.
    Config(LocalConfigArgs),
    /// Basic AI abilities: models, chat, TTS, and ASR.
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
    /// Print the raw JSON response.
    #[arg(long)]
    json: bool,
}

// ---------------------------------------------------------------------------
// ling config
// ---------------------------------------------------------------------------

#[derive(Debug, Args)]
struct LocalConfigArgs {
    #[command(subcommand)]
    command: LocalConfigCommand,
}

#[derive(Debug, Subcommand)]
enum LocalConfigCommand {
    /// Manage the local Device ID used by app requests.
    DeviceId(LocalDeviceIdArgs),
}

#[derive(Debug, Args)]
struct LocalDeviceIdArgs {
    #[command(subcommand)]
    command: LocalDeviceIdCommand,
}

#[derive(Debug, Subcommand)]
enum LocalDeviceIdCommand {
    /// Show the current local Device ID.
    Show {
        /// Print the result as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Generate and save a new local Device ID.
    Reset {
        /// Print the result as JSON.
        #[arg(long)]
        json: bool,
    },
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
    /// Recognize speech from a PCM/WAV file (wss /v2/asr).
    Asr(AsrArgs),
    /// Generate a wakeword resource (ROMFS bin).
    #[command(hide = true)]
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
    #[arg(required = true)]
    text: Vec<String>,
    /// Platform voice identifier (VCN).
    #[arg(long)]
    vcn: Option<String>,
    /// Audio format.
    #[arg(long, value_parser = ["mp3", "pcm"])]
    format: Option<String>,
    /// Sample rate in Hz.
    #[arg(long = "sample-rate", value_parser = ["8000", "16000", "24000"])]
    sample_rate: Option<String>,
    /// Speed, 1-100 (default 50).
    #[arg(long, value_parser = percentage_parser())]
    speed: Option<u32>,
    /// Volume, 1-100 (default 50).
    #[arg(long, value_parser = percentage_parser())]
    volume: Option<u32>,
    /// Pitch, 1-100 (default 50).
    #[arg(long, value_parser = percentage_parser())]
    pitch: Option<u32>,
    /// smartTTS emotion (e.g. cheerful, sad, auto).
    #[arg(long)]
    emotion: Option<String>,
    /// smartTTS emotion scale, -20..=20.
    #[arg(long = "emotion-scale", value_parser = emotion_scale_parser())]
    emotion_scale: Option<i32>,
    /// smartTTS style.
    #[arg(long)]
    style: Option<String>,
    /// Also download the audio into a file.
    #[arg(short = 'o', long)]
    output: Option<PathBuf>,
    /// Print the result as JSON.
    #[arg(long)]
    json: bool,
}

fn page_parser() -> clap::builder::RangedI64ValueParser<u32> {
    clap::value_parser!(u32).range(1..=1000)
}

fn page_size_parser() -> clap::builder::RangedI64ValueParser<u32> {
    clap::value_parser!(u32).range(1..=100)
}

fn percentage_parser() -> clap::builder::RangedI64ValueParser<u32> {
    clap::value_parser!(u32).range(1..=100)
}

fn emotion_scale_parser() -> clap::builder::RangedI64ValueParser<i32> {
    clap::value_parser!(i32).range(-20..=20)
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
    /// Show every ASR control frame and audio frame summary.
    #[arg(long)]
    verbose: bool,
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
    /// Target application by Product ID. Defaults to product_id in listenai.toml.
    #[arg(
        long = "product-id",
        global = true,
        conflicts_with_all = ["project_id", "app_id"]
    )]
    product_id: Option<String>,
    /// Target application directly by Project ID.
    #[arg(
        long = "project-id",
        global = true,
        conflicts_with_all = ["product_id", "app_id"]
    )]
    project_id: Option<String>,
    /// Target application by App ID.
    #[arg(
        long = "app-id",
        global = true,
        conflicts_with_all = ["product_id", "project_id"]
    )]
    app_id: Option<String>,
    #[command(subcommand)]
    command: AppCommand,
}

#[derive(Debug, Clone, Default)]
struct AppSelector {
    product_id: Option<String>,
    project_id: Option<String>,
    app_id: Option<String>,
}

#[derive(Debug)]
struct ResolvedApp {
    api_key: String,
    product_id: String,
    project_id: String,
}

#[derive(Debug, Subcommand)]
enum AppCommand {
    /// List platform apps.
    List {
        /// Page number (1-1000).
        #[arg(long, default_value_t = 1, value_parser = page_parser())]
        page: u32,
        /// Items per page (1-100).
        #[arg(long = "page-size", default_value_t = 20, value_parser = page_size_parser())]
        page_size: u32,
        /// Print the raw JSON response.
        #[arg(long)]
        json: bool,
    },
    /// Create a platform app.
    Create {
        /// App name.
        name: String,
        /// Application scenario description.
        #[arg(long)]
        description: Option<String>,
        /// Print the raw JSON response.
        #[arg(long)]
        json: bool,
    },
    /// Scaffold a local agent project and link it to a platform app.
    Init(InitArgs),
    /// Bundle the agent project to a single JS file.
    Build(ling_plugin_app_project::BuildArgs),
    /// Preview or upload an agent bundle to the platform.
    Deploy(ling_plugin_app_project::DeployArgs),
    /// List custom Agent versions or select the app test chain.
    Chain(ChainArgs),
    /// Inspect an app. Product id defaults to listenai.toml.
    Inspect {
        /// Positional product id (same as --product-id).
        #[arg(value_name = "product_id", id = "product_id_pos")]
        positional_product_id: Option<String>,
        /// Print the raw JSON response.
        #[arg(long)]
        json: bool,
    },
    /// Open the web workflow for deleting an app.
    Delete,
    /// Send a simulated request and show a bidirectional event timeline.
    Request(RequestArgs),
    /// Look up the Agent execution log by SID.
    Trace {
        /// SID printed by `ling app request` or found in link frames.
        sid: String,
        /// Show every Agent log entry as one compact line.
        #[arg(long, visible_alias = "full")]
        verbose: bool,
        /// Print the raw Agent log response as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Device management.
    Device(DeviceArgs),
    /// Firmware OTA management.
    Ota(OtaArgs),
    /// Role management.
    Role(RoleArgs),
    /// Wake-up word generation and response management.
    Wakeword(AppWakewordArgs),
    /// App-linked knowledge bases.
    Kb(AppKbArgs),
    /// Domain lexicon (hotwords) management.
    Lexicon(LexiconArgs),
    /// Device prompt tone texts.
    Tone(ToneArgs),
    /// MCP server configuration.
    Mcp(McpArgs),
    /// App metadata, interaction, prompt, and model access.
    Config(ConfigArgs),
}

#[derive(Debug, Args)]
struct InitArgs {
    #[command(flatten)]
    create: ling_plugin_app_project::CreateArgs,
}

#[derive(Debug, Args)]
struct RequestArgs {
    /// Product Secret. Required when the target app is not manageable by this account.
    #[arg(
        long = "product-secret",
        env = "LING_PRODUCT_SECRET",
        hide_env_values = true
    )]
    product_secret: Option<String>,
    /// Send a text utterance.
    #[arg(long, conflicts_with = "file", required_unless_present = "file")]
    text: Option<String>,
    /// Send an audio file (raw PCM or 16k 16bit LE mono WAV).
    #[arg(long)]
    file: Option<PathBuf>,
    /// Override the stable per-install device id.
    #[arg(long = "device-id")]
    device_id: Option<String>,
    /// Override the app id (llm_app) for targeted debugging.
    #[arg(long = "llm-app")]
    llm_app: Option<String>,
    /// Show every protocol frame with timestamp and direction.
    #[arg(long)]
    verbose: bool,
    /// Download the first returned TTS audio as MP3 without format conversion.
    #[arg(long = "output-tts", value_name = "MP3_FILE")]
    output_tts: Option<PathBuf>,
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
    /// Show where to view imported device ids.
    List,
    /// Import one or more device ids, or upload a text file.
    Add {
        #[arg(required_unless_present_any = ["file", "self_device"])]
        device_ids: Vec<String>,
        /// UTF-8 text file with one device id per line.
        #[arg(long, conflicts_with_all = ["device_ids", "self_device"])]
        file: Option<PathBuf>,
        /// Import this CLI installation's local Device ID.
        #[arg(
            long = "self",
            conflicts_with_all = ["device_ids", "file"]
        )]
        self_device: bool,
        /// Print the raw JSON response.
        #[arg(long)]
        json: bool,
    },
    /// Check whether a device id is authorized.
    Query {
        device_id: String,
        /// Print the raw JSON response.
        #[arg(long)]
        json: bool,
    },
    /// Show which devices are allowed to connect. Changes are web-only.
    Enforce {
        /// Print the raw JSON response.
        #[arg(long)]
        json: bool,
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
    List {
        /// Page number (1-1000).
        #[arg(long, default_value_t = 1, value_parser = page_parser())]
        page: u32,
        /// Items per page (1-100).
        #[arg(long = "page-size", default_value_t = 20, value_parser = page_size_parser())]
        page_size: u32,
        #[arg(long)]
        json: bool,
    },
    /// Upload an OTA package.
    Upload {
        file: PathBuf,
        #[arg(long)]
        version: String,
        #[arg(long = "version-number")]
        version_number: u64,
        #[arg(long = "ota-mode", value_parser = ["selectable", "mandatory"])]
        ota_mode: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Show an OTA package.
    Show {
        package_id: String,
        #[arg(long)]
        json: bool,
    },
    /// Edit an OTA package.
    Edit {
        package_id: String,
        /// Replace the firmware package.
        #[arg(long)]
        file: Option<PathBuf>,
        /// Update the human-readable version.
        #[arg(long)]
        version: Option<String>,
        /// Update the release description.
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Open the web workflow for formal OTA publication.
    Publish { package_id: String },
    /// Open the web workflow for revoking formal OTA publication.
    Revoke { package_id: String },
    /// Delete an unpublished OTA package.
    Delete {
        package_id: String,
        /// Skip the confirmation prompt.
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        json: bool,
    },
    /// Manage the OTA test whitelist.
    Whitelist {
        #[command(subcommand)]
        command: OtaWhitelistCommand,
    },
}

#[derive(Debug, Subcommand)]
enum OtaWhitelistCommand {
    List {
        /// Page number (1-1000).
        #[arg(long, default_value_t = 1, value_parser = page_parser())]
        page: u32,
        /// Items per page (1-100).
        #[arg(long = "page-size", default_value_t = 20, value_parser = page_size_parser())]
        page_size: u32,
        #[arg(long)]
        json: bool,
    },
    Add {
        #[arg(required_unless_present = "self_device")]
        device_id: Option<String>,
        /// Add this CLI installation's local Device ID.
        #[arg(long = "self", conflicts_with = "device_id")]
        self_device: bool,
        #[arg(long)]
        json: bool,
    },
    Delete {
        device_id: String,
        #[arg(long)]
        json: bool,
    },
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
        /// Page number (1-1000).
        #[arg(long, default_value_t = 1, value_parser = page_parser())]
        page: u32,
        /// Items per page (1-100).
        #[arg(long = "page-size", default_value_t = 20, value_parser = page_size_parser())]
        page_size: u32,
        /// Print the raw JSON response.
        #[arg(long)]
        json: bool,
    },
    /// Show the complete configuration of one role.
    Show {
        role_id: String,
        /// Print the raw JSON response.
        #[arg(long)]
        json: bool,
    },
    /// Create a role.
    Create {
        name: String,
        #[command(flatten)]
        input: JsonEditArgs,
        #[arg(long)]
        json: bool,
    },
    /// Edit a role.
    Edit {
        role_id: String,
        #[command(flatten)]
        input: JsonEditArgs,
        #[arg(long)]
        json: bool,
    },
    /// Show the web page for deleting a role.
    Delete { role_id: String },
    /// Set the default role.
    #[command(name = "set-default")]
    SetDefault {
        role_id: String,
        #[arg(long)]
        json: bool,
    },
    /// Show or switch the wake-up word used by a role.
    Wakeword(RoleWakewordArgs),
}

#[derive(Debug, Args)]
struct RoleWakewordArgs {
    #[command(subcommand)]
    command: RoleWakewordCommand,
}

#[derive(Debug, Subcommand)]
enum RoleWakewordCommand {
    /// Show the wake-up word currently used by a role.
    Show {
        role_id: String,
        #[arg(long)]
        json: bool,
    },
    /// Switch a role to a ready wake-up word.
    Set {
        role_id: String,
        wakeword_id: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Args)]
struct AppWakewordArgs {
    #[command(subcommand)]
    command: WakewordCommand,
}

#[derive(Debug, Subcommand)]
enum WakewordCommand {
    /// List generated and system wake-up words.
    List {
        /// Page number (1-1000).
        #[arg(long, default_value_t = 1, value_parser = page_parser())]
        page: u32,
        /// Items per page (1-100).
        #[arg(long = "page-size", default_value_t = 20, value_parser = page_size_parser())]
        page_size: u32,
        #[arg(long)]
        json: bool,
    },
    /// Show one wake-up word and its generation status.
    Show {
        wakeword_id: String,
        #[arg(long)]
        json: bool,
    },
    /// Start generating a wake-up word.
    Generate {
        name: String,
        #[arg(long, value_parser = ["low", "medium", "high"], default_value = "medium")]
        sensitivity: String,
        /// Initial response text (1-12 chars). Repeat up to five times.
        #[arg(long = "response")]
        responses: Vec<String>,
        /// Description, up to 120 characters.
        #[arg(long)]
        description: Option<String>,
        /// Skip the paid-generation confirmation prompt.
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        json: bool,
    },
    /// Show all response texts of a wake-up word.
    Responses {
        /// Wake-up word ID.
        wakeword_id: String,
        /// Print the raw JSON response.
        #[arg(long)]
        json: bool,
    },
    /// Replace all response texts of a wake-up word.
    #[command(name = "set-responses")]
    SetResponses {
        /// Wake-up word ID.
        wakeword_id: String,
        /// Response texts (1-12 chars each, up to five).
        #[arg(value_name = "TEXT", required = true, num_args = 1..=5)]
        responses: Vec<String>,
        /// Print the raw JSON response.
        #[arg(long)]
        json: bool,
    },
    /// Restore the default response text of a wake-up word.
    #[command(name = "reset-responses")]
    ResetResponses {
        /// Wake-up word ID.
        wakeword_id: String,
        /// Print the raw JSON response.
        #[arg(long)]
        json: bool,
    },
    /// Delete a generated wake-up word.
    Delete {
        wakeword_id: String,
        /// Skip the confirmation prompt.
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        json: bool,
    },
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
        /// Page number (1-1000).
        #[arg(long, default_value_t = 1, value_parser = page_parser())]
        page: u32,
        /// Items per page (1-100).
        #[arg(long = "page-size", default_value_t = 20, value_parser = page_size_parser())]
        page_size: u32,
        /// Print the raw JSON response.
        #[arg(long)]
        json: bool,
    },
    /// Link a knowledge base to the app.
    Link {
        index_id: String,
        #[arg(long)]
        json: bool,
    },
    /// Unlink a knowledge base from the app.
    Unlink {
        index_id: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Args)]
struct LexiconArgs {
    #[command(subcommand)]
    command: LexiconCommand,
}

#[derive(Debug, Args)]
struct ChainArgs {
    #[command(subcommand)]
    command: ChainCommand,
}

#[derive(Debug, Subcommand)]
enum ChainCommand {
    /// Show the selected test chain mode and version.
    Show {
        /// Print the raw JSON response.
        #[arg(long)]
        json: bool,
    },
    /// List uploaded custom Agent versions.
    Versions {
        /// Page number (1-1000).
        #[arg(long, default_value_t = 1, value_parser = page_parser())]
        page: u32,
        /// Items per page (1-100).
        #[arg(long = "page-size", default_value_t = 20, value_parser = page_size_parser())]
        page_size: u32,
        /// Print the raw JSON response.
        #[arg(long)]
        json: bool,
    },
    /// Select the chain mode and, for custom chains, an uploaded version.
    Set {
        #[command(subcommand)]
        target: ChainSetCommand,
    },
}

#[derive(Debug, Subcommand)]
enum ChainSetCommand {
    /// Use the latest managed Agent version.
    Managed {
        /// Print the raw JSON response.
        #[arg(long)]
        json: bool,
    },
    /// Use an uploaded custom Agent version.
    Custom {
        /// Uploaded version in vX.Y.Z format.
        version: String,
        /// Print the raw JSON response.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum LexiconCommand {
    /// List domain lexicon entries.
    List {
        /// Page number (1-1000).
        #[arg(long, default_value_t = 1, value_parser = page_parser())]
        page: u32,
        /// Items per page (1-100).
        #[arg(long = "page-size", default_value_t = 20, value_parser = page_size_parser())]
        page_size: u32,
        /// Print the raw JSON response.
        #[arg(long)]
        json: bool,
    },
    /// Add a lexicon entry.
    Add {
        word: String,
        #[arg(long)]
        json: bool,
    },
    /// Import professional vocabulary from a UTF-8 text file, one word per line.
    Import {
        file: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Edit a lexicon entry without changing its ID.
    Edit {
        hotword_id: String,
        word: String,
        #[arg(long)]
        json: bool,
    },
    /// Show the web page for deleting a lexicon entry.
    Delete { hotword_id: String },
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
        /// Page number (1-1000).
        #[arg(long, default_value_t = 1, value_parser = page_parser())]
        page: u32,
        /// Items per page (1-100).
        #[arg(long = "page-size", default_value_t = 20, value_parser = page_size_parser())]
        page_size: u32,
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
        /// Restore every prompt tone to its default.
        #[arg(long = "reset-all", conflicts_with_all = ["set", "reset", "file"])]
        reset_all: bool,
        /// Complete `texts` array or `{ "texts": [...] }` JSON file.
        #[arg(long, conflicts_with_all = ["set", "reset", "reset_all"])]
        file: Option<PathBuf>,
        #[arg(long)]
        json: bool,
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
    List {
        /// Page number (1-1000).
        #[arg(long, default_value_t = 1, value_parser = page_parser())]
        page: u32,
        /// Items per page (1-100).
        #[arg(long = "page-size", default_value_t = 20, value_parser = page_size_parser())]
        page_size: u32,
        #[arg(long)]
        json: bool,
    },
    /// Show the complete configuration of one MCP server.
    Show {
        mcp_id: String,
        #[arg(long)]
        json: bool,
    },
    /// Add an existing MCP server to the app.
    Add {
        name: String,
        #[arg(long = "server-id")]
        server_id: String,
        #[arg(long = "transport", value_parser = ["sse", "http"])]
        transport_type: String,
        #[arg(long)]
        url: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        authorization: Option<String>,
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        enabled: bool,
        #[arg(long)]
        json: bool,
    },
    /// Edit an MCP server.
    Edit {
        mcp_id: String,
        #[command(flatten)]
        input: JsonEditArgs,
        #[arg(long)]
        json: bool,
    },
    /// Show the web page for deleting an MCP server.
    Delete { mcp_id: String },
    /// Enable an MCP server.
    Enable {
        mcp_id: String,
        #[arg(long)]
        json: bool,
    },
    /// Disable an MCP server.
    Disable {
        mcp_id: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Args)]
struct JsonEditArgs {
    /// Set a field as key=value. Repeat to update multiple fields.
    #[arg(long = "set", value_name = "key=value", conflicts_with = "file")]
    set: Vec<String>,
    /// Read the request object from a JSON file.
    #[arg(long, conflicts_with = "set")]
    file: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct ConfigArgs {
    #[command(subcommand)]
    command: ConfigCommand,
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    /// Show app metadata, interaction, prompt, and model access.
    Show {
        #[arg(long)]
        json: bool,
    },
    /// Update fields shown by `config show`.
    Edit {
        #[command(flatten)]
        input: JsonEditArgs,
        #[arg(long)]
        json: bool,
    },
    /// Restore the default Agent model access.
    #[command(name = "reset-model")]
    ResetModel {
        #[arg(long)]
        json: bool,
    },
    /// Test a model access configuration without saving it.
    #[command(name = "test-model")]
    TestModel {
        #[arg(long)]
        endpoint: String,
        #[arg(long)]
        model: String,
        #[arg(long)]
        authorization: Option<String>,
        #[arg(long)]
        json: bool,
    },
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
        /// Page number (1-1000).
        #[arg(long, default_value_t = 1, value_parser = page_parser())]
        page: u32,
        /// Items per page (1-100).
        #[arg(long, default_value_t = 20, value_parser = page_size_parser())]
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
    /// Show the web page for deleting a knowledge base.
    Delete {
        index_id: String,
        /// Deprecated compatibility flag; deletion is web-only.
        #[arg(long, hide = true)]
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
        /// Page number (1-1000).
        #[arg(long, default_value_t = 1, value_parser = page_parser())]
        page: u32,
        /// Items per page (1-100).
        #[arg(long, default_value_t = 20, value_parser = page_size_parser())]
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
        /// Print the raw JSON response.
        #[arg(long)]
        json: bool,
    },
    /// Show the web page for deleting documents.
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
    let cli = match cli_command()
        .try_get_matches()
        .and_then(|matches| Cli::from_arg_matches(&matches))
    {
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
}

async fn run(cli: Cli) -> Result<ExitCode> {
    let Cli {
        api_base_url,
        command,
    } = cli;
    let ctx = Ctx { api_base_url };

    match command {
        Command::Login(args) => {
            login(ctx.api_base_url, args).await?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Account { json } => {
            account_command(ctx.api_base_url, json).await?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Config(args) => {
            local_config_command(args)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Ai(args) => ai_command(&ctx, args).await,
        Command::App(args) => app_command(&ctx, args).await,
        Command::Kb(args) => {
            kb_command(&ctx.api_base_url, args).await?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Wiki(args) => {
            wiki_command(args).await?;
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

    if args.json {
        print_json(&output)
    } else {
        eprintln!("{}", api_key::render_login_success(&output, &api_base_url));
        Ok(())
    }
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

fn local_config_command(args: LocalConfigArgs) -> Result<()> {
    match args.command {
        LocalConfigCommand::DeviceId(args) => match args.command {
            LocalDeviceIdCommand::Show { json } => {
                let device_id = config::LingConfig::show_device_id()?;
                if json {
                    print_json(&serde_json::json!({"device_id": device_id}))
                } else {
                    println!("本地 Device ID：{device_id}");
                    Ok(())
                }
            }
            LocalDeviceIdCommand::Reset { json } => {
                let device_id = config::LingConfig::reset_local_device_id()?;
                if json {
                    print_json(&serde_json::json!({"device_id": device_id}))
                } else {
                    println!("已重新生成本地 Device ID：{device_id}");
                    Ok(())
                }
            }
        },
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
            return Err(platform_write_unavailable(
                "唤醒词资源生成",
                PLATFORM_CUSTOM_FIRMWARE_URL,
            ));
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
    let verbose = args.verbose;
    let show_partial = io::stderr().is_terminal() && !verbose;
    let text = ling_plugin_ai::asr(api_base_url, &api_key, &audio, &opts, |event| {
        if verbose {
            if let Some(line) = ling_plugin_ai::render_asr_verbose_event(&event) {
                eprintln!("{line}");
            }
        } else if show_partial {
            if let ling_plugin_ai::AsrEvent::Partial { text } = event {
                eprintln!("… {text}");
            }
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
    let selector = AppSelector {
        product_id: args.product_id,
        project_id: args.project_id,
        app_id: args.app_id,
    };
    match args.command {
        AppCommand::List {
            page,
            page_size,
            json,
        } => {
            ensure_no_app_selector(&selector, "list")?;
            let api_key = resolve_api_key()?;
            let output = ling_plugin_app::list_product_projects(
                &cli.api_base_url,
                &api_key,
                page,
                page_size,
            )
            .await?;
            if json {
                print_json(&output)?;
            } else {
                println!("{}", ling_plugin_app::render_project_list(&output)?);
            }
        }
        AppCommand::Create {
            name,
            description,
            json,
        } => {
            ensure_no_app_selector(&selector, "create")?;
            let api_key = resolve_api_key()?;
            let output = management::create_project(
                &cli.api_base_url,
                &api_key,
                &name,
                description.as_deref(),
            )
            .await?;
            if json {
                print_json(&output)?;
            } else {
                print_action_result(&output, "应用创建成功")?;
            }
        }
        AppCommand::Init(args) => {
            let product_id = explicit_product_id(cli, selector).await?;
            return init_command(cli, args, product_id).await;
        }
        AppCommand::Build(args) => {
            ensure_no_app_selector(&selector, "build")?;
            let ctx = agent_context(cli)?;
            return ling_plugin_app_project::build_command(&ctx, args).await;
        }
        AppCommand::Deploy(mut args) => {
            args.product_id = explicit_product_id(cli, selector).await?;
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
        AppCommand::Chain(args) => chain_command(cli, args, selector).await?,
        AppCommand::Inspect {
            positional_product_id,
            json,
        } => {
            let selector = selector.with_positional_product(positional_product_id)?;
            let app = resolve_app(cli, selector).await?;
            let output = get_app_detail(cli, &app).await?;
            if json {
                print_json(&output)?;
            } else {
                let mcp_count = management::list_all_resource(
                    &cli.api_base_url,
                    &app.api_key,
                    &app.project_id,
                    &["mcp-servers"],
                )
                .await
                .ok()
                .map(|servers| servers.len());
                println!(
                    "{}",
                    ling_plugin_app::render_project_inspect_with_mcp_count(&output, mcp_count)?
                );
            }
        }
        AppCommand::Delete => {
            let app = resolve_app(cli, selector).await?;
            return Err(application_only_operation(
                "删除应用",
                &app.product_id,
                "设置",
            ));
        }
        AppCommand::Request(args) => request_command(cli, args, selector).await?,
        AppCommand::Trace { sid, verbose, json } => {
            ensure_no_app_selector(&selector, "trace")?;
            trace_command(cli, &sid, verbose, json).await?
        }
        AppCommand::Device(args) => device_command(cli, args, selector).await?,
        AppCommand::Ota(args) => ota_command(cli, args, selector).await?,
        AppCommand::Role(args) => role_command(cli, args, selector).await?,
        AppCommand::Wakeword(args) => wakeword_command(cli, args, selector).await?,
        AppCommand::Kb(args) => app_kb_command(cli, args, selector).await?,
        AppCommand::Lexicon(args) => lexicon_command(cli, args, selector).await?,
        AppCommand::Tone(args) => tone_command(cli, args, selector).await?,
        AppCommand::Mcp(args) => mcp_command(cli, args, selector).await?,
        AppCommand::Config(args) => config_command(cli, args, selector).await?,
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
            eprintln!(
                "已关联应用 product_id={product_id}（写入 {}）",
                ling_plugin_app_project::project::manifest_path(&project_dir).display()
            );
        }
        None => {
            eprintln!(
                "未关联平台应用。可稍后在 {} 中设置 product_id，或重新运行 `ling app init`。",
                ling_plugin_app_project::project::manifest_path(&project_dir).display()
            );
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// 交互式选择一个平台应用；非交互环境或无 API Key 时返回 None。
async fn select_product_interactively(cli: &Ctx) -> Result<Option<String>> {
    if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        eprintln!("非交互环境，跳过应用关联（可用 --product-id 指定）。");
        return Ok(None);
    }
    let Some(api_key) = resolve_optional_api_key()? else {
        eprintln!("未找到 API Key，跳过应用关联（先 `ling login` 或用 --product-id 指定）。");
        return Ok(None);
    };

    let output = ling_plugin_app::list_product_projects(&cli.api_base_url, &api_key, 1, 50).await?;
    let projects = output
        .get("data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if projects.is_empty() {
        eprintln!("平台上暂无应用，跳过关联。");
        return Ok(None);
    }

    eprintln!("选择要关联的平台应用：");
    for (index, project) in projects.iter().enumerate() {
        eprintln!(
            "  {}. {} ({})",
            index + 1,
            project.get("name").and_then(Value::as_str).unwrap_or("-"),
            project
                .get("product_id")
                .and_then(Value::as_str)
                .unwrap_or("-")
        );
    }
    eprint!("输入编号（回车跳过）: ");
    io::stderr().flush()?;
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

async fn request_command(cli: &Ctx, args: RequestArgs, selector: AppSelector) -> Result<()> {
    let verbose = args.verbose;
    let output_tts = args.output_tts;
    let output_tts_for_download = output_tts.clone();
    let request_output = RequestTimelineOutput::new(io::stdout().is_terminal() && !verbose);
    let device_id = match args.device_id.clone() {
        Some(device_id) => device_id,
        None => config::LingConfig::load_or_create_device_id()?,
    };
    let supplied_secret = normalize_product_secret(args.product_secret)?;
    let direct_product_id = supplied_secret
        .as_ref()
        .and_then(|_| direct_request_product_id(&selector));
    let (product_id, secret) = match (direct_product_id, supplied_secret) {
        (Some(product_id), Some(secret)) => (product_id, secret),
        (_, supplied_secret) => {
            let app = resolve_app(cli, selector).await?;
            let secret = match supplied_secret {
                Some(secret) => secret,
                None => {
                    let detail = get_app_detail(cli, &app).await?;
                    config_view::product_secret(&detail).context(
                        "应用详情未返回产品密钥。请由用户本人前往平台网页的应用详情复制 Secret，\
                         然后在自己的终端中传入 --product-secret <secret>；不要把 Secret 发到对话或日志中",
                    )?
                }
            };
            (app.product_id, secret)
        }
    };

    let input = if let Some(text) = args.text {
        RequestInput::Text(text)
    } else {
        let file = args.file.expect("clap 保证 text/file 至少一个");
        RequestInput::Audio(ling_plugin_ai::load_pcm_audio(&file)?)
    };
    let opts = RequestOptions {
        device_id,
        llm_app: args.llm_app,
    };

    let mut sid: Option<String> = None;
    let mut upstream_frames = 0_u64;
    let mut downstream_frames = 0_u64;
    let mut upstream_bytes = 0_u64;
    let mut downstream_bytes = 0_u64;
    let mut text_urls = Vec::<String>::new();
    let mut tts_urls = Vec::<String>::new();
    let mut seen_text_urls = HashSet::<String>::new();
    let mut seen_tts_urls = HashSet::<String>::new();
    let mut text_stream_tasks = Vec::new();
    let mut tts_download_task = None;
    let started_at = Instant::now();
    let interaction_result = ling_plugin_app::request::interaction_request(
        &cli.api_base_url,
        &product_id,
        &secret,
        &input,
        &opts,
        |event| {
            match &event {
                RequestEvent::Frame {
                    direction,
                    body: frame,
                } => {
                    match direction {
                        RequestDirection::Upstream => {
                            upstream_frames += 1;
                            upstream_bytes += frame.len() as u64;
                        }
                        RequestDirection::Downstream => {
                            downstream_frames += 1;
                            downstream_bytes += frame.len() as u64;
                        }
                    }
                    if sid.is_none() {
                        sid = serde_json::from_str::<Value>(frame).ok().and_then(|value| {
                            value
                                .get("sid")
                                .and_then(Value::as_str)
                                .filter(|sid| !sid.is_empty())
                                .map(str::to_owned)
                        });
                    }
                    if matches!(direction, &RequestDirection::Downstream) {
                        if let Some(url) = ling_plugin_app::request::text_stream_url(frame) {
                            if seen_text_urls.insert(url.clone()) {
                                text_urls.push(url.clone());
                                let reply_output = request_output.clone();
                                let sse_output = request_output.clone();
                                let verbose_sse = verbose;
                                text_stream_tasks.push(tokio::spawn(async move {
                                    ling_plugin_app::request::stream_reply_text(
                                        &url,
                                        |text| reply_output.update_reply(text),
                                        |frame| {
                                            if verbose_sse {
                                                sse_output.print_line(
                                                    &ling_plugin_app::request::render_verbose_sse_frame(frame),
                                                );
                                            }
                                        },
                                    )
                                    .await
                                }));
                            }
                        }
                        if let Some(url) = ling_plugin_app::request::tts_url(frame) {
                            if seen_tts_urls.insert(url.clone()) {
                                tts_urls.push(url.clone());
                                if tts_download_task.is_none() {
                                    if let Some(path) = output_tts_for_download.clone() {
                                        tts_download_task = Some(tokio::spawn(async move {
                                            ling_plugin_app::request::download_tts(&url, &path)
                                                .await
                                        }));
                                    }
                                }
                            }
                        }
                    }
                }
                RequestEvent::Binary {
                    direction, bytes, ..
                } => match direction {
                    RequestDirection::Upstream => {
                        upstream_frames += 1;
                        upstream_bytes += *bytes as u64;
                    }
                    RequestDirection::Downstream => {
                        downstream_frames += 1;
                        downstream_bytes += *bytes as u64;
                    }
                },
            }
            if verbose {
                request_output.print_line(&ling_plugin_app::request::render_verbose_event(&event));
            } else {
                request_output.print_line(&ling_plugin_app::request::render_event(&event));
            }
        },
    )
    .await;

    if let Err(error) = interaction_result {
        for task in &text_stream_tasks {
            task.abort();
        }
        request_output.finish_reply(None);
        return Err(error);
    }

    for task in text_stream_tasks {
        match task.await {
            Ok(Ok(text)) => request_output.finish_reply(Some(&text)),
            Ok(Err(error)) => {
                request_output.finish_reply(None);
                request_output.print_line(&ling_plugin_app::request::render_reply_stream_error(
                    &error.to_string(),
                ));
            }
            Err(error) => {
                request_output.finish_reply(None);
                request_output.print_line(&ling_plugin_app::request::render_reply_stream_error(
                    &error.to_string(),
                ));
            }
        }
    }

    let saved_tts = match (output_tts, tts_download_task) {
        (Some(path), Some(task)) => {
            let bytes = task.await.context("TTS 下载任务异常结束")??;
            Some((path, bytes))
        }
        (Some(_), None) => {
            anyhow::bail!("未收到 TTS URL，无法使用 --output-tts 保存 MP3")
        }
        (None, _) => None,
    };

    let elapsed = started_at.elapsed();
    println!();
    println!("- Device ID: {}", opts.device_id);
    if let Some(sid) = &sid {
        println!("- SID: {sid}");
    }
    for url in &tts_urls {
        println!("- TTS URL: {url}");
    }
    for url in &text_urls {
        println!("- 文本 URL: {url}");
    }
    if let Some((path, bytes)) = saved_tts {
        println!("- TTS MP3 文件: {}（{bytes} bytes）", path.display());
    }
    println!("- 耗时: {:.2}s", elapsed.as_secs_f64());
    println!("- 上行: {upstream_frames} 帧，{upstream_bytes} bytes");
    println!("- 下行: {downstream_frames} 帧，{downstream_bytes} bytes");
    Ok(())
}

#[derive(Clone)]
struct RequestTimelineOutput {
    state: Arc<Mutex<RequestTimelineOutputState>>,
}

struct RequestTimelineOutputState {
    live_updates: bool,
    terminal_width: usize,
    live_reply: Option<String>,
}

impl RequestTimelineOutput {
    fn new(live_updates: bool) -> Self {
        Self {
            state: Arc::new(Mutex::new(RequestTimelineOutputState {
                live_updates,
                terminal_width: terminal::width().unwrap_or(80),
                live_reply: None,
            })),
        }
    }

    fn print_line(&self, line: &str) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.refresh_terminal_width();
        Self::write_output(Some(state.print_line_output(line)));
    }

    fn update_reply(&self, text: &str) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.refresh_terminal_width();
        Self::write_output(state.update_reply_output(text));
    }

    fn finish_reply(&self, final_text: Option<&str>) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        Self::write_output(state.finish_reply_output(final_text));
    }

    fn write_output(output: Option<String>) {
        let Some(output) = output else {
            return;
        };
        let mut stdout = io::stdout().lock();
        let _ = write!(stdout, "{output}");
        let _ = stdout.flush();
    }
}

impl RequestTimelineOutputState {
    fn refresh_terminal_width(&mut self) {
        if self.live_updates {
            if let Some(width) = terminal::width() {
                self.terminal_width = width;
            }
        }
    }

    fn reply_preview(&self, text: &str) -> String {
        // Keep the last terminal column empty: writing into it can trigger an automatic wrap.
        ling_plugin_app::request::render_reply_preview(text, self.terminal_width.saturating_sub(1))
    }

    fn print_line_output(&self, line: &str) -> String {
        if self.live_updates {
            if let Some(reply) = &self.live_reply {
                let reply_line = self.reply_preview(reply);
                return format!("\r\x1b[2K{line}\n{reply_line}");
            }
        }
        format!("{line}\n")
    }

    fn update_reply_output(&mut self, text: &str) -> Option<String> {
        let line = self.reply_preview(text);
        self.live_reply = Some(text.to_owned());
        self.live_updates.then(|| format!("\r\x1b[2K{line}"))
    }

    fn finish_reply_output(&mut self, final_text: Option<&str>) -> Option<String> {
        let line = match final_text.filter(|text| !text.is_empty()) {
            Some(text) => ling_plugin_app::request::render_reply_text(text),
            None => match &self.live_reply {
                Some(live_text) => ling_plugin_app::request::render_reply_text(live_text),
                None => return None,
            },
        };
        self.live_reply = None;
        Some(if self.live_updates {
            format!("\r\x1b[2K{line}\n")
        } else {
            format!("{line}\n")
        })
    }
}

async fn trace_command(cli: &Ctx, sid: &str, verbose: bool, json: bool) -> Result<()> {
    let api_key = resolve_api_key()?;
    let logs = ling_plugin_app::records::query_agent_logs(&cli.api_base_url, &api_key, sid).await?;
    let has_logs = logs.as_ref().is_some_and(|output| {
        output
            .get("data")
            .and_then(Value::as_array)
            .is_some_and(|logs| !logs.is_empty())
    });
    // 未命中报错退出，便于脚本判断。
    if !has_logs {
        anyhow::bail!(
            "未找到 SID 为 {sid} 的 Agent 执行日志；请核对 SID，\
             或确认该会话是否已产生日志（日志有保留期）。"
        );
    }
    let output = logs.expect("has_logs 已确认日志存在");
    if json {
        return print_json(&output);
    }
    println!(
        "{}",
        ling_plugin_app::records::render_agent_trace(&output, sid, verbose)?
    );
    Ok(())
}

async fn device_command(cli: &Ctx, args: DeviceArgs, selector: AppSelector) -> Result<()> {
    if matches!(&args.command, DeviceCommand::List) {
        println!("{}", device_list_guidance());
        return Ok(());
    }

    let local_device_id = match &args.command {
        DeviceCommand::Add {
            self_device: true, ..
        } => Some(config::LingConfig::load_or_create_device_id()?),
        _ => None,
    };
    let app = resolve_app(cli, selector).await?;
    match args.command {
        DeviceCommand::Quota { json } => {
            let detail = get_app_detail(cli, &app).await?;
            if json {
                let product = config_view::project_data(&detail)
                    .get("product")
                    .cloned()
                    .unwrap_or(Value::Null);
                print_json(&serde_json::json!({
                    "assignedDeviceQuota": product.get("assignedDeviceQuota").or_else(|| product.get("assigned_device_quota")),
                    "consumedDeviceQuota": product.get("consumedDeviceQuota").or_else(|| product.get("consumed_device_quota")),
                    "deviceAuthCheck": product.get("deviceAuthCheck").or_else(|| product.get("device_auth_check")),
                }))?;
            } else {
                println!("{}", config_view::render_device_quota(&detail)?);
            }
        }
        DeviceCommand::Query { device_id, json } => {
            let output = ling_plugin_app::device_query(
                &cli.api_base_url,
                &app.api_key,
                &app.product_id,
                &device_id,
            )
            .await?;
            let valid = output
                .get("is_valid")
                .or_else(|| output.pointer("/data/is_valid"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if json {
                print_json(&output)?;
            }
            if valid {
                if !json {
                    println!("设备 {device_id} 已授权（is_valid=true）。");
                }
            } else {
                anyhow::bail!(
                    "设备 {device_id} 未授权（is_valid=false）；请先导入设备或检查 Product ID"
                )
            }
        }
        DeviceCommand::Enforce { json } => {
            let output = management::get_resource(
                &cli.api_base_url,
                &app.api_key,
                &app.project_id,
                &["device-whitelist"],
            )
            .await?;
            if json {
                print_json(&output)?;
            } else {
                let enabled = output
                    .pointer("/data/enabled")
                    .and_then(Value::as_bool)
                    .context("设备白名单响应缺少 data.enabled")?;
                println!("{}", device_enforcement_summary(enabled));
            }
        }
        DeviceCommand::List => unreachable!("handled before app resolution"),
        DeviceCommand::Add {
            device_ids,
            file,
            self_device,
            json,
        } => {
            debug_assert_eq!(self_device, local_device_id.is_some());
            let output = if let Some(file) = file {
                management::upload_device_file(
                    &cli.api_base_url,
                    &app.api_key,
                    &app.project_id,
                    &file,
                )
                .await?
            } else {
                management::create_resource(
                    &cli.api_base_url,
                    &app.api_key,
                    &app.project_id,
                    &["devices", "import-by-ids"],
                    serde_json::json!({
                        "device_ids": local_device_id
                            .clone()
                            .map(|device_id| vec![device_id])
                            .unwrap_or(device_ids)
                    }),
                )
                .await?
            };
            if json {
                print_json(&output)?;
            }
            validate_device_import_result(&output)?;
            if !json {
                if let Some(device_id) = local_device_id {
                    eprintln!("已导入当前 CLI Device ID：{device_id}");
                } else {
                    eprintln!("设备导入成功");
                }
            }
        }
    }
    Ok(())
}

async fn role_command(cli: &Ctx, args: RoleArgs, selector: AppSelector) -> Result<()> {
    let app = resolve_app(cli, selector).await?;
    match args.command {
        RoleCommand::List {
            page,
            page_size,
            json,
        } => {
            let output = management::list_resource(
                &cli.api_base_url,
                &app.api_key,
                &app.project_id,
                &["roles"],
                page,
                page_size,
            )
            .await?;
            if json {
                print_json(&output)
            } else {
                println!("{}", config_view::render_management_role_list(&output)?);
                Ok(())
            }
        }
        RoleCommand::Show { role_id, json } => {
            let output = management::get_resource(
                &cli.api_base_url,
                &app.api_key,
                &app.project_id,
                &["roles", &role_id],
            )
            .await?;
            if json {
                print_json(&output)
            } else {
                let roles = management::list_all_resource(
                    &cli.api_base_url,
                    &app.api_key,
                    &app.project_id,
                    &["roles"],
                )
                .await?;
                let output = role_detail_with_project_default(output, &roles, &role_id);
                println!("{}", config_view::render_management_role_detail(&output)?);
                Ok(())
            }
        }
        RoleCommand::Create { name, input, json } => {
            let mut body = json_body_from_input(input, role_edit_key)?;
            body.as_object_mut()
                .context("角色请求必须是 JSON 对象")?
                .insert("name".to_owned(), Value::String(name));
            let output = management::create_resource(
                &cli.api_base_url,
                &app.api_key,
                &app.project_id,
                &["roles"],
                body,
            )
            .await?;
            print_action_or_json(&output, json, "角色创建成功")
        }
        RoleCommand::Edit {
            role_id,
            input,
            json,
        } => {
            let mut body = json_body_from_input(input, role_edit_key)?;
            ensure_non_empty_object(&body, "role edit")?;
            if body.get("tts").is_some() {
                let detail = management::get_resource(
                    &cli.api_base_url,
                    &app.api_key,
                    &app.project_id,
                    &["roles", &role_id],
                )
                .await?;
                complete_role_tts(&mut body, &detail)?;
            }
            let output = management::update_resource(
                &cli.api_base_url,
                &app.api_key,
                &app.project_id,
                &["roles", &role_id],
                body,
            )
            .await?;
            print_action_or_json(&output, json, "角色更新成功")
        }
        RoleCommand::Delete { role_id } => Err(app_config_only_operation(
            &format!("删除角色 {role_id}"),
            &app.project_id,
        )),
        RoleCommand::SetDefault { role_id, json } => {
            let output = management::update_resource(
                &cli.api_base_url,
                &app.api_key,
                &app.project_id,
                &["default-role"],
                serde_json::json!({"role_id": role_id}),
            )
            .await?;
            print_action_or_json(&output, json, "默认角色设置成功")
        }
        RoleCommand::Wakeword(args) => match args.command {
            RoleWakewordCommand::Show { role_id, json } => {
                management::require_cli_capability(
                    &cli.api_base_url,
                    &app.api_key,
                    "project.wakeup-word",
                    "角色唤醒词管理",
                )
                .await?;
                let output = management::get_resource(
                    &cli.api_base_url,
                    &app.api_key,
                    &app.project_id,
                    &["roles", &role_id, "wakeup-word"],
                )
                .await?;
                if json {
                    print_json(&output)
                } else {
                    let wakeword_id = output
                        .pointer("/data/wakeup_word_id")
                        .and_then(Value::as_str)
                        .filter(|value| !value.is_empty())
                        .context("角色尚未配置唤醒词")?;
                    let detail = management::get_resource(
                        &cli.api_base_url,
                        &app.api_key,
                        &app.project_id,
                        &["wakeup-words", wakeword_id],
                    )
                    .await?;
                    println!(
                        "{}",
                        config_view::render_role_wakeup_word(&role_id, &detail)?
                    );
                    Ok(())
                }
            }
            RoleWakewordCommand::Set {
                role_id,
                wakeword_id,
                json,
            } => {
                management::require_cli_capability(
                    &cli.api_base_url,
                    &app.api_key,
                    "project.wakeup-word",
                    "角色唤醒词管理",
                )
                .await?;
                let detail = management::get_resource(
                    &cli.api_base_url,
                    &app.api_key,
                    &app.project_id,
                    &["wakeup-words", &wakeword_id],
                )
                .await?;
                let status = detail
                    .pointer("/data/status")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if status != "ready" {
                    anyhow::bail!(
                        "唤醒词 {wakeword_id} 尚不可用（状态：{}）",
                        config_view::wakeup_word_status(status)
                    );
                }
                let output = management::update_resource(
                    &cli.api_base_url,
                    &app.api_key,
                    &app.project_id,
                    &["roles", &role_id, "wakeup-word"],
                    serde_json::json!({"wakeup_word_id": wakeword_id}),
                )
                .await?;
                print_action_or_json(&output, json, "角色唤醒词切换成功")
            }
        },
    }
}

async fn wakeword_command(cli: &Ctx, args: AppWakewordArgs, selector: AppSelector) -> Result<()> {
    let app = resolve_app(cli, selector).await?;
    management::require_cli_capability(
        &cli.api_base_url,
        &app.api_key,
        "project.wakeup-word",
        "唤醒词管理",
    )
    .await?;
    match args.command {
        WakewordCommand::List {
            page,
            page_size,
            json,
        } => {
            let output = management::list_resource(
                &cli.api_base_url,
                &app.api_key,
                &app.project_id,
                &["wakeup-words"],
                page,
                page_size,
            )
            .await?;
            if json {
                print_json(&output)
            } else {
                println!(
                    "{}",
                    config_view::render_management_wakeup_word_list(&output)?
                );
                Ok(())
            }
        }
        WakewordCommand::Show { wakeword_id, json } => {
            let output = management::get_resource(
                &cli.api_base_url,
                &app.api_key,
                &app.project_id,
                &["wakeup-words", &wakeword_id],
            )
            .await?;
            if json {
                print_json(&output)
            } else {
                println!(
                    "{}",
                    config_view::render_management_wakeup_word_detail(&output)?
                );
                Ok(())
            }
        }
        WakewordCommand::Generate {
            name,
            sensitivity,
            responses,
            description,
            yes,
            json,
        } => {
            let name = validate_wakeup_word_name(&name)?;
            let responses = wakeup_word_responses(responses, true)?;
            if description
                .as_deref()
                .is_some_and(|value| value.chars().count() > 120)
            {
                anyhow::bail!("唤醒词描述最多 120 个字符");
            }
            if !yes
                && !confirm(&format!(
                    "生成唤醒词「{name}」可能产生费用。确认提交生成任务？"
                ))?
            {
                eprintln!("已取消。");
                return Ok(());
            }
            let mut body = serde_json::json!({
                "name": name,
                "sensitivity": sensitivity,
            });
            if !responses.is_empty() {
                body["responses"] = Value::Array(responses);
            }
            if let Some(description) = description {
                body["description"] = Value::String(description);
            }
            let output = management::create_resource(
                &cli.api_base_url,
                &app.api_key,
                &app.project_id,
                &["wakeup-words"],
                body,
            )
            .await?;
            if json {
                print_json(&output)
            } else {
                println!("唤醒词生成任务已提交。");
                println!(
                    "{}",
                    config_view::render_management_wakeup_word_detail(&output)?
                );
                if let Some(id) = output.pointer("/data/id").and_then(Value::as_str) {
                    println!(
                        "\n使用 `ling app --product-id {} wakeword show {id}` 查询生成状态。",
                        app.product_id
                    );
                }
                Ok(())
            }
        }
        WakewordCommand::Responses { wakeword_id, json } => {
            let output = management::get_resource(
                &cli.api_base_url,
                &app.api_key,
                &app.project_id,
                &["wakeup-words", &wakeword_id, "responses"],
            )
            .await?;
            if json {
                print_json(&output)
            } else {
                println!("{}", config_view::render_wakeup_word_responses(&output)?);
                Ok(())
            }
        }
        WakewordCommand::SetResponses {
            wakeword_id,
            responses,
            json,
        } => {
            let responses = wakeup_word_responses(responses, false)?;
            let output = management::update_resource(
                &cli.api_base_url,
                &app.api_key,
                &app.project_id,
                &["wakeup-words", &wakeword_id, "responses"],
                serde_json::json!({"responses": responses}),
            )
            .await?;
            print_action_or_json(&output, json, "唤醒应答语更新成功")
        }
        WakewordCommand::ResetResponses { wakeword_id, json } => {
            let output = management::update_resource(
                &cli.api_base_url,
                &app.api_key,
                &app.project_id,
                &["wakeup-words", &wakeword_id, "responses"],
                serde_json::json!({"responses": [{"text": "你好"}]}),
            )
            .await?;
            print_action_or_json(&output, json, "唤醒应答语已恢复默认")
        }
        WakewordCommand::Delete {
            wakeword_id,
            yes,
            json,
        } => {
            if !yes && !confirm(&format!("确认删除唤醒词 {wakeword_id}？"))? {
                eprintln!("已取消。");
                return Ok(());
            }
            let output = management::delete_resource(
                &cli.api_base_url,
                &app.api_key,
                &app.project_id,
                &["wakeup-words", &wakeword_id],
            )
            .await?;
            print_action_or_json(&output, json, "唤醒词删除成功")
        }
    }
}

fn validate_wakeup_word_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        anyhow::bail!("唤醒词不能为空");
    }
    if name.chars().count() > 12 {
        anyhow::bail!("唤醒词最多 12 个字符");
    }
    Ok(name.to_owned())
}

fn wakeup_word_responses(responses: Vec<String>, allow_empty: bool) -> Result<Vec<Value>> {
    if responses.is_empty() && !allow_empty {
        anyhow::bail!("至少需要一条唤醒应答语");
    }
    if responses.len() > 5 {
        anyhow::bail!("唤醒应答语最多 5 条");
    }
    responses
        .into_iter()
        .map(|text| {
            let text = text.trim();
            if text.is_empty() {
                anyhow::bail!("唤醒应答语不能为空");
            }
            if text.chars().count() > 12 {
                anyhow::bail!("单条唤醒应答语最多 12 个字符");
            }
            Ok(serde_json::json!({"text": text}))
        })
        .collect()
}

fn role_detail_with_project_default(mut detail: Value, roles: &[Value], role_id: &str) -> Value {
    let is_default = roles
        .iter()
        .find(|role| role.get("id").and_then(Value::as_str) == Some(role_id))
        .and_then(|role| role.get("is_default"))
        .and_then(Value::as_bool);
    if let (Some(is_default), Some(role)) = (
        is_default,
        detail.get_mut("data").and_then(Value::as_object_mut),
    ) {
        role.insert("is_default".to_owned(), Value::Bool(is_default));
    }
    detail
}

fn complete_role_tts(body: &mut Value, detail: &Value) -> Result<()> {
    let requested = body
        .get_mut("tts")
        .and_then(Value::as_object_mut)
        .context("角色 tts 配置必须是 JSON 对象")?;
    let current = detail
        .pointer("/data/tts")
        .and_then(Value::as_object)
        .context("角色详情缺少 data.tts，无法进行部分音色更新")?;
    for key in ["vcn", "volume", "speed"] {
        if !requested.contains_key(key) {
            let value = current
                .get(key)
                .with_context(|| format!("角色详情缺少 data.tts.{key}"))?;
            requested.insert(key.to_owned(), value.clone());
        }
    }
    Ok(())
}

async fn ota_command(cli: &Ctx, args: OtaArgs, selector: AppSelector) -> Result<()> {
    let app = resolve_app(cli, selector).await?;
    match args.command {
        OtaCommand::List {
            page,
            page_size,
            json,
        } => {
            let output = management::list_resource(
                &cli.api_base_url,
                &app.api_key,
                &app.project_id,
                &["ota", "packages"],
                page,
                page_size,
            )
            .await?;
            if json {
                print_json(&output)
            } else {
                println!("{}", config_view::render_management_ota_list(&output)?);
                Ok(())
            }
        }
        OtaCommand::Show { package_id, json } => {
            let items = management::list_all_resource(
                &cli.api_base_url,
                &app.api_key,
                &app.project_id,
                &["ota", "packages"],
            )
            .await?;
            let item = items
                .into_iter()
                .find(|item| {
                    item.get("package_id").and_then(Value::as_str) == Some(package_id.as_str())
                })
                .with_context(|| format!("未找到 OTA 包：{package_id}"))?;
            if json {
                print_json(&item)
            } else {
                println!("{}", render_ota_package(&item));
                Ok(())
            }
        }
        OtaCommand::Upload {
            file,
            version,
            version_number,
            ota_mode,
            description,
            json,
        } => {
            let output = management::upload_ota(
                &cli.api_base_url,
                &app.api_key,
                &app.project_id,
                None,
                management::OtaForm {
                    file: Some(&file),
                    version: Some(&version),
                    version_number: Some(version_number),
                    ota_mode: Some(&ota_mode),
                    description: description.as_deref(),
                },
            )
            .await?;
            print_action_or_json(&output, json, "OTA 包上传成功")
        }
        OtaCommand::Edit {
            package_id,
            file,
            version,
            description,
            json,
        } => {
            if file.is_none() && version.is_none() && description.is_none() {
                anyhow::bail!("ota edit 至少需要一个修改参数");
            }
            let output = management::upload_ota(
                &cli.api_base_url,
                &app.api_key,
                &app.project_id,
                Some(&package_id),
                management::OtaForm {
                    file: file.as_deref(),
                    version: version.as_deref(),
                    version_number: None,
                    ota_mode: None,
                    description: description.as_deref(),
                },
            )
            .await?;
            print_action_or_json(&output, json, "OTA 包更新成功")
        }
        OtaCommand::Publish { package_id } => Err(application_only_operation(
            &format!("正式发布 OTA 包 {package_id}"),
            &app.product_id,
            "固件升级",
        )),
        OtaCommand::Revoke { package_id } => Err(application_only_operation(
            &format!("撤销 OTA 包 {package_id}"),
            &app.product_id,
            "固件升级",
        )),
        OtaCommand::Delete {
            package_id,
            yes,
            json,
        } => {
            if !yes
                && !confirm(&format!(
                    "仅未正式发布的 OTA 包可删除。确认删除 OTA 包 {package_id}？"
                ))?
            {
                eprintln!("已取消。");
                return Ok(());
            }
            let output = management::delete_resource(
                &cli.api_base_url,
                &app.api_key,
                &app.project_id,
                &["ota", "packages", &package_id],
            )
            .await?;
            print_action_or_json(&output, json, "OTA 包删除成功")
        }
        OtaCommand::Whitelist { command } => match command {
            OtaWhitelistCommand::List {
                page,
                page_size,
                json,
            } => {
                let output = management::list_resource(
                    &cli.api_base_url,
                    &app.api_key,
                    &app.project_id,
                    &["ota", "whitelist"],
                    page,
                    page_size,
                )
                .await?;
                if json {
                    print_json(&output)
                } else {
                    println!("{}", config_view::render_management_ota_whitelist(&output)?);
                    Ok(())
                }
            }
            OtaWhitelistCommand::Add {
                device_id,
                self_device,
                json,
            } => {
                let device_id = if self_device {
                    config::LingConfig::show_device_id()?
                } else {
                    device_id.expect("clap requires a Device ID or --self")
                };
                let output = management::action_resource(
                    &cli.api_base_url,
                    &app.api_key,
                    &app.project_id,
                    &["ota", "whitelist", &device_id],
                    None,
                )
                .await?;
                print_action_or_json(&output, json, "OTA 白名单设备添加成功")
            }
            OtaWhitelistCommand::Delete { device_id, json } => {
                let output = management::delete_resource(
                    &cli.api_base_url,
                    &app.api_key,
                    &app.project_id,
                    &["ota", "whitelist", &device_id],
                )
                .await?;
                print_action_or_json(&output, json, "OTA 白名单设备删除成功")
            }
        },
    }
}

async fn app_kb_command(cli: &Ctx, args: AppKbArgs, selector: AppSelector) -> Result<()> {
    let ResolvedApp {
        api_key,
        project_id,
        ..
    } = resolve_app(cli, selector).await?;
    match args.command {
        AppKbCommand::List {
            page,
            page_size,
            json,
        } => {
            let output = management::list_resource(
                &cli.api_base_url,
                &api_key,
                &project_id,
                &["knowledge-bases"],
                page,
                page_size,
            )
            .await?;
            if json {
                print_json(&output)
            } else {
                println!("{}", config_view::render_management_app_kbs(&output)?);
                Ok(())
            }
        }
        AppKbCommand::Link { index_id, json } => {
            replace_project_kb(cli, &api_key, &project_id, &index_id, true, json).await
        }
        AppKbCommand::Unlink { index_id, json } => {
            replace_project_kb(cli, &api_key, &project_id, &index_id, false, json).await
        }
    }
}

async fn replace_project_kb(
    cli: &Ctx,
    api_key: &str,
    project_id: &str,
    index_id: &str,
    link: bool,
    json: bool,
) -> Result<()> {
    let current =
        management::list_all_resource(&cli.api_base_url, api_key, project_id, &["knowledge-bases"])
            .await?;
    let mut ids = current
        .iter()
        .filter_map(|item| {
            item.get("index_id")
                .or_else(|| item.get("id"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect::<Vec<_>>();
    if link {
        if !ids.iter().any(|id| id == index_id) {
            ids.push(index_id.to_owned());
        }
    } else {
        ids.retain(|id| id != index_id);
    }
    let output = management::update_resource(
        &cli.api_base_url,
        api_key,
        project_id,
        &["knowledge-bases"],
        serde_json::json!({"knowledge_base_ids": ids}),
    )
    .await?;
    print_action_or_json(
        &output,
        json,
        if link {
            "知识库关联成功"
        } else {
            "知识库解除关联成功"
        },
    )
}

async fn chain_command(cli: &Ctx, args: ChainArgs, selector: AppSelector) -> Result<()> {
    let app = resolve_app(cli, selector).await?;
    let detail = get_app_detail(cli, &app).await?;
    let app_id = ling_plugin_app::project_app_id(config_view::project_data(&detail))
        .context("应用详情缺少 App ID，无法管理测试链路")?;
    match args.command {
        ChainCommand::Show { json } => {
            let output =
                management::get_framework_agent_version(&cli.api_base_url, &app.api_key, &app_id)
                    .await?;
            if json {
                print_json(&output)
            } else {
                println!("{}", config_view::render_framework_agent_version(&output)?);
                Ok(())
            }
        }
        ChainCommand::Versions {
            page,
            page_size,
            json,
        } => {
            let output = management::list_framework_agent_versions(
                &cli.api_base_url,
                &app.api_key,
                &app_id,
                page,
                page_size,
            )
            .await?;
            if json {
                return print_json(&output);
            }
            let current =
                management::get_framework_agent_version(&cli.api_base_url, &app.api_key, &app_id)
                    .await?;
            let current_version = config_view::framework_agent_version(&current)?;
            println!(
                "{}",
                config_view::render_framework_agent_versions(&output, current_version)?
            );
            Ok(())
        }
        ChainCommand::Set { target } => {
            let (version, json, message) = match target {
                ChainSetCommand::Managed { json } => (
                    None,
                    json,
                    "测试链路已切换为 managed（官方最新版本）".to_owned(),
                ),
                ChainSetCommand::Custom { version, json } => {
                    let version = normalize_framework_agent_version(&version)?;
                    (
                        Some(version.clone()),
                        json,
                        format!("测试链路已切换为 custom（{version}）"),
                    )
                }
            };
            let output = management::set_framework_agent_version(
                &cli.api_base_url,
                &app.api_key,
                &app_id,
                version.as_deref(),
            )
            .await?;
            print_action_or_json(&output, json, &message)
        }
    }
}

fn normalize_framework_agent_version(value: &str) -> Result<String> {
    let value = value.trim();
    let raw = value.strip_prefix('v').unwrap_or(value);
    let parts = raw.split('.').collect::<Vec<_>>();
    if parts.len() != 3
        || parts
            .iter()
            .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        anyhow::bail!("Agent 版本必须为 vX.Y.Z，例如 v0.0.8");
    }
    Ok(format!("v{raw}"))
}

async fn lexicon_command(cli: &Ctx, args: LexiconArgs, selector: AppSelector) -> Result<()> {
    let ResolvedApp {
        api_key,
        project_id,
        ..
    } = resolve_app(cli, selector).await?;
    match args.command {
        LexiconCommand::List {
            page,
            page_size,
            json,
        } => {
            let output = management::list_resource(
                &cli.api_base_url,
                &api_key,
                &project_id,
                &["hotwords"],
                page,
                page_size,
            )
            .await?;
            if json {
                print_json(&output)
            } else {
                println!("{}", config_view::render_management_lexicon(&output)?);
                Ok(())
            }
        }
        LexiconCommand::Add { word, json } => {
            let word = normalize_hotword(&word)?;
            let current = management::list_all_resource(
                &cli.api_base_url,
                &api_key,
                &project_id,
                &["hotwords"],
            )
            .await?;
            if current
                .iter()
                .filter_map(hotword_word)
                .any(|existing| existing == word)
            {
                anyhow::bail!("专业词汇已存在：{word}");
            }
            validate_hotwords_total(hotwords_total_chars(&current) + word.chars().count())?;
            let output = management::create_resource(
                &cli.api_base_url,
                &api_key,
                &project_id,
                &["hotwords"],
                serde_json::json!({"word": word}),
            )
            .await?;
            print_action_or_json(&output, json, "专业词汇添加成功")
        }
        LexiconCommand::Import { file, json } => {
            let content = std::fs::read_to_string(&file)
                .with_context(|| format!("读取专业词汇文件失败：{}", file.display()))?;
            let entries = lexicon_import_entries(&content);
            if entries.is_empty() {
                anyhow::bail!("专业词汇文件没有可导入的非空行：{}", file.display());
            }
            for entry in &entries {
                validate_hotword(&entry.word)?;
            }

            let current = management::list_all_resource(
                &cli.api_base_url,
                &api_key,
                &project_id,
                &["hotwords"],
            )
            .await?;
            let existing_words = current
                .iter()
                .filter_map(hotword_word)
                .collect::<HashSet<_>>();
            let new_words = entries
                .iter()
                .map(|entry| entry.word.as_str())
                .filter(|word| !existing_words.contains(word))
                .collect::<HashSet<_>>();
            validate_hotwords_total(
                hotwords_total_chars(&current)
                    + new_words
                        .iter()
                        .map(|word| word.chars().count())
                        .sum::<usize>(),
            )?;
            let mut existing = current
                .iter()
                .filter_map(hotword_word)
                .map(str::to_owned)
                .collect::<HashSet<_>>();
            let mut seen = HashSet::new();
            let mut items = Vec::with_capacity(entries.len());
            let mut succeeded = 0_u64;
            let mut duplicates = 0_u64;
            let mut failed = 0_u64;

            for entry in entries {
                let duplicate_reason = if !seen.insert(entry.word.clone()) {
                    Some("file")
                } else if existing.contains(&entry.word) {
                    Some("existing")
                } else {
                    None
                };
                if let Some(reason) = duplicate_reason {
                    duplicates += 1;
                    items.push(serde_json::json!({
                        "line": entry.line,
                        "word": entry.word,
                        "status": "duplicate",
                        "reason": reason,
                    }));
                    continue;
                }

                match management::create_resource(
                    &cli.api_base_url,
                    &api_key,
                    &project_id,
                    &["hotwords"],
                    serde_json::json!({"word": entry.word}),
                )
                .await
                {
                    Ok(output) => {
                        succeeded += 1;
                        existing.insert(entry.word.clone());
                        items.push(serde_json::json!({
                            "line": entry.line,
                            "word": entry.word,
                            "status": "created",
                            "data": output.get("data").cloned().unwrap_or(Value::Null),
                        }));
                    }
                    Err(error) => {
                        failed += 1;
                        items.push(serde_json::json!({
                            "line": entry.line,
                            "word": entry.word,
                            "status": "failed",
                            "error": format!("{error:#}"),
                        }));
                    }
                }
            }

            let summary = serde_json::json!({
                "total": items.len(),
                "succeeded": succeeded,
                "duplicates": duplicates,
                "failed": failed,
                "items": items,
            });
            if json {
                print_json(&summary)?;
            } else {
                for item in summary["items"].as_array().into_iter().flatten() {
                    let line = item["line"].as_u64().unwrap_or_default();
                    let word = item["word"].as_str().unwrap_or("-");
                    match item["status"].as_str() {
                        Some("created") => println!("+ 第 {line} 行：{word}"),
                        Some("duplicate") => println!("= 第 {line} 行：{word}（重复，已跳过）"),
                        Some("failed") => eprintln!(
                            "! 第 {line} 行：{word}：{}",
                            item["error"].as_str().unwrap_or("未知错误")
                        ),
                        _ => {}
                    }
                }
                eprintln!("导入完成：成功 {succeeded}，重复 {duplicates}，失败 {failed}");
            }
            if failed > 0 {
                anyhow::bail!("有 {failed} 条专业词汇导入失败");
            }
            Ok(())
        }
        LexiconCommand::Edit {
            hotword_id,
            word,
            json,
        } => {
            let word = normalize_hotword(&word)?;
            let current = management::list_all_resource(
                &cli.api_base_url,
                &api_key,
                &project_id,
                &["hotwords"],
            )
            .await?;
            let current_entry = current
                .iter()
                .find(|item| hotword_id_of(item) == Some(hotword_id.as_str()))
                .with_context(|| format!("未找到专业词汇 ID：{hotword_id}"))?;
            if current.iter().any(|item| {
                hotword_id_of(item) != Some(hotword_id.as_str())
                    && hotword_word(item) == Some(word.as_str())
            }) {
                anyhow::bail!("专业词汇已存在：{word}");
            }
            let old_chars = hotword_word(current_entry)
                .map(|word| word.chars().count())
                .unwrap_or_default();
            validate_hotwords_total(
                hotwords_total_chars(&current) - old_chars + word.chars().count(),
            )?;
            let output = management::update_resource(
                &cli.api_base_url,
                &api_key,
                &project_id,
                &["hotwords", &hotword_id],
                serde_json::json!({"word": word}),
            )
            .await?;
            print_action_or_json(&output, json, "专业词汇修改成功")
        }
        LexiconCommand::Delete { hotword_id } => Err(app_config_only_operation(
            &format!("删除专业词汇 {hotword_id}"),
            &project_id,
        )),
    }
}

#[derive(Debug, PartialEq, Eq)]
struct LexiconImportEntry {
    line: usize,
    word: String,
}

fn lexicon_import_entries(content: &str) -> Vec<LexiconImportEntry> {
    content
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let word = line.trim();
            (!word.is_empty()).then(|| LexiconImportEntry {
                line: index + 1,
                word: word.to_owned(),
            })
        })
        .collect()
}

fn normalize_hotword(value: &str) -> Result<String> {
    let word = value.trim();
    validate_hotword(word)?;
    Ok(word.to_owned())
}

fn validate_hotword(word: &str) -> Result<()> {
    if word.is_empty() {
        anyhow::bail!("专业词汇不能为空");
    }
    let chars = word.chars().count();
    if chars > MAX_HOTWORD_CHARS {
        anyhow::bail!("单个专业词汇最多 {MAX_HOTWORD_CHARS} 个字符，当前为 {chars} 个字符：{word}");
    }
    Ok(())
}

fn validate_hotwords_total(chars: usize) -> Result<()> {
    if chars > MAX_HOTWORDS_TOTAL_CHARS {
        anyhow::bail!(
            "所有专业词汇合计最多 {MAX_HOTWORDS_TOTAL_CHARS} 个字符，操作后将达到 {chars} 个字符"
        );
    }
    Ok(())
}

fn hotword_word(item: &Value) -> Option<&str> {
    item.get("word")
        .or_else(|| item.get("name"))
        .and_then(Value::as_str)
}

fn hotword_id_of(item: &Value) -> Option<&str> {
    item.get("id")
        .or_else(|| item.get("hotword_id"))
        .and_then(Value::as_str)
}

fn hotwords_total_chars(items: &[Value]) -> usize {
    items
        .iter()
        .filter_map(hotword_word)
        .map(|word| word.chars().count())
        .sum()
}

async fn tone_command(cli: &Ctx, args: ToneArgs, selector: AppSelector) -> Result<()> {
    let ResolvedApp {
        api_key,
        project_id,
        ..
    } = resolve_app(cli, selector).await?;
    match args.command {
        ToneCommand::Show {
            page,
            page_size,
            json,
        } => {
            let output = management::list_resource(
                &cli.api_base_url,
                &api_key,
                &project_id,
                &["prompt-tone-texts"],
                page,
                page_size,
            )
            .await?;
            if json {
                print_json(&output)
            } else {
                println!("{}", config_view::render_management_tones(&output)?);
                Ok(())
            }
        }
        ToneCommand::Edit {
            set,
            reset,
            reset_all,
            file,
            json,
        } => {
            if reset_all {
                let output = management::action_resource(
                    &cli.api_base_url,
                    &api_key,
                    &project_id,
                    &["prompt-tone-texts", "restore-default"],
                    None,
                )
                .await?;
                return print_action_or_json(&output, json, "全部提示语已恢复默认");
            }

            let texts = if let Some(file) = file {
                let value = read_json_file(&file)?;
                value.get("texts").cloned().unwrap_or(value)
            } else {
                if set.is_empty() && reset.is_empty() {
                    anyhow::bail!("tone edit 需要 --set、--reset、--reset-all 或 --file");
                }

                let assignments = parse_tone_assignments(&set)?;
                let mut texts = if assignments.is_empty() {
                    None
                } else {
                    let current = management::list_resource(
                        &cli.api_base_url,
                        &api_key,
                        &project_id,
                        &["prompt-tone-texts"],
                        1,
                        100,
                    )
                    .await?;
                    let texts = tone_texts(&current)?;
                    validate_tone_assignment_keys(&texts, &assignments)?;
                    Some(texts)
                };

                let mut restored = Vec::new();
                let mut last_restore = None;
                for key in reset {
                    match management::action_resource(
                        &cli.api_base_url,
                        &api_key,
                        &project_id,
                        &["prompt-tone-texts", "restore-default"],
                        Some(serde_json::json!({"key": key})),
                    )
                    .await
                    {
                        Ok(output) => {
                            restored.push(key);
                            last_restore = Some(output);
                        }
                        Err(error) => {
                            let applied = if restored.is_empty() {
                                "无".to_owned()
                            } else {
                                restored.join(", ")
                            };
                            anyhow::bail!(
                                "提示语 {key} 恢复默认失败；此前已恢复：{applied}；未执行 --set：{error:#}"
                            );
                        }
                    }
                }

                if assignments.is_empty() {
                    return print_action_or_json(
                        last_restore
                            .as_ref()
                            .context("tone edit 没有产生任何恢复结果")?,
                        json,
                        "提示语已恢复默认",
                    );
                }
                if let Some(output) = last_restore {
                    texts = Some(tone_texts(&output)?);
                }
                let mut texts = texts.context("提示语列表不可用")?;
                apply_tone_assignments(&mut texts, &assignments)?;
                Value::Array(texts)
            };
            if !texts.is_array() {
                anyhow::bail!("tone --file 必须包含 JSON 数组或 {{\"texts\": [...]}}");
            }
            let output = management::update_resource(
                &cli.api_base_url,
                &api_key,
                &project_id,
                &["prompt-tone-texts"],
                serde_json::json!({"texts": texts}),
            )
            .await?;
            print_action_or_json(&output, json, "提示语更新成功")
        }
    }
}

fn tone_texts(value: &Value) -> Result<Vec<Value>> {
    value
        .get("data")
        .and_then(Value::as_array)
        .context("提示语响应缺少 data 数组")?
        .iter()
        .map(|item| {
            let key = item
                .get("key")
                .and_then(Value::as_str)
                .context("提示语列表项缺少 key")?;
            let text = item
                .get("text")
                .and_then(Value::as_str)
                .context("提示语列表项缺少 text")?;
            Ok(serde_json::json!({"key": key, "text": text}))
        })
        .collect()
}

fn parse_tone_assignments(values: &[String]) -> Result<Vec<(String, String)>> {
    values
        .iter()
        .map(|assignment| {
            let (key, text) = split_assignment(assignment)?;
            match parse_json_literal(&text) {
                Value::String(text) if !text.trim().is_empty() => Ok((key, text)),
                Value::String(_) => anyhow::bail!("提示语文案不能为空：{key}"),
                _ => anyhow::bail!("提示语文案必须是字符串：{key}"),
            }
        })
        .collect()
}

fn validate_tone_assignment_keys(texts: &[Value], assignments: &[(String, String)]) -> Result<()> {
    for (key, _) in assignments {
        if !texts
            .iter()
            .any(|item| item.get("key").and_then(Value::as_str) == Some(key.as_str()))
        {
            anyhow::bail!("当前提示语配置中不存在 key：{key}");
        }
    }
    Ok(())
}

fn apply_tone_assignments(texts: &mut [Value], assignments: &[(String, String)]) -> Result<()> {
    validate_tone_assignment_keys(texts, assignments)?;
    for (key, text) in assignments {
        let item = texts
            .iter_mut()
            .find(|item| item.get("key").and_then(Value::as_str) == Some(key.as_str()))
            .expect("tone assignment keys were validated");
        item.as_object_mut()
            .context("提示语列表项不是 JSON 对象")?
            .insert("text".to_owned(), Value::String(text.clone()));
    }
    Ok(())
}

async fn mcp_command(cli: &Ctx, args: McpArgs, selector: AppSelector) -> Result<()> {
    let ResolvedApp {
        api_key,
        project_id,
        ..
    } = resolve_app(cli, selector).await?;
    match args.command {
        McpCommand::List {
            page,
            page_size,
            json,
        } => {
            let output = management::list_resource(
                &cli.api_base_url,
                &api_key,
                &project_id,
                &["mcp-servers"],
                page,
                page_size,
            )
            .await?;
            if json {
                print_json(&config_view::redact_mcp_credentials(&output))
            } else {
                println!("{}", config_view::render_management_mcps(&output)?);
                Ok(())
            }
        }
        McpCommand::Show { mcp_id, json } => {
            let items = management::list_all_resource(
                &cli.api_base_url,
                &api_key,
                &project_id,
                &["mcp-servers"],
            )
            .await?;
            let item = items
                .into_iter()
                .find(|item| item.get("id").and_then(Value::as_str) == Some(mcp_id.as_str()))
                .with_context(|| format!("未找到 ID 为 {mcp_id} 的 MCP 服务器"))?;
            let output = serde_json::json!({"data": item});
            if json {
                print_json(&config_view::redact_mcp_credentials(&output))
            } else {
                println!("{}", config_view::render_management_mcp_detail(&output)?);
                Ok(())
            }
        }
        McpCommand::Add {
            name,
            server_id,
            transport_type,
            url,
            description,
            authorization,
            enabled,
            json,
        } => {
            let mut body = serde_json::json!({
                "name": name,
                "server_id": server_id,
                "transport_type": transport_type,
                "url": url,
                "enabled": enabled,
            });
            if let Some(description) = description {
                body["description"] = Value::String(description);
            }
            if let Some(authorization) = authorization {
                body["authorization"] = Value::String(authorization);
            }
            let output = management::create_resource(
                &cli.api_base_url,
                &api_key,
                &project_id,
                &["mcp-servers"],
                body,
            )
            .await?;
            print_action_or_json(&output, json, "MCP 服务器已添加")
        }
        McpCommand::Edit {
            mcp_id,
            input,
            json,
        } => {
            let body = json_body_from_input(input, |key| key.to_owned())?;
            ensure_non_empty_object(&body, "mcp edit")?;
            let output = management::update_resource(
                &cli.api_base_url,
                &api_key,
                &project_id,
                &["mcp-servers", &mcp_id],
                body,
            )
            .await?;
            print_action_or_json(&output, json, "MCP 服务器更新成功")
        }
        McpCommand::Enable { mcp_id, json } => {
            let output = management::update_resource(
                &cli.api_base_url,
                &api_key,
                &project_id,
                &["mcp-servers", &mcp_id],
                serde_json::json!({"enabled": true}),
            )
            .await?;
            print_action_or_json(&output, json, "MCP 服务器已启用")
        }
        McpCommand::Disable { mcp_id, json } => {
            let output = management::update_resource(
                &cli.api_base_url,
                &api_key,
                &project_id,
                &["mcp-servers", &mcp_id],
                serde_json::json!({"enabled": false}),
            )
            .await?;
            print_action_or_json(&output, json, "MCP 服务器已停用")
        }
        McpCommand::Delete { mcp_id } => Err(app_config_only_operation(
            &format!("删除 MCP 服务器 {mcp_id}"),
            &project_id,
        )),
    }
}

async fn config_command(cli: &Ctx, args: ConfigArgs, selector: AppSelector) -> Result<()> {
    let ResolvedApp {
        api_key,
        project_id,
        ..
    } = resolve_app(cli, selector).await?;
    match args.command {
        ConfigCommand::Show { json } => {
            let (project, interaction, prompt, model) = tokio::try_join!(
                management::get_project(&cli.api_base_url, &api_key, &project_id),
                management::get_resource(
                    &cli.api_base_url,
                    &api_key,
                    &project_id,
                    &["interaction-mode"],
                ),
                management::get_resource(
                    &cli.api_base_url,
                    &api_key,
                    &project_id,
                    &["agent", "prompt"],
                ),
                management::get_resource(
                    &cli.api_base_url,
                    &api_key,
                    &project_id,
                    &["agent", "model"],
                ),
            )?;
            let output = config_show_output(&project, &interaction, &prompt, &model);
            if json {
                print_json(&output)
            } else {
                println!("{}", config_view::render_management_config(&output)?);
                Ok(())
            }
        }
        ConfigCommand::Edit { input, json } => {
            let body = json_body_from_input(input, |key| key.replace('-', "_"))?;
            ensure_non_empty_object(&body, "config edit")?;
            let mut fields = body
                .as_object()
                .cloned()
                .context("config edit 请求必须是 JSON 对象")?;
            let unsupported = fields
                .keys()
                .filter(|key| !APP_CONFIG_EDITABLE_KEYS.contains(&key.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            if !unsupported.is_empty() {
                anyhow::bail!(
                    "服务端当前不支持这些 app config 字段：{}。可用字段：{}",
                    unsupported.join(", "),
                    APP_CONFIG_EDITABLE_KEYS.join("、")
                );
            }
            // 服务端按分组接收更新，回显仍用 config show 的字段名。
            let updated_keys = APP_CONFIG_EDITABLE_KEYS
                .iter()
                .filter(|key| fields.contains_key(**key))
                .copied()
                .collect::<Vec<_>>();
            let mut results = serde_json::Map::new();

            if let Some(project) = take_project_config(&mut fields)? {
                let output =
                    management::update_project(&cli.api_base_url, &api_key, &project_id, project)
                        .await?;
                results.insert("project".to_owned(), output);
            }

            if let Some(value) = fields.remove("interaction_mode") {
                let mode = interaction_mode_value(&value)?;
                let output = management::update_resource(
                    &cli.api_base_url,
                    &api_key,
                    &project_id,
                    &["interaction-mode"],
                    serde_json::json!({"interaction_mode": mode}),
                )
                .await?;
                results.insert("interaction".to_owned(), output);
            }

            if let Some(system_prompt) = fields.remove("system_prompt") {
                if !system_prompt.is_string() {
                    anyhow::bail!("system_prompt 必须是字符串");
                }
                let output = management::update_resource(
                    &cli.api_base_url,
                    &api_key,
                    &project_id,
                    &["agent", "prompt"],
                    serde_json::json!({"system_prompt": system_prompt}),
                )
                .await?;
                results.insert("prompt".to_owned(), output);
            }

            let mut model = serde_json::Map::new();
            for key in ["protocol", "endpoint", "authorization", "model"] {
                if let Some(value) = fields.remove(key) {
                    model.insert(key.to_owned(), value);
                }
            }
            if !model.is_empty() {
                let output = management::update_resource(
                    &cli.api_base_url,
                    &api_key,
                    &project_id,
                    &["agent", "model"],
                    Value::Object(model),
                )
                .await?;
                results.insert("model".to_owned(), output);
            }

            debug_assert!(fields.is_empty());
            let output = Value::Object(results);
            if json {
                print_json(&output)
            } else {
                eprintln!("配置已更新：{}", updated_keys.join("、"));
                Ok(())
            }
        }
        ConfigCommand::ResetModel { json } => {
            let output = management::action_resource(
                &cli.api_base_url,
                &api_key,
                &project_id,
                &["agent", "model", "restore-default"],
                None,
            )
            .await?;
            print_action_or_json(&output, json, "模型配置已恢复默认")
        }
        ConfigCommand::TestModel {
            endpoint,
            model,
            authorization,
            json,
        } => {
            let mut body = serde_json::json!({"endpoint": endpoint, "model": model});
            if let Some(authorization) = authorization {
                body["authorization"] = Value::String(authorization);
            }
            let output = management::action_resource(
                &cli.api_base_url,
                &api_key,
                &project_id,
                &["agent", "model", "test"],
                Some(body),
            )
            .await?;
            print_action_or_json(&output, json, "模型连接测试成功")
        }
    }
}

fn take_project_config(fields: &mut serde_json::Map<String, Value>) -> Result<Option<Value>> {
    let mut project = serde_json::Map::new();
    if let Some(value) = fields.remove("name") {
        let name = value.as_str().context("name 必须是字符串")?.trim();
        if name.is_empty() {
            anyhow::bail!("name 不能为空");
        }
        project.insert("name".to_owned(), Value::String(name.to_owned()));
    }
    if let Some(value) = fields.remove("description") {
        let description = value.as_str().context("description 必须是字符串")?.trim();
        project.insert(
            "description".to_owned(),
            Value::String(description.to_owned()),
        );
    }
    Ok((!project.is_empty()).then_some(Value::Object(project)))
}

fn interaction_mode_value(value: &Value) -> Result<i64> {
    match value {
        Value::Number(number) => number
            .as_i64()
            .filter(|value| matches!(value, 0..=2))
            .context("interaction_mode 只允许 0、1、2"),
        Value::String(mode) => match mode.as_str() {
            "oneshot" => Ok(0),
            "full-duplex" | "full_duplex" => Ok(1),
            "half-duplex" | "half_duplex" => Ok(2),
            _ => anyhow::bail!(
                "interaction_mode 只允许 oneshot、full-duplex、half-duplex 或 0、1、2"
            ),
        },
        _ => anyhow::bail!("interaction_mode 只允许 oneshot、full-duplex、half-duplex 或 0、1、2"),
    }
}

fn config_show_output(
    project: &Value,
    interaction: &Value,
    prompt: &Value,
    model: &Value,
) -> Value {
    let mode = interaction
        .pointer("/data/interaction_mode")
        .and_then(Value::as_i64)
        .map(config_view::interact_mode_label)
        .map(str::to_owned);
    let field = |response: &Value, key: &str| {
        response
            .get("data")
            .and_then(|data| data.get(key))
            .cloned()
            .unwrap_or(Value::Null)
    };
    let authorization_configured = model
        .pointer("/data/authorization_configured")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    serde_json::json!({
        "name": field(project, "name"),
        "description": field(project, "description"),
        "interaction_mode": mode,
        "system_prompt": field(prompt, "system_prompt"),
        "protocol": field(model, "protocol"),
        "endpoint": field(model, "endpoint"),
        "model": field(model, "model"),
        "authorization_configured": authorization_configured,
        "editable_fields": {
            "name": {
                "type": "string",
                "max_length": 30,
                "non_empty": true
            },
            "description": {
                "type": "string",
                "max_length": 60,
                "empty_clears": true
            },
            "interaction_mode": {
                "type": "enum",
                "values": ["oneshot", "full-duplex", "half-duplex"]
            },
            "system_prompt": {
                "type": "string",
                "max_length": 20000,
                "empty_restores_default": true
            },
            "protocol": {
                "type": "enum",
                "values": ["chat_completions"]
            },
            "endpoint": {
                "type": "url",
                "max_length": 2048
            },
            "model": {
                "type": "string",
                "max_length": 256
            },
            "authorization": {
                "type": "string",
                "max_length": 8192,
                "write_only": true
            }
        },
    })
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
                eprintln!("已创建知识库「{name}」，index_id: {index_id}");
                Ok(())
            }
        }
        KbCommand::Delete { index_id, .. } => Err(web_only_operation(
            &format!("删除知识库 {index_id}"),
            PLATFORM_KB_URL,
        )),
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
            KbDocCommand::Add { name, url, json } => {
                let output =
                    ling_plugin_kb::add_document(api_base_url, &api_key, &index_id, &name, &url)
                        .await?;
                print_action_or_json(&output, json, "知识库文档添加成功")
            }
            KbDocCommand::Delete { doc_ids } => Err(web_only_operation(
                &format!("删除知识库 {index_id} 中的 {} 个文档", doc_ids.len()),
                &kb_detail_url(&index_id),
            )),
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
            let matched = output
                .get("data")
                .and_then(Value::as_array)
                .is_some_and(|items| !items.is_empty());
            if !matched {
                if json {
                    print_json(&output)?;
                }
                anyhow::bail!("未检索到相关知识点；请调整查询文本、--limit 或 --threshold")
            }
            if json {
                print_json(&output)?;
            } else {
                println!("{}", ling_plugin_kb::render_query(&output)?);
            }
            Ok(())
        }
    }
}
async fn wiki_command(args: WikiArgs) -> Result<()> {
    match args.command {
        WikiCommand::Search { keywords, json } => {
            let keyword_count = keywords
                .iter()
                .filter(|keyword| !keyword.trim().is_empty())
                .count();
            if json {
                let output = ling_plugin_wiki::search(DOCS_BASE_URL, &keywords).await?;
                print_json(&output)
            } else if keyword_count > 1 {
                let groups = ling_plugin_wiki::search_grouped(DOCS_BASE_URL, &keywords).await?;
                println!("{}", ling_plugin_wiki::render_search_groups(&groups));
                Ok(())
            } else {
                let output = ling_plugin_wiki::search(DOCS_BASE_URL, &keywords).await?;
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

impl AppSelector {
    fn with_positional_product(mut self, positional: Option<String>) -> Result<Self> {
        let Some(positional) = clean_identifier(positional) else {
            return Ok(self);
        };
        if self.product_id.is_some() || self.project_id.is_some() || self.app_id.is_some() {
            anyhow::bail!(
                "inspect 的位置参数不能与 --product-id、--project-id 或 --app-id 同时使用"
            );
        }
        self.product_id = Some(positional);
        Ok(self)
    }

    fn is_empty(&self) -> bool {
        self.product_id.is_none() && self.project_id.is_none() && self.app_id.is_none()
    }
}

fn ensure_no_app_selector(selector: &AppSelector, command: &str) -> Result<()> {
    if selector.is_empty() {
        return Ok(());
    }
    anyhow::bail!(
        "`ling app {command}` 不针对单个应用，不能传 --product-id、--project-id 或 --app-id"
    )
}

/// 不针对单个应用的子命令，由 [`ensure_no_app_selector`] 拒绝应用标识。
const TARGETLESS_APP_COMMANDS: [&str; 4] = ["list", "create", "build", "trace"];

const APP_SELECTOR_ARGS: [(&str, &str); 3] = [
    ("product_id", "product-id"),
    ("project_id", "project-id"),
    ("app_id", "app-id"),
];

/// 应用标识是 `ling app` 的全局参数，clap 会展示在每个子命令的帮助里。
/// 用同名隐藏参数覆盖 [`TARGETLESS_APP_COMMANDS`]，取值仍落在 `ling app`
/// 层交给运行时守卫。
fn cli_command() -> clap::Command {
    Cli::command().mut_subcommand("app", |app| {
        TARGETLESS_APP_COMMANDS.iter().fold(app, |app, name| {
            app.mut_subcommand(name, |command| {
                APP_SELECTOR_ARGS
                    .iter()
                    .fold(command, |command, (id, long)| {
                        command.arg(
                            clap::Arg::new(*id)
                                .long(*long)
                                .action(clap::ArgAction::Set)
                                .hide(true),
                        )
                    })
            })
        })
    })
}

async fn explicit_product_id(cli: &Ctx, selector: AppSelector) -> Result<Option<String>> {
    if let Some(product_id) = clean_identifier(selector.product_id.clone()) {
        return Ok(Some(product_id));
    }
    if selector.is_empty() {
        return Ok(None);
    }
    Ok(Some(resolve_app(cli, selector).await?.product_id))
}

async fn resolve_app(cli: &Ctx, mut selector: AppSelector) -> Result<ResolvedApp> {
    selector.product_id = clean_identifier(selector.product_id);
    selector.project_id = clean_identifier(selector.project_id);
    selector.app_id = clean_identifier(selector.app_id);
    if selector.is_empty() {
        selector.product_id = std::env::current_dir()
            .ok()
            .and_then(|cwd| ling_plugin_app_project::project::read_product_id(&cwd));
    }

    let api_key = resolve_api_key()?;
    if let Some(project_id) = selector.project_id {
        match management::get_project(&cli.api_base_url, &api_key, &project_id).await {
            Ok(detail) => {
                return resolved_from_project(
                    &api_key,
                    &project_id,
                    config_view::project_data(&detail),
                );
            }
            Err(detail_error) => {
                let projects =
                    ling_plugin_app::list_all_projects(&cli.api_base_url, &api_key).await?;
                let project = projects
                    .iter()
                    .find(|project| {
                        ling_plugin_app::project_id(project).as_deref()
                            == Some(project_id.as_str())
                    })
                    .with_context(|| {
                        format!(
                            "Project ID {project_id} 的详情接口失败，应用列表中也未找到该应用：{detail_error}"
                        )
                    })?;
                return resolved_from_list_entry(&api_key, project);
            }
        }
    }

    if let Some(app_id) = selector.app_id {
        let projects = ling_plugin_app::list_all_projects(&cli.api_base_url, &api_key).await?;
        let project = projects
            .iter()
            .find(|project| {
                ling_plugin_app::project_app_id(project).as_deref() == Some(app_id.as_str())
            })
            .with_context(|| format!("未找到 App ID 为 {app_id} 且已关联 Product ID 的应用"))?;
        return resolved_from_list_entry(&api_key, project);
    }

    let product_id = selector.product_id.ok_or_else(|| {
        anyhow!(
            "未指定应用：请传 --product-id、--project-id 或 --app-id，或在含 product_id 的 listenai.toml 项目目录内执行"
        )
    })?;

    match management::resolve_project_id(&cli.api_base_url, &api_key, &product_id).await {
        Ok(project_id) => Ok(ResolvedApp {
            api_key,
            product_id,
            project_id,
        }),
        Err(resolve_error) => {
            let projects = ling_plugin_app::list_all_projects(&cli.api_base_url, &api_key).await?;
            let project = projects
                .iter()
                .find(|project| {
                    ling_plugin_app::project_product_id(project).as_deref()
                        == Some(product_id.as_str())
                })
                .with_context(|| {
                    format!(
                        "Product ID {product_id} 的转换接口失败，应用列表中也未找到该应用：{resolve_error}"
                    )
                })?;
            resolved_from_list_entry(&api_key, project)
        }
    }
}

fn resolved_from_list_entry(api_key: &str, project: &Value) -> Result<ResolvedApp> {
    let project_id = ling_plugin_app::project_id(project).context("应用列表项缺少 Project ID")?;
    resolved_from_project(api_key, &project_id, project)
}

fn resolved_from_project(api_key: &str, project_id: &str, project: &Value) -> Result<ResolvedApp> {
    let product_id = ling_plugin_app::project_product_id(project).with_context(|| {
        format!("Project ID {project_id} 没有关联 Product ID，不能由 ling 管理")
    })?;
    Ok(ResolvedApp {
        api_key: api_key.to_owned(),
        product_id,
        project_id: project_id.to_owned(),
    })
}

async fn get_app_detail(cli: &Ctx, app: &ResolvedApp) -> Result<Value> {
    match management::get_project(&cli.api_base_url, &app.api_key, &app.project_id).await {
        Ok(detail) => Ok(detail),
        Err(project_error) => {
            ling_plugin_app::inspect_product(&cli.api_base_url, &app.api_key, &app.product_id)
                .await
                .with_context(|| {
                    format!(
                        "Project ID 详情接口不可用，按 Product ID 兼容读取也失败：{project_error}"
                    )
                })
        }
    }
}

fn clean_identifier(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn normalize_product_secret(value: Option<String>) -> Result<Option<String>> {
    let Some(secret) = clean_identifier(value) else {
        return Ok(None);
    };
    if secret.contains('*') {
        anyhow::bail!("--product-secret 必须是完整产品密钥，不能使用脱敏后的 previewSecret");
    }
    Ok(Some(secret))
}

fn direct_request_product_id(selector: &AppSelector) -> Option<String> {
    if clean_identifier(selector.project_id.clone()).is_some()
        || clean_identifier(selector.app_id.clone()).is_some()
    {
        return None;
    }
    clean_identifier(selector.product_id.clone()).or_else(|| {
        std::env::current_dir()
            .ok()
            .and_then(|cwd| ling_plugin_app_project::project::read_product_id(&cwd))
    })
}

fn platform_write_unavailable(feature: &str, url: &str) -> anyhow::Error {
    anyhow!(
        "「{feature}」的平台开放 API 尚未上线，请暂时在平台网页端操作：{url}\n\
         平台打通 API Key 授权链路后，ling 将在后续版本启用此命令。"
    )
}

fn web_only_operation(feature: &str, url: &str) -> anyhow::Error {
    anyhow!("CLI 不执行「{feature}」；请前往网页确认影响范围并操作：{url}")
}

fn app_config_url(project_id: &str) -> String {
    let mut url = reqwest::Url::parse(PLATFORM_APP_CONFIG_URL).expect("平台应用配置 URL 必须合法");
    url.query_pairs_mut().append_pair("id", project_id);
    url.into()
}

fn app_config_only_operation(feature: &str, project_id: &str) -> anyhow::Error {
    web_only_operation(feature, &app_config_url(project_id))
}

fn application_only_operation(feature: &str, product_id: &str, section: &str) -> anyhow::Error {
    anyhow!(
        "CLI 不执行「{feature}」；请打开应用列表，选择 Product ID 为 {product_id} 的应用，\
         进入「{section}」确认影响范围并操作：\n{PLATFORM_APPLICATION_URL}"
    )
}

fn device_list_guidance() -> String {
    format!(
        "CLI 暂不展示已导入设备列表。\n\
         请打开小聆平台的应用列表，选择目标应用后进入「设备管理」查看：\n\
         {PLATFORM_APPLICATION_URL}"
    )
}

fn device_enforcement_summary(enabled: bool) -> String {
    let (state, rule) = if enabled {
        ("已开启", "仅已导入的设备可以接入此应用。")
    } else {
        ("未开启", "设备无需预先导入即可接入此应用。")
    };
    format!(
        "强制白名单：{state}\n\
         接入规则：{rule}\n\n\
         如需修改，请打开小聆平台的应用列表，选择目标应用后进入「设备管理」：\n\
         {PLATFORM_APPLICATION_URL}"
    )
}

fn kb_detail_url(index_id: &str) -> String {
    let mut url = reqwest::Url::parse(&format!("{PLATFORM_KB_URL}/detail"))
        .expect("平台知识库详情 URL 必须合法");
    url.query_pairs_mut().append_pair("id", index_id);
    url.into()
}

fn print_action_or_json(value: &Value, raw_json: bool, fallback: &str) -> Result<()> {
    if raw_json {
        print_json(value)
    } else {
        print_action_result(value, fallback)
    }
}

fn print_action_result(value: &Value, fallback: &str) -> Result<()> {
    eprintln!("{}", action_result_text(value, fallback));
    Ok(())
}

fn action_result_text(value: &Value, fallback: &str) -> String {
    let hint = value
        .get("data")
        .and_then(Value::as_object)
        .and_then(|data| {
            let name = ["name", "word", "version"]
                .iter()
                .find_map(|key| data.get(*key).and_then(Value::as_str));
            let id = [
                "id",
                "project_id",
                "index_id",
                "role_id",
                "server_id",
                "package_id",
                "document_id",
            ]
            .iter()
            .find_map(|key| data.get(*key).and_then(Value::as_str));
            match (name, id) {
                (Some(name), Some(id)) => Some(format!("：{name}（ID: {id}）")),
                (Some(name), None) => Some(format!("：{name}")),
                (None, Some(id)) => Some(format!("（ID: {id}）")),
                (None, None) => None,
            }
        })
        .unwrap_or_default();
    format!("{fallback}{hint}")
}

fn validate_device_import_result(value: &Value) -> Result<()> {
    let code = value
        .get("code")
        .and_then(Value::as_str)
        .context("设备导入响应缺少 code")?;
    if code != "SUCCESS" {
        let message = value
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("服务端未提供错误信息");
        anyhow::bail!("设备导入失败：{code}：{message}");
    }

    let failed = value
        .pointer("/data/failed")
        .and_then(Value::as_array)
        .context("设备导入响应缺少 data.failed 数组")?;
    if failed.is_empty() {
        return Ok(());
    }

    let details = failed
        .iter()
        .map(|item| match item {
            Value::Object(item) => {
                let id = item
                    .get("id")
                    .and_then(Value::as_str)
                    .context("设备导入失败项缺少 id")?;
                let error = item
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("服务端未提供失败原因");
                Ok(format!("- {id}: {error}"))
            }
            Value::String(id) => Ok(format!("- {id}")),
            _ => anyhow::bail!("设备导入响应中的 data.failed 项格式无效"),
        })
        .collect::<Result<Vec<_>>>()?;
    anyhow::bail!(
        "设备导入未完全成功（{} 项失败）：\n{}",
        details.len(),
        details.join("\n")
    )
}

fn render_ota_package(value: &Value) -> String {
    let field = |keys: &[&str]| {
        keys.iter()
            .find_map(|key| value.get(key))
            .map(|value| match value {
                Value::String(text) => text.clone(),
                other => other.to_string(),
            })
            .unwrap_or_else(|| "-".to_owned())
    };
    [
        ("OTA 包 ID", field(&["package_id"])),
        ("版本", field(&["version"])),
        ("版本号", field(&["version_number", "versionNumber"])),
        ("模式", field(&["ota_mode", "otaMode"])),
        ("状态", field(&["status", "publish_status"])),
        ("描述", field(&["description"])),
    ]
    .into_iter()
    .map(|(label, value)| format!("{label}: {value}"))
    .collect::<Vec<_>>()
    .join("\n")
}

fn read_json_file(path: &std::path::Path) -> Result<Value> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("读取 JSON 文件失败：{}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("JSON 文件格式错误：{}", path.display()))
}

fn split_assignment(value: &str) -> Result<(String, String)> {
    let (key, value) = value
        .split_once('=')
        .with_context(|| format!("参数必须是 key=value：{value}"))?;
    let key = key.trim();
    if key.is_empty() {
        anyhow::bail!("参数 key 不能为空：{value}");
    }
    Ok((key.to_owned(), value.trim().to_owned()))
}

fn parse_json_literal(value: &str) -> Value {
    match value.to_ascii_lowercase().as_str() {
        "on" => Value::Bool(true),
        "off" => Value::Bool(false),
        _ => serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_owned())),
    }
}

fn role_edit_key(key: &str) -> String {
    match key {
        "vcn" | "volume" | "speed" => format!("tts.{key}"),
        _ => key.to_owned(),
    }
}

fn json_body_from_input<F>(input: JsonEditArgs, map_key: F) -> Result<Value>
where
    F: Fn(&str) -> String,
{
    if let Some(file) = input.file {
        let value = read_json_file(&file)?;
        if !value.is_object() {
            anyhow::bail!("--file 必须包含 JSON 对象");
        }
        return Ok(value);
    }

    let mut value = serde_json::json!({});
    for assignment in input.set {
        let (key, literal) = split_assignment(&assignment)?;
        set_json_path(&mut value, &map_key(&key), parse_json_literal(&literal))?;
    }
    Ok(value)
}

fn set_json_path(target: &mut Value, path: &str, value: Value) -> Result<()> {
    let segments = path
        .split('.')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if segments.is_empty() {
        anyhow::bail!("配置 key 不能为空");
    }
    let mut current = target;
    for segment in &segments[..segments.len() - 1] {
        let object = current
            .as_object_mut()
            .context("配置路径与已有标量值冲突")?;
        current = object
            .entry((*segment).to_owned())
            .or_insert_with(|| serde_json::json!({}));
    }
    current
        .as_object_mut()
        .context("配置目标不是 JSON 对象")?
        .insert(segments[segments.len() - 1].to_owned(), value);
    Ok(())
}

fn ensure_non_empty_object(value: &Value, command: &str) -> Result<()> {
    match value.as_object() {
        Some(object) if !object.is_empty() => Ok(()),
        Some(_) => anyhow::bail!("{command} 需要至少一个 --set 或 --file"),
        None => anyhow::bail!("{command} 请求必须是 JSON 对象"),
    }
}

fn confirm(message: &str) -> Result<bool> {
    if !io::stdin().is_terminal() {
        anyhow::bail!("非交互环境，请追加 --yes 确认执行");
    }
    eprint!("{message} [y/N]: ");
    io::stderr().flush()?;
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
    resolve_optional_api_key()?.ok_or_else(|| {
        anyhow::anyhow!(
            "未找到 API Key。请打开 https://platform.listenai.com/keys 获取 API Key，然后执行 `ling login` 或设置 LING_API_KEY"
        )
    })
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
    use crate::test_support::{temp_path, EnvGuard};
    use clap::{CommandFactory, Parser};
    use std::fs;

    #[test]
    fn parses_app_create_management_contract() {
        let cli = Cli::try_parse_from([
            "ling",
            "app",
            "create",
            "Demo",
            "--description",
            "Voice app",
            "--json",
        ])
        .expect("parse app create");

        match cli.command {
            Command::App(app) => match app.command {
                AppCommand::Create {
                    name,
                    description,
                    json,
                } => {
                    assert_eq!(name, "Demo");
                    assert_eq!(description.as_deref(), Some("Voice app"));
                    assert!(json);
                }
                other => panic!("expected app create command, got {other:?}"),
            },
            other => panic!("expected app command, got {other:?}"),
        }
    }

    #[test]
    fn device_add_accepts_ids_or_file_but_not_both() {
        Cli::try_parse_from(["ling", "app", "device", "add", "dev-1", "dev-2"])
            .expect("parse device ids");
        Cli::try_parse_from(["ling", "app", "device", "add", "--file", "devices.txt"])
            .expect("parse device file");
        Cli::try_parse_from(["ling", "app", "device", "add", "--self"])
            .expect("parse local device ID");
        let err = Cli::try_parse_from([
            "ling",
            "app",
            "device",
            "add",
            "dev-1",
            "--file",
            "devices.txt",
        ])
        .expect_err("ids and file conflict");
        assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
        let err = Cli::try_parse_from(["ling", "app", "device", "add", "--self", "dev-1"])
            .expect_err("local and explicit device IDs conflict");
        assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
        let err = Cli::try_parse_from([
            "ling",
            "app",
            "device",
            "add",
            "--self",
            "--file",
            "devices.txt",
        ])
        .expect_err("local device ID and file conflict");
        assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
        Cli::try_parse_from(["ling", "app", "device", "add"])
            .expect_err("device source is required");
    }

    #[test]
    fn local_device_id_commands_parse_without_app_context() {
        let show = Cli::try_parse_from(["ling", "config", "device-id", "show"])
            .expect("parse local device ID show");
        assert!(matches!(
            show.command,
            Command::Config(LocalConfigArgs {
                command: LocalConfigCommand::DeviceId(LocalDeviceIdArgs {
                    command: LocalDeviceIdCommand::Show { json: false }
                })
            })
        ));

        let reset = Cli::try_parse_from(["ling", "config", "device-id", "reset", "--json"])
            .expect("parse local device ID reset");
        assert!(matches!(
            reset.command,
            Command::Config(LocalConfigArgs {
                command: LocalConfigCommand::DeviceId(LocalDeviceIdArgs {
                    command: LocalDeviceIdCommand::Reset { json: true }
                })
            })
        ));
    }

    #[test]
    fn ota_whitelist_add_accepts_a_device_id_or_self() {
        Cli::try_parse_from(["ling", "app", "ota", "whitelist", "add", "device-1"])
            .expect("parse explicit OTA whitelist Device ID");
        Cli::try_parse_from(["ling", "app", "ota", "whitelist", "add", "--self"])
            .expect("parse local OTA whitelist Device ID");

        let error = Cli::try_parse_from([
            "ling",
            "app",
            "ota",
            "whitelist",
            "add",
            "device-1",
            "--self",
        ])
        .expect_err("explicit Device ID and --self conflict");
        assert_eq!(error.kind(), clap::error::ErrorKind::ArgumentConflict);
        Cli::try_parse_from(["ling", "app", "ota", "whitelist", "add"])
            .expect_err("OTA whitelist Device ID source is required");
    }

    #[test]
    fn app_web_only_operations_link_to_the_project_config_page() {
        assert_eq!(
            app_config_url("project-123"),
            "https://platform.listenai.com/appConfig?id=project-123"
        );
        assert_eq!(
            app_config_url("project with spaces"),
            "https://platform.listenai.com/appConfig?id=project+with+spaces"
        );

        let message = app_config_only_operation("删除角色 role-1", "project-123").to_string();
        assert!(message.contains("CLI 不执行「删除角色 role-1」"));
        assert!(message.contains("https://platform.listenai.com/appConfig?id=project-123"));
    }

    #[test]
    fn application_web_only_operations_link_to_the_correct_drawer_section() {
        let delete = application_only_operation("删除应用", "product-123", "设置").to_string();
        assert!(delete.contains("CLI 不执行「删除应用」"));
        assert!(delete.contains("Product ID 为 product-123"));
        assert!(delete.contains("进入「设置」"));
        assert!(delete.contains(PLATFORM_APPLICATION_URL));
        assert!(!delete.contains("appConfig"));

        let ota =
            application_only_operation("正式发布 OTA 包 package-123", "product-123", "固件升级")
                .to_string();
        assert!(ota.contains("进入「固件升级」"));
        assert!(ota.contains(PLATFORM_APPLICATION_URL));
        assert!(!ota.contains("appConfig"));
    }

    #[test]
    fn knowledge_web_only_operations_link_to_the_knowledge_pages() {
        assert_eq!(PLATFORM_KB_URL, "https://platform.listenai.com/datasets");
        assert_eq!(
            kb_detail_url("kb with spaces"),
            "https://platform.listenai.com/datasets/detail?id=kb+with+spaces"
        );
    }

    #[test]
    fn app_delete_keeps_a_safe_web_only_command_entry() {
        let cli = Cli::try_parse_from(["ling", "app", "delete", "--product-id", "product-123"])
            .expect("parse app delete");

        match cli.command {
            Command::App(app) => {
                assert_eq!(app.product_id.as_deref(), Some("product-123"));
                assert!(matches!(app.command, AppCommand::Delete));
            }
            other => panic!("expected app command, got {other:?}"),
        }
    }

    #[test]
    fn app_metadata_is_only_edited_through_config() {
        assert!(Cli::try_parse_from(["ling", "app", "edit", "--name", "新的应用名称"]).is_err());
        Cli::try_parse_from([
            "ling",
            "app",
            "config",
            "edit",
            "--set",
            "name=新的应用名称",
        ])
        .expect("parse config metadata edit");
    }

    #[test]
    fn role_set_builds_nested_tts_json_and_literals() {
        let body = json_body_from_input(
            JsonEditArgs {
                set: vec![
                    "vcn=x4_yezi".to_owned(),
                    "speed=60".to_owned(),
                    "idle_guide.interval_ms=3000".to_owned(),
                    "enabled=on".to_owned(),
                ],
                file: None,
            },
            role_edit_key,
        )
        .unwrap();
        assert_eq!(body["tts"]["vcn"], "x4_yezi");
        assert_eq!(body["tts"]["speed"], 60);
        assert_eq!(body["idle_guide"]["interval_ms"], 3000);
        assert_eq!(body["enabled"], true);
    }

    #[test]
    fn tone_edit_rejects_file_and_set_together() {
        let err = Cli::try_parse_from([
            "ling",
            "app",
            "tone",
            "edit",
            "--set",
            "network_suc=ok",
            "--file",
            "tones.json",
        ])
        .expect_err("file and set conflict");
        assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
    }

    #[test]
    fn app_create_rejects_the_unreachable_mode_option() {
        let err = Cli::try_parse_from(["ling", "app", "create", "Demo", "--mode", "custom"])
            .expect_err("create mode is not a reachable server state");
        assert_eq!(err.kind(), clap::error::ErrorKind::UnknownArgument);

        let err = Cli::try_parse_from(["ling", "app", "create", "Demo", "--template-id", "12"])
            .expect_err("template selection is no longer part of app creation");
        assert_eq!(err.kind(), clap::error::ErrorKind::UnknownArgument);
    }

    #[test]
    fn parses_chain_version_listing_and_selection() {
        let show = Cli::try_parse_from([
            "ling", "app", "--app-id", "app-123", "chain", "show", "--json",
        ])
        .expect("parse chain show");
        assert!(matches!(
            show.command,
            Command::App(AppArgs {
                command: AppCommand::Chain(ChainArgs {
                    command: ChainCommand::Show { json: true }
                }),
                ..
            })
        ));

        let versions = Cli::try_parse_from([
            "ling",
            "app",
            "--app-id",
            "app-123",
            "chain",
            "versions",
            "--page",
            "2",
            "--page-size",
            "50",
            "--json",
        ])
        .expect("parse chain versions");
        match versions.command {
            Command::App(AppArgs {
                command:
                    AppCommand::Chain(ChainArgs {
                        command:
                            ChainCommand::Versions {
                                page,
                                page_size,
                                json,
                            },
                    }),
                ..
            }) => {
                assert_eq!(page, 2);
                assert_eq!(page_size, 50);
                assert!(json);
            }
            other => panic!("expected chain versions command, got {other:?}"),
        }

        let managed = Cli::try_parse_from([
            "ling",
            "app",
            "--product-id",
            "product-123",
            "chain",
            "set",
            "managed",
        ])
        .expect("parse managed chain");
        assert!(matches!(
            managed.command,
            Command::App(AppArgs {
                command: AppCommand::Chain(ChainArgs {
                    command: ChainCommand::Set {
                        target: ChainSetCommand::Managed { json: false }
                    }
                }),
                ..
            })
        ));

        let custom = Cli::try_parse_from([
            "ling", "app", "--app-id", "app-123", "chain", "set", "custom", "v0.0.8", "--json",
        ])
        .expect("parse custom chain");
        match custom.command {
            Command::App(AppArgs {
                command:
                    AppCommand::Chain(ChainArgs {
                        command:
                            ChainCommand::Set {
                                target: ChainSetCommand::Custom { version, json },
                            },
                    }),
                ..
            }) => {
                assert_eq!(version, "v0.0.8");
                assert!(json);
            }
            other => panic!("expected custom chain command, got {other:?}"),
        }

        for old_command in ["managed", "custom"] {
            let err = Cli::try_parse_from(["ling", "app", "chain", old_command])
                .expect_err("chain modes must be selected through chain set");
            assert_eq!(err.kind(), clap::error::ErrorKind::InvalidSubcommand);
        }

        let deploy =
            Cli::try_parse_from(["ling", "app", "deploy", "--version", "v0.0.8", "--activate"])
                .expect("parse deploy activation");
        match deploy.command {
            Command::App(AppArgs {
                command: AppCommand::Deploy(args),
                ..
            }) => assert!(args.activate),
            other => panic!("expected deploy command, got {other:?}"),
        }
    }

    #[test]
    fn normalizes_framework_agent_versions() {
        assert_eq!(
            normalize_framework_agent_version("0.0.8").unwrap(),
            "v0.0.8"
        );
        assert_eq!(
            normalize_framework_agent_version("v12.3.45").unwrap(),
            "v12.3.45"
        );
        assert!(normalize_framework_agent_version("v1.2").is_err());
        assert!(normalize_framework_agent_version("v1.2.3-beta").is_err());
    }

    #[test]
    fn parses_role_show_and_lexicon_import() {
        let role = Cli::try_parse_from(["ling", "app", "role", "show", "role-1", "--json"])
            .expect("parse role show");
        match role.command {
            Command::App(app) => match app.command {
                AppCommand::Role(RoleArgs {
                    command: RoleCommand::Show { role_id, json },
                }) => {
                    assert_eq!(role_id, "role-1");
                    assert!(json);
                }
                other => panic!("expected role show command, got {other:?}"),
            },
            other => panic!("expected app command, got {other:?}"),
        }

        let lexicon =
            Cli::try_parse_from(["ling", "app", "lexicon", "import", "words.txt", "--json"])
                .expect("parse lexicon import");
        match lexicon.command {
            Command::App(app) => match app.command {
                AppCommand::Lexicon(LexiconArgs {
                    command: LexiconCommand::Import { file, json },
                }) => {
                    assert_eq!(file, PathBuf::from("words.txt"));
                    assert!(json);
                }
                other => panic!("expected lexicon import command, got {other:?}"),
            },
            other => panic!("expected app command, got {other:?}"),
        }
    }

    #[test]
    fn parses_wakeup_word_management_commands() {
        let generate = Cli::try_parse_from([
            "ling",
            "app",
            "wakeword",
            "generate",
            "小聆小聆",
            "--sensitivity",
            "high",
            "--response",
            "你好",
            "--response",
            "我在",
            "--yes",
            "--json",
        ])
        .expect("parse wakeword generate");
        match generate.command {
            Command::App(AppArgs {
                command:
                    AppCommand::Wakeword(AppWakewordArgs {
                        command:
                            WakewordCommand::Generate {
                                name,
                                sensitivity,
                                responses,
                                yes,
                                json,
                                ..
                            },
                    }),
                ..
            }) => {
                assert_eq!(name, "小聆小聆");
                assert_eq!(sensitivity, "high");
                assert_eq!(responses, ["你好", "我在"]);
                assert!(yes);
                assert!(json);
            }
            other => panic!("expected wakeword generate command, got {other:?}"),
        }

        let responses = Cli::try_parse_from([
            "ling",
            "app",
            "wakeword",
            "set-responses",
            "word-1",
            "你好",
            "我在",
            "--json",
        ])
        .expect("parse wakeword set-responses");
        match responses.command {
            Command::App(AppArgs {
                command:
                    AppCommand::Wakeword(AppWakewordArgs {
                        command:
                            WakewordCommand::SetResponses {
                                wakeword_id,
                                responses,
                                json,
                            },
                    }),
                ..
            }) => {
                assert_eq!(wakeword_id, "word-1");
                assert_eq!(responses, ["你好", "我在"]);
                assert!(json);
            }
            other => panic!("expected wakeword set-responses command, got {other:?}"),
        }
        assert!(matches!(
            Cli::try_parse_from(["ling", "app", "wakeword", "responses", "word-1", "--json"])
                .expect("parse wakeword responses")
                .command,
            Command::App(AppArgs {
                command: AppCommand::Wakeword(AppWakewordArgs {
                    command: WakewordCommand::Responses { json: true, .. }
                }),
                ..
            })
        ));
        assert!(matches!(
            Cli::try_parse_from([
                "ling",
                "app",
                "wakeword",
                "reset-responses",
                "word-1",
                "--json"
            ])
            .expect("parse wakeword reset-responses")
            .command,
            Command::App(AppArgs {
                command: AppCommand::Wakeword(AppWakewordArgs {
                    command: WakewordCommand::ResetResponses { json: true, .. }
                }),
                ..
            })
        ));
        assert!(Cli::try_parse_from([
            "ling",
            "app",
            "wakeword",
            "response",
            "set",
            "word-1",
            "--response",
            "你好"
        ])
        .is_err());

        assert!(matches!(
            Cli::try_parse_from([
                "ling", "app", "role", "wakeword", "set", "role-1", "word-1", "--json"
            ])
            .expect("parse role wakeword set")
            .command,
            Command::App(AppArgs {
                command: AppCommand::Role(RoleArgs {
                    command: RoleCommand::Wakeword(RoleWakewordArgs {
                        command: RoleWakewordCommand::Set { json: true, .. }
                    })
                }),
                ..
            })
        ));
    }

    #[test]
    fn validates_wakeup_word_generation_inputs() {
        assert!(validate_wakeup_word_name("小聆小聆").is_ok());
        assert!(validate_wakeup_word_name("").is_err());
        assert_eq!(validate_wakeup_word_name(" hello ").unwrap(), "hello");
        assert!(validate_wakeup_word_name("一二三四五六七八九十一二三").is_err());

        let responses =
            wakeup_word_responses(vec![" 你好 ".to_owned(), "我在".to_owned()], false).unwrap();
        assert_eq!(
            responses,
            vec![
                serde_json::json!({"text": "你好"}),
                serde_json::json!({"text": "我在"})
            ]
        );
        assert!(wakeup_word_responses(Vec::new(), false).is_err());
        assert!(wakeup_word_responses(Vec::new(), true).unwrap().is_empty());
        assert!(wakeup_word_responses(vec![" ".to_owned()], false).is_err());
        assert!(wakeup_word_responses(vec!["你好".to_owned(); 6], false).is_err());
        assert!(
            wakeup_word_responses(vec!["一二三四五六七八九十一二三".to_owned()], false).is_err()
        );
    }

    #[test]
    fn parses_consistent_resource_verbs() {
        assert!(matches!(
            Cli::try_parse_from(["ling", "app", "role", "create", "助手"])
                .expect("parse role create")
                .command,
            Command::App(AppArgs {
                command: AppCommand::Role(RoleArgs {
                    command: RoleCommand::Create { .. }
                }),
                ..
            })
        ));
        assert!(matches!(
            Cli::try_parse_from(["ling", "app", "mcp", "show", "mcp-record-1", "--json"])
                .expect("parse mcp show")
                .command,
            Command::App(AppArgs {
                command: AppCommand::Mcp(McpArgs {
                    command: McpCommand::Show { .. }
                }),
                ..
            })
        ));
        assert!(matches!(
            Cli::try_parse_from([
                "ling",
                "app",
                "mcp",
                "add",
                "天气服务",
                "--server-id",
                "weather",
                "--transport",
                "http",
                "--url",
                "https://mcp.example.com"
            ])
            .expect("parse mcp add")
            .command,
            Command::App(AppArgs {
                command: AppCommand::Mcp(McpArgs {
                    command: McpCommand::Add { .. }
                }),
                ..
            })
        ));
        assert!(matches!(
            Cli::try_parse_from(["ling", "app", "ota", "show", "ota-1"])
                .expect("parse ota show")
                .command,
            Command::App(AppArgs {
                command: AppCommand::Ota(OtaArgs {
                    command: OtaCommand::Show { .. }
                }),
                ..
            })
        ));
        for removed in [
            ["ling", "app", "role", "add", "助手"].as_slice(),
            ["ling", "app", "mcp", "create", "天气服务"].as_slice(),
            ["ling", "app", "ota", "get", "ota-1"].as_slice(),
        ] {
            assert!(Cli::try_parse_from(removed).is_err());
        }
    }

    #[test]
    fn ota_edit_only_accepts_mutable_fields() {
        let cli = Cli::try_parse_from([
            "ling",
            "app",
            "ota",
            "edit",
            "ota-1",
            "--version",
            "2.4.1",
            "--description",
            "修订说明",
        ])
        .expect("parse mutable OTA edit fields");

        match cli.command {
            Command::App(AppArgs {
                command:
                    AppCommand::Ota(OtaArgs {
                        command:
                            OtaCommand::Edit {
                                package_id,
                                version,
                                description,
                                ..
                            },
                    }),
                ..
            }) => {
                assert_eq!(package_id, "ota-1");
                assert_eq!(version.as_deref(), Some("2.4.1"));
                assert_eq!(description.as_deref(), Some("修订说明"));
            }
            other => panic!("expected ota edit command, got {other:?}"),
        }

        for immutable in [
            ["--version-number", "241"],
            ["--ota-mode", "mandatory"],
            ["--minimum-version", "2.0.0"],
        ] {
            let error = Cli::try_parse_from([
                "ling",
                "app",
                "ota",
                "edit",
                "ota-1",
                immutable[0],
                immutable[1],
            ])
            .expect_err("immutable OTA field must be rejected");
            assert_eq!(error.kind(), clap::error::ErrorKind::UnknownArgument);
        }

        let error = Cli::try_parse_from([
            "ling",
            "app",
            "ota",
            "upload",
            "firmware.bin",
            "--version",
            "2.4.1",
            "--version-number",
            "241",
            "--ota-mode",
            "mandatory",
            "--minimum-version",
            "2.0.0",
        ])
        .expect_err("unavailable OTA field must be rejected during upload");
        assert_eq!(error.kind(), clap::error::ErrorKind::UnknownArgument);
    }

    #[test]
    fn ota_edit_help_only_lists_supported_fields() {
        let help = rendered_help(&["ling", "app", "ota", "edit"]);
        assert!(help.contains("--version <VERSION>"));
        assert!(!help.contains("--version-number"));
        assert!(!help.contains("--ota-mode"));
        assert!(!help.contains("--minimum-version"));
    }

    #[test]
    fn lexicon_import_preserves_line_numbers_and_ignores_blank_lines() {
        assert_eq!(
            lexicon_import_entries("  ListenAI  \n\n小聆\nListenAI\n"),
            vec![
                LexiconImportEntry {
                    line: 1,
                    word: "ListenAI".to_owned(),
                },
                LexiconImportEntry {
                    line: 3,
                    word: "小聆".to_owned(),
                },
                LexiconImportEntry {
                    line: 4,
                    word: "ListenAI".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn validates_hotword_character_limits() {
        assert_eq!(normalize_hotword("  小聆  ").unwrap(), "小聆");
        assert!(validate_hotword(&"词".repeat(MAX_HOTWORD_CHARS)).is_ok());
        assert!(validate_hotword(&"词".repeat(MAX_HOTWORD_CHARS + 1)).is_err());
        assert!(validate_hotwords_total(MAX_HOTWORDS_TOTAL_CHARS).is_ok());
        assert!(validate_hotwords_total(MAX_HOTWORDS_TOTAL_CHARS + 1).is_err());

        let items = vec![
            serde_json::json!({"id": "one", "word": "小聆"}),
            serde_json::json!({"hotword_id": "two", "word": "ListenAI"}),
        ];
        assert_eq!(hotwords_total_chars(&items), 10);
        assert_eq!(hotword_id_of(&items[1]), Some("two"));
    }

    #[test]
    fn tone_assignments_are_strings_and_last_value_wins() {
        let assignments = parse_tone_assignments(&[
            "network_suc=第一次".to_owned(),
            "network_suc=\"最终文案\"".to_owned(),
        ])
        .unwrap();
        let mut texts = vec![
            serde_json::json!({"key": "network_suc", "text": "旧文案"}),
            serde_json::json!({"key": "network_fail", "text": "失败"}),
        ];
        apply_tone_assignments(&mut texts, &assignments).unwrap();
        assert_eq!(texts[0]["text"], "最终文案");
        assert_eq!(texts[1]["text"], "失败");

        assert!(parse_tone_assignments(&["network_suc=42".to_owned()]).is_err());
        assert!(
            apply_tone_assignments(&mut texts, &[("unknown".to_owned(), "文案".to_owned())])
                .is_err()
        );
    }

    #[test]
    fn renders_ota_package_summary() {
        let summary = render_ota_package(&serde_json::json!({
            "id": 1979,
            "package_id": "33348d36417b86caf8f174db332ae644",
            "version": "2.4.0",
            "version_number": 240,
            "ota_mode": "mandatory",
            "status": "draft"
        }));
        assert!(summary.contains("OTA 包 ID: 33348d36417b86caf8f174db332ae644"));
        assert!(!summary.contains("1979"));
        assert!(summary.contains("版本号: 240"));
        assert!(summary.contains("状态: draft"));
    }

    #[test]
    fn config_interaction_modes_accept_names_and_numbers() {
        assert_eq!(
            interaction_mode_value(&serde_json::json!("oneshot")).unwrap(),
            0
        );
        assert_eq!(
            interaction_mode_value(&serde_json::json!("full-duplex")).unwrap(),
            1
        );
        assert_eq!(interaction_mode_value(&serde_json::json!(2)).unwrap(), 2);
        assert!(interaction_mode_value(&serde_json::json!(3)).is_err());
    }

    #[test]
    fn config_show_output_is_flat_and_describes_editable_fields() {
        let output = config_show_output(
            &serde_json::json!({
                "data": {
                    "name": "设备助手",
                    "description": "桌面测试应用"
                }
            }),
            &serde_json::json!({"data": {"interaction_mode": 1}}),
            &serde_json::json!({"data": {"system_prompt": "你是设备助手"}}),
            &serde_json::json!({
                "data": {
                    "protocol": "chat_completions",
                    "endpoint": "https://example.com/v1",
                    "model": "deepseek-chat",
                    "authorization_configured": true
                }
            }),
        );

        assert_eq!(output["name"], "设备助手");
        assert_eq!(output["description"], "桌面测试应用");
        assert_eq!(output["interaction_mode"], "full-duplex");
        assert_eq!(output["system_prompt"], "你是设备助手");
        assert_eq!(output["protocol"], "chat_completions");
        assert_eq!(output["authorization_configured"], true);
        assert!(output.get("interaction").is_none());
        assert_eq!(
            output["editable_fields"]["interaction_mode"]["values"],
            serde_json::json!(["oneshot", "full-duplex", "half-duplex"])
        );
        assert_eq!(
            output["editable_fields"]["protocol"]["values"],
            serde_json::json!(["chat_completions"])
        );
        assert_eq!(
            output["editable_fields"]["authorization"]["write_only"],
            true
        );
        assert_eq!(output["editable_fields"]["name"]["non_empty"], true);
        assert_eq!(output["editable_fields"]["name"]["max_length"], 30);
        assert_eq!(
            output["editable_fields"]["description"]["empty_clears"],
            true
        );
        assert_eq!(output["editable_fields"]["description"]["max_length"], 60);
    }

    #[test]
    fn project_config_validates_and_normalizes_metadata() {
        let mut fields = serde_json::json!({
            "name": "  新名称  ",
            "description": "  新描述  ",
            "model": "deepseek-chat"
        })
        .as_object()
        .unwrap()
        .clone();
        let project = take_project_config(&mut fields).unwrap().unwrap();
        assert_eq!(project["name"], "新名称");
        assert_eq!(project["description"], "新描述");
        assert_eq!(fields["model"], "deepseek-chat");

        let mut empty_description = serde_json::json!({"description": "   "})
            .as_object()
            .unwrap()
            .clone();
        assert_eq!(
            take_project_config(&mut empty_description)
                .unwrap()
                .unwrap(),
            serde_json::json!({"description": ""})
        );

        let mut empty_name = serde_json::json!({"name": "   "})
            .as_object()
            .unwrap()
            .clone();
        assert!(take_project_config(&mut empty_name).is_err());

        let mut numeric_description = serde_json::json!({"description": 42})
            .as_object()
            .unwrap()
            .clone();
        assert!(take_project_config(&mut numeric_description).is_err());
    }

    #[test]
    fn action_output_is_human_readable_and_omits_credentials() {
        let value = serde_json::json!({
            "data": {
                "id": "role-1",
                "name": "小聆老师",
                "authorization": "Bearer token"
            }
        });
        let output = action_result_text(&value, "角色创建成功");
        assert_eq!(output, "角色创建成功：小聆老师（ID: role-1）");
        assert!(!output.contains("Bearer"));
        assert!(!output.contains('{'));
    }

    #[test]
    fn device_import_requires_an_empty_failed_array() {
        let success = serde_json::json!({
            "code": "SUCCESS",
            "data": {"failed": []},
            "message": "导入完成"
        });
        validate_device_import_result(&success).expect("empty failure list should succeed");

        let partial_failure = serde_json::json!({
            "code": "SUCCESS",
            "data": {
                "failed": [{
                    "id": "device-1",
                    "error": "deviceId已存在"
                }]
            },
            "message": "导入完成"
        });
        let error = validate_device_import_result(&partial_failure)
            .expect_err("item-level failures must fail the command");
        let message = error.to_string();
        assert!(message.contains("1 项失败"));
        assert!(message.contains("device-1: deviceId已存在"));
    }

    #[test]
    fn device_import_rejects_unsuccessful_or_malformed_responses() {
        let unsuccessful = serde_json::json!({
            "code": "IMPORT_FAILED",
            "message": "产品服务不可用",
            "data": {"failed": []}
        });
        assert!(validate_device_import_result(&unsuccessful)
            .expect_err("business error must fail")
            .to_string()
            .contains("IMPORT_FAILED：产品服务不可用"));

        let malformed = serde_json::json!({
            "code": "SUCCESS",
            "data": {}
        });
        assert!(validate_device_import_result(&malformed)
            .expect_err("missing failed list must fail")
            .to_string()
            .contains("data.failed"));
    }

    #[test]
    fn role_detail_uses_project_default_state() {
        let detail = serde_json::json!({
            "data": {"id": "role-1", "name": "小聆老师", "is_default": false}
        });
        let roles = vec![serde_json::json!({
            "id": "role-1",
            "name": "小聆老师",
            "is_default": true
        })];
        let detail = role_detail_with_project_default(detail, &roles, "role-1");
        assert_eq!(detail["data"]["is_default"], true);
    }

    #[test]
    fn role_tts_partial_update_preserves_required_fields() {
        let mut body = serde_json::json!({"tts": {"volume": 60}});
        let detail = serde_json::json!({
            "data": {
                "tts": {
                    "vcn": "x4_lingxiaoyue_oral",
                    "volume": 50,
                    "speed": 50
                }
            }
        });
        complete_role_tts(&mut body, &detail).unwrap();
        assert_eq!(body["tts"]["vcn"], "x4_lingxiaoyue_oral");
        assert_eq!(body["tts"]["volume"], 60);
        assert_eq!(body["tts"]["speed"], 50);
    }

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
    fn app_deploy_uses_only_global_api_base_url() {
        let cli = Cli::try_parse_from([
            "ling",
            "--api-base-url",
            "https://api.example.test/gateway",
            "app",
            "deploy",
            "--version",
            "v1.0.0",
            "--dry-run",
        ])
        .expect("parse app deploy with global API base URL");
        assert_eq!(cli.api_base_url, "https://api.example.test/gateway");

        let err = Cli::try_parse_from([
            "ling",
            "app",
            "deploy",
            "--version",
            "v1.0.0",
            "--endpoint",
            "https://other.example.test",
        ])
        .expect_err("deploy must not expose a separate endpoint override");
        assert_eq!(err.kind(), clap::error::ErrorKind::UnknownArgument);
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
    fn app_project_and_app_ids_are_global_and_mutually_exclusive() {
        for argv in [
            vec!["ling", "app", "--project-id", "project-1", "inspect"],
            vec!["ling", "app", "inspect", "--project-id", "project-1"],
            vec!["ling", "app", "--app-id", "app-1", "role", "list"],
            vec!["ling", "app", "device", "--app-id", "app-1", "quota"],
        ] {
            let cli = Cli::try_parse_from(argv.clone()).expect("parse alternate app id");
            match cli.command {
                Command::App(app) => {
                    assert!(
                        app.project_id.is_some() || app.app_id.is_some(),
                        "argv: {argv:?}"
                    );
                }
                other => panic!("expected app command, got {other:?}"),
            }
        }

        let error = Cli::try_parse_from([
            "ling",
            "app",
            "--product-id",
            "product-1",
            "--project-id",
            "project-1",
            "inspect",
        ])
        .expect_err("identifier flags must conflict");
        assert_eq!(error.kind(), clap::error::ErrorKind::ArgumentConflict);
    }

    #[test]
    fn product_secret_belongs_only_to_request() {
        for argv in [
            vec![
                "ling",
                "app",
                "--product-id",
                "product-1",
                "request",
                "--product-secret",
                "secret-1",
                "--text",
                "hello",
            ],
            vec![
                "ling",
                "app",
                "request",
                "--text",
                "hello",
                "--product-id",
                "product-1",
                "--product-secret",
                "secret-1",
            ],
        ] {
            let cli = Cli::try_parse_from(argv.clone()).expect("parse app with product secret");
            match cli.command {
                Command::App(app) => {
                    assert_eq!(app.product_id.as_deref(), Some("product-1"));
                    match app.command {
                        AppCommand::Request(request) => {
                            assert_eq!(request.product_secret.as_deref(), Some("secret-1"));
                        }
                        other => panic!("expected app request command, got {other:?}"),
                    }
                }
                other => panic!("expected app command, got {other:?}"),
            }
        }

        let err = Cli::try_parse_from([
            "ling",
            "app",
            "inspect",
            "--product-id",
            "product-1",
            "--product-secret",
            "secret-1",
        ])
        .expect_err("inspect must not accept product secret");
        assert_eq!(err.kind(), clap::error::ErrorKind::UnknownArgument);
    }

    #[test]
    fn product_secret_requires_an_unmasked_value() {
        assert_eq!(
            normalize_product_secret(Some("  complete-secret  ".to_owned()))
                .unwrap()
                .as_deref(),
            Some("complete-secret")
        );
        assert!(normalize_product_secret(Some("abc*****xyz".to_owned())).is_err());
        assert_eq!(
            normalize_product_secret(Some("  ".to_owned())).unwrap(),
            None
        );
    }

    #[test]
    fn request_can_use_explicit_device_credentials_without_management_lookup() {
        let selector = AppSelector {
            product_id: Some("  product-1  ".to_owned()),
            ..AppSelector::default()
        };
        assert_eq!(
            direct_request_product_id(&selector).as_deref(),
            Some("product-1")
        );

        for selector in [
            AppSelector {
                project_id: Some("project-1".to_owned()),
                ..AppSelector::default()
            },
            AppSelector {
                app_id: Some("app-1".to_owned()),
                ..AppSelector::default()
            },
        ] {
            assert!(
                direct_request_product_id(&selector).is_none(),
                "project and app ids still need management resolution"
            );
        }
    }

    #[test]
    fn inspect_positional_product_conflicts_with_identifier_flags() {
        let selector = AppSelector {
            project_id: Some("project-1".to_owned()),
            ..AppSelector::default()
        };
        assert!(selector
            .with_positional_product(Some("product-1".to_owned()))
            .is_err());
    }

    #[test]
    fn parses_ai_tts_options() {
        let cli = Cli::try_parse_from([
            "ling",
            "ai",
            "tts",
            "--vcn",
            "account-authorized-custom-voice",
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
                    assert_eq!(tts.vcn.as_deref(), Some("account-authorized-custom-voice"));
                    assert_eq!(tts.format.as_deref(), Some("pcm"));
                    assert_eq!(tts.output.as_deref(), Some(std::path::Path::new("out.pcm")));
                }
                other => panic!("expected ai tts command, got {other:?}"),
            },
            other => panic!("expected ai command, got {other:?}"),
        }
    }

    #[test]
    fn ai_asr_accepts_verbose_diagnostics() {
        let cli = Cli::try_parse_from(["ling", "ai", "asr", "input.wav", "--verbose", "--json"])
            .expect("parse ai asr diagnostics");
        match cli.command {
            Command::Ai(ai) => match ai.command {
                AiCommand::Asr(asr) => {
                    assert!(asr.verbose);
                    assert!(asr.json);
                    assert_eq!(asr.file, std::path::Path::new("input.wav"));
                }
                other => panic!("expected ai asr command, got {other:?}"),
            },
            other => panic!("expected ai command, got {other:?}"),
        }
    }

    #[test]
    fn ai_tts_accepts_arbitrary_vcn_values() {
        let cli = Cli::try_parse_from([
            "ling",
            "ai",
            "tts",
            "--vcn",
            "account-authorized-custom-voice",
            "你好",
        ])
        .expect("VCN availability belongs to the platform");
        match cli.command {
            Command::Ai(ai) => match ai.command {
                AiCommand::Tts(tts) => {
                    assert_eq!(tts.vcn.as_deref(), Some("account-authorized-custom-voice"));
                }
                other => panic!("expected ai tts command, got {other:?}"),
            },
            other => panic!("expected ai command, got {other:?}"),
        }
    }

    #[test]
    fn ai_tts_enforces_documented_numeric_ranges() {
        for (flag, invalid_values) in [
            ("--speed", &["0", "101", "4294967295"][..]),
            ("--volume", &["0", "101", "4294967295"][..]),
            ("--pitch", &["0", "101", "4294967295"][..]),
            ("--emotion-scale", &["-21", "21", "2147483647"][..]),
        ] {
            for value in invalid_values {
                let option = format!("{flag}={value}");
                let error = Cli::try_parse_from(["ling", "ai", "tts", &option, "你好"])
                    .expect_err("out-of-range TTS options should fail before the request");
                assert_eq!(
                    error.kind(),
                    clap::error::ErrorKind::ValueValidation,
                    "{flag} accepted {value}"
                );
            }
        }

        Cli::try_parse_from([
            "ling",
            "ai",
            "tts",
            "--speed",
            "1",
            "--volume",
            "100",
            "--pitch",
            "50",
            "--emotion-scale=-20",
            "你好",
        ])
        .expect("TTS range boundaries should be accepted");
    }

    #[test]
    fn ai_tts_list_vcn_is_removed() {
        let error = Cli::try_parse_from(["ling", "ai", "tts", "--list-vcn"])
            .expect_err("platform TTS does not expose voice discovery");
        assert_eq!(error.kind(), clap::error::ErrorKind::UnknownArgument);
    }

    #[test]
    fn parses_login_json() {
        let cli = Cli::try_parse_from(["ling", "login", "--api-key", "test-key", "--json"])
            .expect("parse login json");

        match cli.command {
            Command::Login(login) => {
                assert_eq!(login.api_key.as_deref(), Some("test-key"));
                assert!(login.json);
            }
            other => panic!("expected login command, got {other:?}"),
        }
    }

    #[test]
    fn ai_tts_requires_text() {
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
                    assert!(request.device_id.is_none());
                    assert!(!request.verbose);
                    assert!(request.output_tts.is_none());
                }
                other => panic!("expected app request command, got {other:?}"),
            },
            other => panic!("expected app command, got {other:?}"),
        }
    }

    #[test]
    fn app_request_accepts_verbose_output() {
        let cli = Cli::try_parse_from(["ling", "app", "request", "--text", "你好", "--verbose"])
            .expect("parse request verbose");
        match cli.command {
            Command::App(app) => match app.command {
                AppCommand::Request(request) => assert!(request.verbose),
                other => panic!("expected app request command, got {other:?}"),
            },
            other => panic!("expected app command, got {other:?}"),
        }
    }

    #[test]
    fn app_request_accepts_an_explicit_device_id_override() {
        let cli = Cli::try_parse_from([
            "ling",
            "app",
            "request",
            "--text",
            "你好",
            "--device-id",
            "device-1",
        ])
        .expect("parse request device id");
        match cli.command {
            Command::App(app) => match app.command {
                AppCommand::Request(request) => {
                    assert_eq!(request.device_id.as_deref(), Some("device-1"));
                }
                other => panic!("expected app request command, got {other:?}"),
            },
            other => panic!("expected app command, got {other:?}"),
        }
    }

    #[test]
    fn app_request_passes_device_id_overrides_through_without_validation() {
        let device_id = format!("自定义 device id {}", "x".repeat(64));
        let cli = Cli::try_parse_from([
            "ling",
            "app",
            "request",
            "--text",
            "你好",
            "--device-id",
            &device_id,
        ])
        .expect("Device ID validation belongs to the service");

        match cli.command {
            Command::App(app) => match app.command {
                AppCommand::Request(request) => {
                    assert_eq!(request.device_id.as_deref(), Some(device_id.as_str()));
                }
                other => panic!("expected app request command, got {other:?}"),
            },
            other => panic!("expected app command, got {other:?}"),
        }
    }

    #[test]
    fn app_trace_uses_verbose_and_keeps_full_as_an_alias() {
        for flag in ["--verbose", "--full"] {
            let cli = Cli::try_parse_from(["ling", "app", "trace", "sid-1", flag])
                .expect("parse trace details");
            match cli.command {
                Command::App(app) => match app.command {
                    AppCommand::Trace { sid, verbose, json } => {
                        assert_eq!(sid, "sid-1");
                        assert!(verbose);
                        assert!(!json);
                    }
                    other => panic!("expected app trace command, got {other:?}"),
                },
                other => panic!("expected app command, got {other:?}"),
            }
        }
    }

    #[test]
    fn app_request_accepts_tts_output_path() {
        let cli = Cli::try_parse_from([
            "ling",
            "app",
            "request",
            "--text",
            "你好",
            "--output-tts",
            "reply.mp3",
        ])
        .expect("parse request TTS output");
        match cli.command {
            Command::App(app) => match app.command {
                AppCommand::Request(request) => {
                    assert_eq!(
                        request.output_tts.as_deref(),
                        Some(std::path::Path::new("reply.mp3"))
                    );
                }
                other => panic!("expected app request command, got {other:?}"),
            },
            other => panic!("expected app command, got {other:?}"),
        }
    }

    #[test]
    fn app_request_help_describes_tts_output_as_mp3() {
        let help = rendered_help(&["ling", "app", "request"]);
        assert!(help.contains("--output-tts <MP3_FILE>"));
        assert!(help.contains("as MP3 without format conversion"));
    }

    #[test]
    fn request_tty_redraws_reply_around_other_frames() {
        let mut state = RequestTimelineOutputState {
            live_updates: true,
            terminal_width: 80,
            live_reply: None,
        };

        let first = state
            .update_reply_output("今天天气")
            .expect("TTY should draw streaming replies");
        assert!(first.starts_with("\r\u{1b}[2K"));
        assert!(first.contains("↓ 回复：今天天气"));
        assert!(!first.ends_with('\n'));

        let reply_line = state.reply_preview(
            state
                .live_reply
                .as_deref()
                .expect("reply should stay active"),
        );
        assert_eq!(
            state.print_line_output("[12:00:00.000] ↓ TTS URL：https://example.com"),
            format!("\r\u{1b}[2K[12:00:00.000] ↓ TTS URL：https://example.com\n{reply_line}")
        );

        let finished = state
            .finish_reply_output(Some("今天天气"))
            .expect("TTY should finish the active reply");
        assert!(finished.starts_with("\r\u{1b}[2K"));
        assert!(finished.ends_with('\n'));
        assert!(state.live_reply.is_none());
    }

    #[test]
    fn request_without_live_updates_prints_only_the_final_reply() {
        let mut state = RequestTimelineOutputState {
            live_updates: false,
            terminal_width: 80,
            live_reply: None,
        };

        assert!(state.update_reply_output("今天天气").is_none());
        assert!(state.update_reply_output("今天天气很好").is_none());
        let finished = state
            .finish_reply_output(Some("今天天气很好"))
            .expect("non-TTY should print the final reply");

        assert!(finished.contains("↓ 回复：今天天气很好"));
        assert_eq!(finished.lines().count(), 1);
        assert!(!finished.contains('\u{1b}'));
        assert!(state.live_reply.is_none());
    }

    #[test]
    fn parses_chat_options() {
        let cli = Cli::try_parse_from([
            "ling",
            "ai",
            "chat",
            "hello",
            "world",
            "--model",
            "doubao-test",
            "--system",
            "be concise",
            "--temperature",
            "0.2",
            "--top-p",
            "0.8",
            "--max-tokens",
            "128",
        ])
        .expect("parse chat options");

        match cli.command {
            Command::Ai(ai) => match ai.command {
                AiCommand::Chat(chat) => {
                    assert_eq!(chat.prompt, vec!["hello", "world"]);
                    assert_eq!(chat.model, "doubao-test");
                    assert_eq!(chat.system.as_deref(), Some("be concise"));
                    assert_eq!(chat.temperature, Some(0.2));
                    assert_eq!(chat.top_p, Some(0.8));
                    assert_eq!(chat.max_tokens, Some(128));
                }
                other => panic!("expected chat command, got {other:?}"),
            },
            other => panic!("expected ai command, got {other:?}"),
        }
    }

    #[test]
    fn chat_stream_conflicts_with_json() {
        let err = Cli::try_parse_from(["ling", "ai", "chat", "hello", "--stream", "--json"])
            .expect_err("stream and json should conflict");
        assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
    }

    #[test]
    fn parses_app_list_options() {
        let cli = Cli::try_parse_from([
            "ling",
            "app",
            "list",
            "--page",
            "2",
            "--page-size",
            "50",
            "--json",
        ])
        .expect("parse app list");

        match cli.command {
            Command::App(args) => match args.command {
                AppCommand::List {
                    page,
                    page_size,
                    json,
                } => {
                    assert_eq!(page, 2);
                    assert_eq!(page_size, 50);
                    assert!(json);
                }
                other => panic!("expected app list command, got {other:?}"),
            },
            other => panic!("expected app command, got {other:?}"),
        }
    }

    #[test]
    fn paginated_commands_enforce_api_bounds() {
        let commands: &[(&[&str], &str)] = &[
            (&["app", "list"], "--page-size"),
            (&["app", "ota", "list"], "--page-size"),
            (&["app", "ota", "whitelist", "list"], "--page-size"),
            (&["app", "role", "list"], "--page-size"),
            (&["app", "wakeword", "list"], "--page-size"),
            (&["app", "kb", "list"], "--page-size"),
            (&["app", "chain", "versions"], "--page-size"),
            (&["app", "lexicon", "list"], "--page-size"),
            (&["app", "tone", "show"], "--page-size"),
            (&["app", "mcp", "list"], "--page-size"),
            (&["kb", "list"], "--size"),
            (&["kb", "doc", "index-1", "list"], "--size"),
        ];

        for &(command, size_flag) in commands {
            for page in ["0", "1001", "4294967295"] {
                let mut args = vec!["ling"];
                args.extend_from_slice(command);
                args.extend_from_slice(&["--page", page]);
                let error = match Cli::try_parse_from(args) {
                    Ok(_) => panic!("{command:?} accepted page {page}"),
                    Err(error) => error,
                };
                assert_eq!(
                    error.kind(),
                    clap::error::ErrorKind::ValueValidation,
                    "{command:?} returned the wrong error for page {page}"
                );
            }

            for size in ["0", "101", "4294967295"] {
                let mut args = vec!["ling"];
                args.extend_from_slice(command);
                args.extend_from_slice(&[size_flag, size]);
                let error = match Cli::try_parse_from(args) {
                    Ok(_) => panic!("{command:?} accepted {size_flag} {size}"),
                    Err(error) => error,
                };
                assert_eq!(
                    error.kind(),
                    clap::error::ErrorKind::ValueValidation,
                    "{command:?} returned the wrong error for {size_flag} {size}"
                );
            }
        }

        Cli::try_parse_from([
            "ling",
            "app",
            "list",
            "--page",
            "1000",
            "--page-size",
            "100",
        ])
        .expect("pagination upper bounds should be accepted");
    }

    #[test]
    fn interact_mode_only_exists_as_app_config() {
        assert!(Cli::try_parse_from(["ling", "app", "interact-mode"]).is_err());
        Cli::try_parse_from([
            "ling",
            "app",
            "config",
            "edit",
            "--set",
            "interaction-mode=full-duplex",
        ])
        .expect("parse interaction mode through app config");
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
            "--json",
        ])
        .expect("parse kb doc add");
        match cli.command {
            Command::Kb(kb) => match kb.command {
                KbCommand::Doc { index_id, command } => {
                    assert_eq!(index_id, "idx-1");
                    assert!(matches!(command, KbDocCommand::Add { json: true, .. }));
                }
                other => panic!("expected kb doc command, got {other:?}"),
            },
            other => panic!("expected kb command, got {other:?}"),
        }
    }

    #[test]
    fn app_service_type_option_is_gone() {
        let err = Cli::try_parse_from(["ling", "app", "list", "--service-type", "device"])
            .expect_err("app list should always target device apps");
        assert_eq!(err.kind(), clap::error::ErrorKind::UnknownArgument);
    }

    #[test]
    fn parses_wiki_search_keywords() {
        let cli = Cli::try_parse_from(["ling", "wiki", "search", "--json", "标准API", "密钥"])
            .expect("parse wiki search");

        match cli.command {
            Command::Wiki(args) => match args.command {
                WikiCommand::Search { keywords, json } => {
                    assert_eq!(keywords, vec!["标准API", "密钥"]);
                    assert!(json);
                }
            },
            other => panic!("expected wiki command, got {other:?}"),
        }
    }

    #[test]
    fn old_top_level_commands_are_gone() {
        for cmd in ["models", "chat", "create", "build", "deploy"] {
            assert!(
                Cli::try_parse_from(["ling", cmd]).is_err(),
                "`ling {cmd}` should no longer parse"
            );
        }
    }

    #[test]
    fn app_capabilities_command_is_gone() {
        let err = Cli::try_parse_from(["ling", "app", "capabilities"])
            .expect_err("server capability discovery is not a user command");
        assert_eq!(err.kind(), clap::error::ErrorKind::InvalidSubcommand);
    }

    #[test]
    fn help_includes_new_command_groups() {
        let help = Cli::command().render_long_help().to_string();
        assert!(help.contains("ai"));
        assert!(help.contains("app"));
        assert!(help.contains("kb"));
        assert!(help.contains("wiki"));
    }

    /// 走 clap 解析路径渲染 `--help`，全局参数已传播到子命令。
    fn rendered_help(args: &[&str]) -> String {
        let error = cli_command()
            .try_get_matches_from(args.iter().copied().chain(["--help"]))
            .expect_err("--help always short-circuits parsing");
        assert_eq!(error.kind(), clap::error::ErrorKind::DisplayHelp);
        error.to_string()
    }

    #[test]
    fn targetless_app_commands_hide_app_selectors_from_help() {
        for command in TARGETLESS_APP_COMMANDS {
            let help = rendered_help(&["ling", "app", command]);
            for (_, long) in APP_SELECTOR_ARGS {
                assert!(
                    !help.contains(long),
                    "`ling app {command} --help` 不应宣传运行时会被拒绝的 --{long}"
                );
            }
        }

        // 真正针对单个应用的子命令仍然展示标识参数。
        let help = rendered_help(&["ling", "app", "inspect"]);
        for (_, long) in APP_SELECTOR_ARGS {
            assert!(help.contains(long), "`ling app inspect` 应展示 --{long}");
        }
    }

    #[test]
    fn hidden_app_selectors_still_reach_the_runtime_guard() {
        // 隐藏只影响帮助展示，误传仍走运行时守卫而非 clap 报错。
        for args in [
            vec!["ling", "app", "--product-id", "product-1", "trace", "sid-1"],
            vec!["ling", "app", "trace", "sid-1", "--product-id", "product-1"],
        ] {
            let cli = cli_command()
                .try_get_matches_from(&args)
                .and_then(|matches| Cli::from_arg_matches(&matches))
                .expect("selector parses, the guard rejects it later");
            match cli.command {
                Command::App(app) => assert_eq!(app.product_id.as_deref(), Some("product-1")),
                other => panic!("expected app command, got {other:?}"),
            }
        }
    }

    #[test]
    fn device_enforce_is_read_only() {
        let error = Cli::try_parse_from(["ling", "app", "device", "enforce", "on"])
            .expect_err("toggling whitelist enforcement is a web-only operation");
        assert_eq!(error.kind(), clap::error::ErrorKind::UnknownArgument);
        Cli::try_parse_from(["ling", "app", "device", "enforce"])
            .expect("showing state is allowed");
    }

    #[test]
    fn device_management_guidance_explains_impact_and_uses_application_page() {
        let enabled = device_enforcement_summary(true);
        assert!(enabled.contains("强制白名单：已开启"));
        assert!(enabled.contains("仅已导入的设备可以接入此应用"));
        assert!(enabled.contains(PLATFORM_APPLICATION_URL));
        assert!(!enabled.contains("appConfig"));

        let disabled = device_enforcement_summary(false);
        assert!(disabled.contains("强制白名单：未开启"));
        assert!(disabled.contains("设备无需预先导入即可接入此应用"));
        assert!(disabled.contains(PLATFORM_APPLICATION_URL));

        let list = device_list_guidance();
        assert!(list.contains("暂不展示已导入设备列表"));
        assert!(list.contains("选择目标应用"));
        assert!(list.contains(PLATFORM_APPLICATION_URL));
        assert!(!list.contains("appConfig"));
    }

    #[test]
    fn unavailable_ai_wakeword_is_hidden_from_help() {
        let mut command = Cli::command();
        let ai = command.find_subcommand_mut("ai").expect("ai command");
        let help = ai.render_long_help().to_string();
        assert!(!help.contains("wakeword"));

        let message =
            platform_write_unavailable("唤醒词资源生成", PLATFORM_CUSTOM_FIRMWARE_URL).to_string();
        assert!(message.contains(PLATFORM_CUSTOM_FIRMWARE_URL));
        assert!(!message.contains("https://platform.listenai.com\n"));
    }

    #[test]
    fn targetless_app_commands_reject_app_selectors() {
        let selector = AppSelector {
            product_id: Some("product-1".to_owned()),
            ..Default::default()
        };
        for command in ["list", "create", "build", "trace"] {
            let error =
                ensure_no_app_selector(&selector, command).expect_err("selector must be rejected");
            assert!(error.to_string().contains(command));
        }
        assert!(ensure_no_app_selector(&AppSelector::default(), "list").is_ok());
    }

    #[test]
    fn resolves_api_key_from_env_before_config() {
        let guard = EnvGuard::new(&["LING_API_KEY", "LING_CONFIG"]);
        let dir = temp_path("ling-resolve-env-test");
        let config_path = dir.join("config.json");
        guard.set_var("LING_CONFIG", &config_path);
        config::LingConfig {
            api_key: Some("saved-key".to_owned()),
            ..Default::default()
        }
        .save()
        .expect("save config");

        guard.set_var("LING_API_KEY", "Bearer env-key");

        assert_eq!(
            resolve_optional_api_key().unwrap().as_deref(),
            Some("env-key")
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn resolves_api_key_from_saved_config() {
        let guard = EnvGuard::new(&["LING_API_KEY", "LING_CONFIG"]);
        let dir = temp_path("ling-resolve-config-test");
        let config_path = dir.join("nested").join("config.json");
        guard.set_var("LING_CONFIG", &config_path);
        config::LingConfig {
            api_key: Some("Bearer saved-key".to_owned()),
            ..Default::default()
        }
        .save()
        .expect("save config");

        assert_eq!(
            resolve_optional_api_key().unwrap().as_deref(),
            Some("saved-key")
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn resolve_api_key_reports_missing_credentials() {
        let guard = EnvGuard::new(&["LING_API_KEY", "LING_CONFIG"]);
        let config_path = temp_path("ling-resolve-missing-test").join("config.json");
        guard.set_var("LING_CONFIG", &config_path);

        let err = resolve_api_key().expect_err("missing key should fail");

        assert!(format!("{err:?}").contains("未找到 API Key"));
    }

    #[test]
    fn help_omits_removed_docs_options() {
        let help = Cli::command().render_long_help().to_string();
        assert!(!help.contains("--docs-graphql-url"));
        assert!(!help.contains("--docs-base-url"));
        assert!(!help.contains("--platform-base-url"));
    }
}
