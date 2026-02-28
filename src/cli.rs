use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(name = "whisperx", version, about = "DX-first wrapper for whisper.cpp")]
pub struct Cli {
    /// Path to config file (defaults to OS-specific user config dir)
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Transcribe an audio file
    Transcribe(TranscribeArgs),

    /// Microphone daemon and trigger commands
    Mic {
        #[command(subcommand)]
        command: MicCommand,
    },

    /// Manage model registry and local cache
    Models {
        #[command(subcommand)]
        command: ModelsCommand,
    },

    /// Initialize or inspect local config
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },

    /// Check environment and print actionable diagnostics
    Doctor,
}

#[derive(Debug, Clone, Subcommand)]
pub enum ModelsCommand {
    /// List available models and installed status
    List,

    /// Install a model into the local model cache
    Install { name: String },

    /// Print model cache directory
    Path,
}

#[derive(Debug, Clone, Subcommand)]
pub enum ConfigCommand {
    /// Write a default config file
    Init {
        /// Overwrite existing config file
        #[arg(long)]
        force: bool,
    },

    /// Show effective config (defaults + file)
    Show,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, ValueEnum)]
pub enum OutputFormat {
    Txt,
    Json,
    Srt,
    Vtt,
}

#[derive(Debug, Clone, Args)]
#[command(trailing_var_arg = true)]
pub struct TranscribeArgs {
    /// Input audio file path
    pub input: PathBuf,

    /// Model name from the registry (e.g. base.en)
    #[arg(long)]
    pub model: Option<String>,

    /// Output format to print to stdout
    #[arg(long, value_enum, default_value_t = OutputFormat::Txt)]
    pub output: OutputFormat,

    /// Disable ffmpeg normalization and pass the input directly to whisper-cli
    #[arg(long)]
    pub no_convert: bool,

    /// Number of threads for whisper-cli
    #[arg(long)]
    pub threads: Option<usize>,

    /// Language code passed to whisper-cli, use "auto" to disable explicit language
    #[arg(long)]
    pub language: Option<String>,

    /// Set whisper-cli translate mode
    #[arg(long)]
    pub translate: bool,

    /// Pass-through whisper.cpp flags after `--`
    #[arg(allow_hyphen_values = true, num_args = 0..)]
    pub passthrough: Vec<String>,
}

#[derive(Debug, Clone, Subcommand)]
pub enum MicCommand {
    /// Run microphone daemon (long-running)
    Daemon(MicDaemonArgs),

    /// Start microphone recording
    Start(MicControlArgs),

    /// Stop recording, transcribe, and type output
    Stop(MicControlArgs),

    /// Toggle recording/transcription cycle
    Toggle(MicControlArgs),

    /// Show daemon status
    Status(MicControlArgs),

    /// Ask daemon to shut down
    Shutdown(MicControlArgs),
}

#[derive(Debug, Clone, Args)]
#[command(trailing_var_arg = true)]
pub struct MicDaemonArgs {
    /// Unix socket path for mic daemon
    #[arg(long)]
    pub socket: Option<PathBuf>,

    /// Microphone source passed to ffmpeg pulse input
    #[arg(long)]
    pub source: Option<String>,

    /// Ignore captured chunks shorter than this duration in seconds
    #[arg(long)]
    pub min_seconds: Option<f32>,

    /// Path or command name for xdotool
    #[arg(long)]
    pub xdotool_bin: Option<String>,

    /// Path or command name for ffmpeg (overrides config)
    #[arg(long)]
    pub ffmpeg_bin: Option<String>,

    /// Model name from registry, defaults to config default_model
    #[arg(long)]
    pub model: Option<String>,

    /// Number of whisper threads
    #[arg(long)]
    pub threads: Option<usize>,

    /// Language code (use auto to disable explicit language)
    #[arg(long)]
    pub language: Option<String>,

    /// Enable whisper-cli translate mode
    #[arg(long)]
    pub translate: bool,

    /// Do not type into active window; print transcript to stdout
    #[arg(long)]
    pub dry_run: bool,

    /// Extra whisper.cpp flags passed through after `--`
    #[arg(allow_hyphen_values = true, num_args = 0..)]
    pub passthrough: Vec<String>,
}

#[derive(Debug, Clone, Args)]
pub struct MicControlArgs {
    /// Unix socket path for mic daemon
    #[arg(long)]
    pub socket: Option<PathBuf>,
}
