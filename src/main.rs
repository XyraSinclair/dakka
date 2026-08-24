mod engine;
mod model;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "dakka", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Doctor,
    Pack {
        #[arg(long)]
        composition: String,
        #[arg(long)]
        ask: Option<String>,
        #[arg(long)]
        plan_file: Option<PathBuf>,
    },
    Run {
        composition: String,
        #[arg(long)]
        ask: Option<String>,
        #[arg(long)]
        plan_file: Option<PathBuf>,
        #[arg(long)]
        n: Option<usize>,
        #[arg(long, default_value = "PLAN.md")]
        out: PathBuf,
    },
    Plan {
        #[arg(long)]
        ask: String,
    },
    Fresh {
        #[arg(long, default_value = "PLAN.md")]
        plan_file: PathBuf,
    },
    Climb {
        #[arg(long)]
        file: PathBuf,
    },
    Replan {
        #[arg(long, default_value = "PLAN.md")]
        plan_file: PathBuf,
    },
    Judge {
        #[arg(required = true, num_args = 2..)]
        file: Vec<PathBuf>,
        #[arg(long)]
        ask: Option<String>,
    },
    Bench {
        #[arg(long)]
        operator: String,
        #[arg(long)]
        file: PathBuf,
    },
    Ledger,
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Commands::Doctor => {
            if !engine::doctor()? {
                bail!("default harness command is missing");
            }
        }
        Commands::Pack { composition, ask, plan_file } => {
            let loaded = engine::load(&composition)?;
            engine::pack(&loaded, ask, read_optional(plan_file.as_deref())?, None)?;
        }
        Commands::Run { composition, ask, plan_file, n, out } => {
            run(&composition, ask, read_optional(plan_file.as_deref())?, n, &out)?;
        }
        Commands::Plan { ask } => run("deep-plan", Some(ask), None, None, Path::new("PLAN.md"))?,
        Commands::Fresh { plan_file } => {
            run("fresh", None, Some(read_required(&plan_file)?), None, Path::new("PLAN.md"))?;
        }
        Commands::Climb { file } => {
            run("climb", None, Some(read_required(&file)?), None, Path::new("PLAN.md"))?;
        }
        Commands::Replan { plan_file } => {
            run("replan", None, Some(read_required(&plan_file)?), None, Path::new("PLAN.md"))?;
        }
        Commands::Judge { file, ask } => {
            let loaded = engine::load("deep-plan")?;
            engine::standalone_judge(&loaded, &file, ask)?;
        }
        Commands::Bench { operator, file } => {
            let loaded = engine::load("deep-plan")?;
            engine::bench(&loaded, &operator, &file)?;
        }
        Commands::Ledger => engine::print_ledger()?,
    }
    Ok(())
}

fn run(composition: &str, ask: Option<String>, plan: Option<String>, n: Option<usize>, out: &Path) -> Result<()> {
    let loaded = engine::load(composition)?;
    let path = engine::run_composition(&loaded, ask, plan, n, out)?;
    println!("plan: {}", path.canonicalize().unwrap_or_else(|_| path.to_path_buf()).display());
    Ok(())
}

fn read_optional(path: Option<&Path>) -> Result<Option<String>> {
    path.map(read_required).transpose()
}

fn read_required(path: &Path) -> Result<String> {
    fs::read_to_string(path).with_context(|| format!("read plan file {}", path.display()))
}
