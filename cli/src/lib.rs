//! The command surface. Turns a [`Registry`] into a complete CLI.
//!
//! Downstream binaries are a few lines:
//!
//! ```no_run
//! # use portkit_core::Registry;
//! #[tokio::main]
//! async fn main() -> std::process::ExitCode {
//!     portkit_cli::run(my_registry()).await
//! }
//! # fn my_registry() -> Registry { Registry::new() }
//! ```

mod commands;
mod logging;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};

use portkit_core::{Config, Registry};

/// Parse arguments, run the requested command, and map the result to an exit code.
///
/// Errors are printed to stderr — stdout stays reserved for command output and
/// for the MCP protocol stream.
pub async fn run(registry: Registry) -> ExitCode {
    let cli = Cli::parse();

    let config = match Config::load(cli.config.as_deref()) {
        Ok(mut config) => {
            if let Some(level) = &cli.log_level {
                config.log.level = level.clone();
            }
            config
        }
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::FAILURE;
        }
    };

    logging::init(&config.log.level);

    match cli.command.execute(registry, &config).await {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "pk",
    author,
    version,
    about = "Port tested Python agentic processes to Rust tooling — as a CLI, an MCP server, and a library.",
    long_about = None,
    propagate_version = true
)]
pub struct Cli {
    /// Config file to layer over the built-in defaults.
    #[arg(short, long, value_name = "FILE", global = true)]
    pub config: Option<PathBuf>,

    /// Log filter, e.g. `debug` or `pk=trace`. Logs always go to stderr.
    #[arg(
        short,
        long,
        value_name = "FILTER",
        global = true,
        env = "PK_LOG_LEVEL"
    )]
    pub log_level: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// List the registered tools.
    Tools {
        /// Emit JSON instead of a table.
        #[arg(long)]
        json: bool,
    },

    /// Print a tool's JSON Schema.
    Schema {
        /// Tool name, as shown by `pk tools`.
        tool: String,
    },

    /// Call a tool directly.
    Run {
        /// Tool name, as shown by `pk tools`.
        tool: String,

        /// Arguments as a JSON file, or `-` to read stdin.
        #[arg(short, long, value_name = "FILE")]
        input: Option<PathBuf>,

        /// Inline argument as `key=value`. Values parse as JSON when they can,
        /// and stay strings when they cannot. Repeatable.
        #[arg(short = 'a', long = "arg", value_name = "KEY=VALUE")]
        args: Vec<String>,

        /// Print compact JSON rather than indented.
        #[arg(long)]
        compact: bool,

        /// Route the call through the MCP envelope, as an agent would.
        #[arg(long)]
        through_mcp: bool,

        /// Cap output at N bytes of JSON (~4 bytes/token). Overrides the
        /// tool's own budget and the config default.
        #[arg(long, value_name = "BYTES")]
        budget: Option<usize>,

        /// Print the whole result, however large.
        #[arg(long, conflicts_with = "budget")]
        full: bool,
    },

    /// Serve the tools over MCP on stdio.
    Serve {
        /// Transport. Only stdio is supported today.
        #[arg(long, value_enum, default_value_t = Transport::Stdio)]
        transport: Transport,
    },

    /// Capture reference fixtures and replay them against this port.
    Port {
        #[command(subcommand)]
        command: PortCommand,
    },

    /// Summarize recorded tool-call costs.
    Trace {
        /// Directory of JSONL trace files. Defaults to the configured dir.
        #[arg(short, long, value_name = "DIR")]
        dir: Option<PathBuf>,

        /// Emit JSON instead of a table.
        #[arg(long)]
        json: bool,
    },

    /// Show the effective configuration after all layers are applied.
    Config,

    /// Generate a shell completion script.
    Completion {
        #[arg(value_enum)]
        shell: Shell,
    },
}

#[derive(Subcommand, Debug)]
pub enum PortCommand {
    /// Run cases through the reference implementation and record the results.
    Capture {
        /// Shell command running the reference. It receives
        /// `{"tool":…,"input":…}` on stdin and must print JSON on stdout.
        #[arg(short = 'C', long, value_name = "COMMAND")]
        cmd: String,

        /// JSONL file of cases, one `{"tool":…,"input":…}` per line.
        #[arg(long, value_name = "FILE", default_value = "cases.jsonl")]
        cases: PathBuf,

        /// Directory to write fixtures into.
        #[arg(short, long, value_name = "DIR", default_value = "fixtures")]
        out: PathBuf,

        /// Stop at the first case the reference fails on.
        #[arg(long)]
        fail_fast: bool,
    },

    /// Replay fixtures against the registry and report any drift.
    Replay {
        /// Directory holding the fixtures.
        #[arg(short, long, value_name = "DIR", default_value = "fixtures")]
        fixtures: PathBuf,

        /// Replay only this tool.
        #[arg(short, long, value_name = "TOOL")]
        tool: Option<String>,

        /// Which surface to call through.
        #[arg(long, value_enum, default_value_t = SurfaceArg::Both)]
        surface: SurfaceArg,

        /// Absolute float tolerance. Overrides `parity.epsilon` from config.
        #[arg(long, value_name = "EPS")]
        epsilon: Option<f64>,

        /// Emit the report as JSON.
        #[arg(long)]
        json: bool,
    },
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transport {
    Stdio,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceArg {
    /// Call the registry directly.
    Direct,
    /// Call through MCP `tools/call`.
    Mcp,
    /// Check both, which is what CI should do.
    Both,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
    Elvish,
    PowerShell,
}

impl Command {
    async fn execute(self, registry: Registry, config: &Config) -> portkit_core::Result<ExitCode> {
        match self {
            Command::Tools { json } => commands::tools(&registry, json),
            Command::Schema { tool } => commands::schema(&registry, &tool),
            Command::Run {
                tool,
                input,
                args,
                compact,
                through_mcp,
                budget,
                full,
            } => {
                commands::run_tool(
                    &registry,
                    config,
                    commands::RunArgs {
                        tool: &tool,
                        input,
                        args: &args,
                        compact,
                        through_mcp,
                        budget,
                        full,
                    },
                )
                .await
            }
            Command::Serve { transport } => commands::serve(registry, config, transport).await,
            Command::Port { command } => commands::port(&registry, config, command).await,
            Command::Trace { dir, json } => commands::trace(config, dir, json),
            Command::Config => commands::show_config(config),
            Command::Completion { shell } => {
                commands::completion(shell);
                Ok(ExitCode::SUCCESS)
            }
        }
    }
}
