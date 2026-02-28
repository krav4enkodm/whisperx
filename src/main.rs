mod cache;
mod cli;
mod config;
mod doctor;
mod models;
mod transcribe;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands, ConfigCommand, ModelsCommand};

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Transcribe(args) => {
            let (config, _) = config::load(cli.config.as_ref())?;
            transcribe::run(&config, &args)?;
        }
        Commands::Models { command } => {
            let (config, _) = config::load(cli.config.as_ref())?;
            match command {
                ModelsCommand::List => models::list(&config)?,
                ModelsCommand::Install { name } => {
                    let path = models::ensure_model(&config, &name)?;
                    println!("Installed model '{name}' at {}", path.display());
                }
                ModelsCommand::Path => println!("{}", config.model_dir.display()),
            }
        }
        Commands::Config { command } => match command {
            ConfigCommand::Init { force } => {
                let path = config::init(cli.config.as_ref(), force)?;
                println!("Wrote config to {}", path.display());
            }
            ConfigCommand::Show => {
                let (cfg, path) = config::load(cli.config.as_ref())?;
                println!("# path: {}", path.display());
                println!("{}", toml::to_string_pretty(&cfg)?);
            }
        },
        Commands::Doctor => {
            let (config, _) = config::load(cli.config.as_ref())?;
            doctor::run(&config)?;
        }
    }

    Ok(())
}
