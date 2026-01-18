use std::sync::Arc;

use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};

use crate::{
    files::FindTestFiles,
    reports::{HumanReporter, JsonReporter},
    runners::{NixTestRunner, TestSuiteRunner},
};

mod files;
mod reports;
mod runners;

#[derive(Debug, Parser)]
#[command(name = "nix-tests")]
#[command(about = "Nix testing utilities")]
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

    let test_files = args.paths.find_test_files()?;

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
