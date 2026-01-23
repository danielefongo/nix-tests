use std::{path::Path, sync::Arc};

use anyhow::bail;
use clap::{Args as ClapArgs, Parser, ValueEnum};

use crate::{
    config::Config,
    files::{FindSearchTestFiles, RgSearchTestFiles, SearchTestFiles, TestFile},
    reports::{config as report_config, ConfigurableReporter},
    runners::{config as runner_config, NixTestRunner, TestSuiteRunner},
};

mod config;
mod files;
mod reports;
mod runners;

#[derive(Debug, Clone, ValueEnum, PartialEq, Eq)]
#[value(rename_all = "kebab-case")]
pub enum Format {
    Json,
    Human,
}

impl From<Format> for report_config::Format {
    fn from(value: Format) -> Self {
        match value {
            Format::Json => report_config::Format::Json,
            Format::Human => report_config::Format::Human,
        }
    }
}

#[derive(Debug, ClapArgs, Clone)]
pub struct ConfigArgs {
    #[arg(
        long,
        help = "Number of threads to use for running tests (1-1024, default: number of CPU cores)",
        value_parser = clap::builder::RangedU64ValueParser::<usize>::new().range(1..=1024)
    )]
    num_threads: Option<usize>,

    #[arg(long, value_enum, help = "Output format for test results")]
    format: Option<Format>,

    #[arg(long, help = "Timeout for each test in milliseconds")]
    timeout: Option<u64>,
}

impl ConfigArgs {
    pub fn apply_to(&self, base: Config) -> Config {
        Config {
            runner: runner_config::Config {
                num_threads: self
                    .num_threads
                    .map(Into::into)
                    .unwrap_or(base.runner.num_threads),
                timeout: self.timeout.or(base.runner.timeout),
            },
            report: report_config::Config {
                format: self
                    .format
                    .clone()
                    .map(Into::into)
                    .unwrap_or(base.report.format),
            },
        }
    }
}

#[derive(Debug, Parser)]
#[command(name = "nix-tests")]
#[command(about = "Nix testing utilities")]
#[command(
    long_about = "A lightweight testing framework for Nix, written in Rust.\n\n\
    Requires either 'rg' (ripgrep, preferred) or 'find' to discover test files.\n\
    Automatically uses 'rg' if available, otherwise falls back to 'find'."
)]
struct Args {
    #[arg(long, help = "Path to the configuration directory or file")]
    config: Option<String>,

    #[command(flatten)]
    config_args: ConfigArgs,

    #[arg(default_value = ".", num_args = 0..)]
    paths: Vec<String>,

    #[arg(long, help = "Show the loaded config and exit")]
    show: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let config = load_config(&args)?;

    if args.show {
        println!("{}", toml::to_string(&config)?);
        return Ok(());
    }

    let test_files = find_files(args.paths)?;

    let report = TestSuiteRunner::new(
        Arc::new(NixTestRunner::new()),
        ConfigurableReporter::new(&config.report),
        config.runner,
    )
    .run(&test_files)
    .await;

    if report.has_issues() {
        std::process::exit(1);
    }

    Ok(())
}

fn load_config(args: &Args) -> anyhow::Result<Config> {
    let Some(config_path) = &args.config else {
        return Ok(Config::default());
    };

    let path = Path::new(&config_path).canonicalize()?;
    let config_file = path.join(".nix-tests.toml");
    let file_config = if config_file.exists() {
        Config::try_from(config_file)?
    } else {
        Config::search_in(&path)?.unwrap_or_default()
    };

    let config = args.config_args.apply_to(file_config);

    Ok(config)
}

fn find_files(paths: Vec<String>) -> anyhow::Result<Vec<TestFile>> {
    let searcher: Box<dyn SearchTestFiles> = if command_exists("rg") {
        Box::new(RgSearchTestFiles)
    } else if command_exists("find") {
        Box::new(FindSearchTestFiles)
    } else {
        bail!("Neither 'rg' nor 'find' command found in the system");
    };

    searcher.search_test_files(paths)
}

fn command_exists(cmd: &str) -> bool {
    std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {}", cmd))
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod config_args_tests {
    use assert2::check;

    use crate::{
        config::Config, reports::config as report_config, runners::config as runner_config,
        ConfigArgs, Format,
    };

    #[test]
    fn it_applies_cli_args_to_config() {
        let file_config = Config {
            runner: runner_config::Config {
                num_threads: runner_config::NumThreads::new(8),
                timeout: Some(5000),
            },
            report: report_config::Config {
                format: report_config::Format::Human,
            },
        };

        let args = ConfigArgs {
            num_threads: Some(4),
            timeout: None,
            format: None,
        };

        let merged = args.apply_to(file_config);
        check!(
            merged
                == Config {
                    runner: runner_config::Config {
                        num_threads: runner_config::NumThreads::new(4),
                        timeout: Some(5000),
                    },
                    report: report_config::Config {
                        format: report_config::Format::Human,
                    }
                }
        );
    }

    #[test]
    fn it_preserves_config_when_no_cli_args() {
        let file_config = Config {
            runner: runner_config::Config {
                num_threads: runner_config::NumThreads::new(8),
                timeout: None,
            },
            report: report_config::Config {
                format: report_config::Format::Json,
            },
        };

        let args = ConfigArgs {
            num_threads: None,
            timeout: None,
            format: None,
        };

        let merged = args.apply_to(file_config.clone());
        check!(merged == file_config);
    }

    #[test]
    fn it_overrides_all_fields_when_all_cli_args_present() {
        let file_config = Config {
            runner: runner_config::Config {
                num_threads: runner_config::NumThreads::new(8),
                timeout: None,
            },
            report: report_config::Config {
                format: report_config::Format::Human,
            },
        };

        let args = ConfigArgs {
            num_threads: Some(12),
            timeout: Some(3000),
            format: Some(Format::Json),
        };

        let merged = args.apply_to(file_config);
        check!(
            merged
                == Config {
                    runner: runner_config::Config {
                        num_threads: runner_config::NumThreads::new(12),
                        timeout: Some(3000),
                    },
                    report: report_config::Config {
                        format: report_config::Format::Json,
                    }
                }
        );
    }
}
