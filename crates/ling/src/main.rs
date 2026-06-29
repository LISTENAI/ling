mod api_key;
mod config;
mod secret_prompt;
mod terminal;
mod v1_api;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use std::process::ExitCode;

#[derive(Debug, Parser)]
#[command(name = "ling", version, about = "ListenAI local CLI")]
struct Cli {
    #[arg(
        long,
        env = "LING_API_BASE_URL",
        default_value = "https://api.listenai.com"
    )]
    api_base_url: String,

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
    /// List available v1 models.
    Models {
        /// Print the raw JSON response.
        #[arg(long)]
        json: bool,
    },
    /// Send a prompt to /v1/chat/completions.
    Chat(ChatArgs),
    /// Scaffold a new agent project from a template.
    Create(ling_plugin_agent::CreateArgs),
    /// Bundle an agent project to a single JS file.
    Build(ling_plugin_agent::BuildArgs),
    /// Run an agent locally with hot reload and a mock device REPL.
    Dev,
    /// Preview or upload an agent bundle to the platform.
    Deploy(ling_plugin_agent::DeployArgs),
    /// Platform app commands.
    App(AppArgs),
    /// Search ListenAI documentation center.
    Wiki(WikiArgs),
}

#[derive(Debug, Args)]
struct LoginArgs {
    /// API Key from platform.listenai.com/keys. If omitted, ling prompts for it.
    #[arg(long = "api-key", env = "LING_API_KEY")]
    api_key: Option<String>,
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
struct AppArgs {
    #[command(subcommand)]
    command: AppCommand,
}

#[derive(Debug, Subcommand)]
enum AppCommand {
    /// List projects with the saved /keys API Key.
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
    /// Inspect an app by product_id with the saved /keys API Key.
    Inspect {
        #[arg(value_name = "product_id")]
        product_id: String,
        /// Print the raw JSON response.
        #[arg(long)]
        json: bool,
    },
}

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

async fn run(cli: Cli) -> Result<ExitCode> {
    match cli.command {
        Command::Login(args) => {
            login(cli.api_base_url, args).await?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Account { json } => {
            account_command(cli.api_base_url, json).await?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Models { json } => {
            models_command(cli.api_base_url, json).await?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Chat(args) => {
            chat_command(cli.api_base_url, args).await?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Create(args) => {
            let ctx = ling_plugin_agent::AgentContext {
                api_base_url: cli.api_base_url,
                saved_api_key: None,
            };
            ling_plugin_agent::create_command(&ctx, args).await
        }
        Command::Build(args) => {
            let ctx = ling_plugin_agent::AgentContext {
                api_base_url: cli.api_base_url,
                saved_api_key: None,
            };
            ling_plugin_agent::build_command(&ctx, args).await
        }
        Command::Dev => {
            let ctx = ling_plugin_agent::AgentContext {
                api_base_url: cli.api_base_url,
                saved_api_key: None,
            };
            ling_plugin_agent::dev_command(&ctx).await
        }
        Command::Deploy(args) => {
            let saved_api_key = if args.dry_run {
                None
            } else {
                config::LingConfig::load()?.api_key
            };
            let ctx = ling_plugin_agent::AgentContext {
                api_base_url: cli.api_base_url,
                saved_api_key,
            };
            ling_plugin_agent::deploy_command(&ctx, args).await
        }
        Command::App(args) => {
            app_command(cli.api_base_url, args).await?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Wiki(args) => {
            wiki_command(cli.docs_graphql_url, cli.docs_base_url, args).await?;
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

async fn models_command(api_base_url: String, json: bool) -> Result<()> {
    let api_key = resolve_api_key()?;
    let output = v1_api::models(&api_base_url, &api_key).await?;
    if json {
        print_json(&output)
    } else {
        println!("{}", v1_api::render_models(&output)?);
        Ok(())
    }
}

async fn chat_command(api_base_url: String, args: ChatArgs) -> Result<()> {
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
        v1_api::chat_completion_stream(&api_base_url, &api_key, &request).await
    } else {
        let output = v1_api::chat_completion(&api_base_url, &api_key, &request).await?;
        if args.json {
            print_json(&output)
        } else {
            println!("{}", v1_api::render_chat_completion(&output)?);
            Ok(())
        }
    }
}

async fn app_command(api_base_url: String, args: AppArgs) -> Result<()> {
    let api_key = resolve_api_key()?;

    match args.command {
        AppCommand::List {
            page,
            page_size,
            service_type,
            json,
        } => {
            let output = ling_plugin_app::list_projects(
                &api_base_url,
                &api_key,
                page,
                page_size,
                service_type.as_deref(),
            )
            .await?;
            if json {
                print_json(&output)
            } else {
                println!("{}", ling_plugin_app::render_project_list(&output)?);
                Ok(())
            }
        }
        AppCommand::Inspect { product_id, json } => {
            let output =
                ling_plugin_app::inspect_product(&api_base_url, &api_key, &product_id).await?;
            if json {
                print_json(&output)
            } else {
                println!("{}", ling_plugin_app::render_project_inspect(&output)?);
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

fn print_json<T: serde::Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn resolve_api_key() -> Result<String> {
    if let Ok(api_key) = std::env::var("LING_API_KEY") {
        let api_key = api_key::strip_bearer(&api_key);
        if !api_key.is_empty() {
            return Ok(api_key);
        }
    }

    let cfg = config::LingConfig::load()?;
    cfg.api_key
        .filter(|api_key| !api_key.trim().is_empty())
        .map(|api_key| api_key::strip_bearer(&api_key))
        .ok_or_else(|| anyhow::anyhow!("未找到 API Key，请先执行 `ling login` 或设置 LING_API_KEY"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, Parser};

    #[test]
    fn parses_build_defaults() {
        let cli = Cli::try_parse_from(["ling", "build"]).expect("parse build");

        match cli.command {
            Command::Build(build) => {
                assert_eq!(build.entry, "agent.ts");
                assert_eq!(build.out, "dist/agent.js");
                assert!(!build.release);
            }
            other => panic!("expected build command, got {other:?}"),
        }
    }

    #[test]
    fn deploy_requires_product_id() {
        let err = Cli::try_parse_from(["ling", "deploy", "--version", "v1.0.0", "--dry-run"])
            .expect_err("product id should be required");
        assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn deploy_requires_version() {
        let err = Cli::try_parse_from(["ling", "deploy", "--product-id", "prod_dev_local"])
            .expect_err("version should be required");
        assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn parses_create_defaults() {
        let cli = Cli::try_parse_from(["ling", "create", "my-agent"]).expect("parse create");

        match cli.command {
            Command::Create(create) => {
                assert_eq!(create.name, "my-agent");
                assert_eq!(create.template, "listenai");
                assert!(!create.no_install);
            }
            other => panic!("expected create command, got {other:?}"),
        }
    }

    #[test]
    fn parses_create_no_install() {
        let cli =
            Cli::try_parse_from(["ling", "create", "my-agent", "--no-install"]).expect("parse");

        match cli.command {
            Command::Create(create) => assert!(create.no_install),
            other => panic!("expected create command, got {other:?}"),
        }
    }

    #[test]
    fn help_includes_agent_developer_commands() {
        let help = Cli::command().render_long_help().to_string();
        assert!(help.contains("create"));
        assert!(help.contains("build"));
        assert!(help.contains("deploy"));
    }
}
