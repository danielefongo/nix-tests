use std::sync::Arc;

use anyhow::bail;
use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};

use crate::{
    files::{FindSearchTestFiles, RgSearchTestFiles, SearchTestFiles},
    reports::{HumanReporter, JsonReporter},
    runners::{NixTestRunner, TestSuiteRunner},
};

mod files;
mod reports;
mod runners;

#[derive(Debug, Parser)]
#[command(name = "nix-tests")]
#[command(about = "Nix testing utilities")]
#[command(
    long_about = "A lightweight testing framework for Nix, written in Rust.\n\n\
    Requires either 'rg' (ripgrep, preferred) or 'find' to discover test files.\n\
    Automatically uses 'rg' if available, otherwise falls back to 'find'."
)]
struct Args {
    #[arg(long, value_enum, default_value_t = ReportFormat::Human)]
    format: ReportFormat,

    #[arg(default_value = ".", num_args = 0..)]
    paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ValueEnum)]
enum ReportFormat {
    Json,
    Human,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let searcher: Box<dyn SearchTestFiles> = if command_exists("rg") {
        Box::new(RgSearchTestFiles)
    } else if command_exists("find") {
        Box::new(FindSearchTestFiles)
    } else {
        bail!("Neither 'rg' nor 'find' command found in the system");
    };

    let test_files = searcher.search_test_files(args.paths)?;

    let report = match args.format {
        ReportFormat::Human => {
            TestSuiteRunner::new(Arc::new(NixTestRunner::new()), Arc::new(HumanReporter))
                .run(&test_files)
                .await
        }
        ReportFormat::Json => {
            TestSuiteRunner::new(Arc::new(NixTestRunner::new()), Arc::new(JsonReporter))
                .run(&test_files)
                .await
        }
    };

    if report.has_issues() {
        std::process::exit(1);
    }

    Ok(())
}

fn command_exists(cmd: &str) -> bool {
    std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {}", cmd))
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}
