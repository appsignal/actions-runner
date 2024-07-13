use anyhow::Result;
use builder::Builder;
use camino::Utf8PathBuf;
use chrono::Utc;
use clap::{Parser, Subcommand};
use config::manager::ManagerConfig;

use manager::Manager;
use std::env;
use std::process::ExitCode;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Runs the manager, which will start the instances and manage them
    Run(ManageArgs),

    /// Build new image from a Dockerfile
    Build(BuildArgs),
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct BuildArgs {
    dockerfile: Utf8PathBuf,

    output: Utf8PathBuf,

    #[arg(short, long)]
    size: Option<u8>,

    #[arg(short, long)]
    log_level: Option<log::LevelFilter>,
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct ManageArgs {
    #[arg(short, long)]
    config: Utf8PathBuf,

    #[arg(short, long)]
    debug_role: Option<String>,

    #[arg(short, long, default_value_t = 201)]
    instance_index: u8,

    #[arg(short, long)]
    log_level: Option<log::LevelFilter>,
}

fn main() -> Result<ExitCode> {
    match env::args().next() {
        Some(path) if path.ends_with("actions-init") => {
            init(&path)?;
        }
        Some(path) if path.ends_with("actions-run") => {
            run()?;
        }
        _ => {
            let args = Args::parse();
            match args.command {
                Commands::Build(args) => build(args)?,
                Commands::Run(args) => manage(args)?,
            }
        }
    }

    Ok(ExitCode::SUCCESS)
}

fn run() -> Result<()> {
    setup_logger(log::LevelFilter::Debug).expect("Could not setup logger");

    let runner = runner::Runner::new();
    runner.run()?;

    Ok(())
}

fn init(path: &str) -> Result<()> {
    setup_logger(log::LevelFilter::Debug).expect("Could not setup logger");

    let initialiser = initialiser::Initialiser::new(path);
    initialiser.run()?;

    Ok(())
}

fn build(args: BuildArgs) -> Result<()> {
    setup_logger(args.log_level.unwrap_or(log::LevelFilter::Info)).expect("Could not setup logger");

    let builder = Builder::new(&args.dockerfile, &args.output, args.size)?;
    builder.build()?;

    Ok(())
}

fn manage(args: ManageArgs) -> Result<()> {
    setup_logger(args.log_level.unwrap_or(log::LevelFilter::Info)).expect("Could not setup logger");

    let config =
        ManagerConfig::from_file(&args.config.clone()).expect("Could not load config");
    let mut manager = Manager::new(config);

    match args.debug_role {
        Some(role) => {
            log::info!(
                "Debugging role: `{}` with index: '{}' from config: `{}`",
                role,
                args.instance_index,
                args.config
            );
            manager
                .debug(&role, args.instance_index)
                .expect("Could not debug instance");
        }
        None => {
            log::info!("Starting with config: {}", args.config);
            manager.setup()?;
            manager.run()?;
        }
    }

    Ok(())
}

fn setup_logger(log_level: log::LevelFilter) -> Result<(), fern::InitError> {
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{} {} {}] {}",
                Utc::now().to_rfc3339(),
                record.level(),
                record.target(),
                message
            ))
        })
        .level(log_level)
        .chain(std::io::stdout())
        .apply()?;
    Ok(())
}
