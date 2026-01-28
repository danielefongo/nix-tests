use std::{path::Path, sync::Arc};

use anyhow::bail;
use clap::{Args as ClapArgs, Parser, ValueEnum};
use tokio::signal::unix::{signal, SignalKind};

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

#[derive(Default, Debug, ClapArgs, Clone)]
#[command(next_help_heading = "[runner] options")]
pub struct RunnerArgs {
    #[arg(
        long,
        help = "Number of threads to use for running tests (1-1024, default: number of CPU cores)",
        value_parser = clap::builder::RangedU64ValueParser::<usize>::new().range(1..=1024)
    )]
    num_threads: Option<usize>,

    #[arg(
        long,
        help = "Timeout in milliseconds for each test file (0 for no timeout)"
    )]
    timeout: Option<u64>,
}

#[derive(Default, Debug, ClapArgs, Clone)]
#[command(next_help_heading = "[report] options")]
pub struct ReportArgs {
    #[arg(long, value_enum, help = "Output format for test results")]
    format: Option<Format>,

    #[arg(
        long,
        help = "Hide individual reports for succeeded test files",
        value_name = "BOOL",
        default_missing_value = "true"
    )]
    hide_succeeded: Option<bool>,

    #[arg(
        long,
        help = "Hide individual reports for failed test files",
        value_name = "BOOL",
        default_missing_value = "true"
    )]
    hide_failed: Option<bool>,

    #[arg(
        long,
        help = "Hide individual reports for errored test files",
        value_name = "BOOL",
        default_missing_value = "true"
    )]
    hide_errored: Option<bool>,
}

#[derive(Default, Debug, ClapArgs, Clone)]
pub struct ConfigArgs {
    #[command(flatten)]
    runner: RunnerArgs,

    #[command(flatten)]
    report: ReportArgs,
}

impl ConfigArgs {
    pub fn apply_to(&self, base: Config) -> Config {
        Config {
            runner: runner_config::Config {
                num_threads: self
                    .runner
                    .num_threads
                    .map(Into::into)
                    .unwrap_or(base.runner.num_threads),
                timeout: self.runner.timeout.unwrap_or(base.runner.timeout),
            },
            report: report_config::Config {
                format: self
                    .report
                    .format
                    .clone()
                    .map(Into::into)
                    .unwrap_or(base.report.format),
                hide_succeeded: self
                    .report
                    .hide_succeeded
                    .unwrap_or(base.report.hide_succeeded),
                hide_failed: self.report.hide_failed.unwrap_or(base.report.hide_failed),
                hide_errored: self.report.hide_errored.unwrap_or(base.report.hide_errored),
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

    #[arg(long, help = "Show the loaded config and exit")]
    show: bool,

    #[command(flatten)]
    config_args: ConfigArgs,

    #[arg(default_value = ".", num_args = 0..)]
    paths: Vec<String>,
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

    let runner = TestSuiteRunner::new(
        Arc::new(NixTestRunner::new(config.runner.timeout)),
        ConfigurableReporter::new(&config.report),
        config.runner,
    );

    tokio::select! {
        report = runner.run(&test_files) => {
            if report.has_issues() {
                std::process::exit(1);
            }
        }
        code = shutdown_signal() => {
            std::process::exit(code);
        }
    }

    Ok(())
}

fn load_config(args: &Args) -> anyhow::Result<Config> {
    let file_config = if let Some(config_path) = &args.config {
        let path = Path::new(&config_path).canonicalize()?;
        let config_file = path.join(".nix-tests.toml");
        if config_file.exists() {
            Config::try_from(config_file)?
        } else {
            Config::search_in(&path)?.unwrap_or_default()
        }
    } else {
        Config::search()?.unwrap_or_default()
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

async fn shutdown_signal() -> i32 {
    let mut sigint = signal(SignalKind::interrupt()).unwrap();
    let mut sigterm = signal(SignalKind::terminate()).unwrap();
    let mut sighup = signal(SignalKind::hangup()).unwrap();

    tokio::select! {
        _ = sigint.recv() => 130,
        _ = sigterm.recv() => 143,
        _ = sighup.recv() => 129,
    }
}

#[cfg(test)]
mod config_args_tests {
    use assert2::check;

    use crate::{
        config::Config, reports::config as report_config, runners::config as runner_config,
        ConfigArgs, Format, ReportArgs, RunnerArgs,
    };

    #[test]
    fn it_applies_cli_args_to_config() {
        let file_config = Config {
            runner: runner_config::Config {
                num_threads: runner_config::NumThreads::new(8),
                timeout: 0,
            },
            report: report_config::Config {
                format: report_config::Format::Human,
                hide_succeeded: false,
                hide_failed: false,
                hide_errored: false,
            },
        };

        let args = ConfigArgs {
            runner: RunnerArgs {
                num_threads: Some(4),
                timeout: None,
            },
            ..Default::default()
        };

        let merged = args.apply_to(file_config);
        check!(
            merged
                == Config {
                    runner: runner_config::Config {
                        num_threads: runner_config::NumThreads::new(4),
                        timeout: 0,
                    },
                    report: report_config::Config {
                        format: report_config::Format::Human,
                        hide_succeeded: false,
                        hide_failed: false,
                        hide_errored: false,
                    }
                }
        );
    }

    #[test]
    fn it_preserves_config_when_no_cli_args() {
        let file_config = Config {
            runner: runner_config::Config {
                num_threads: runner_config::NumThreads::new(8),
                timeout: 0,
            },
            report: report_config::Config {
                format: report_config::Format::Json,
                hide_succeeded: false,
                hide_failed: false,
                hide_errored: false,
            },
        };

        let args = ConfigArgs::default();

        let merged = args.apply_to(file_config.clone());
        check!(merged == file_config);
    }

    #[test]
    fn it_overrides_all_fields_when_all_cli_args_present() {
        let file_config = Config {
            runner: runner_config::Config {
                num_threads: runner_config::NumThreads::new(8),
                timeout: 0,
            },
            report: report_config::Config {
                format: report_config::Format::Human,
                hide_succeeded: false,
                hide_failed: false,
                hide_errored: false,
            },
        };

        let args = ConfigArgs {
            runner: RunnerArgs {
                num_threads: Some(12),
                timeout: None,
            },
            report: ReportArgs {
                format: Some(Format::Json),
                hide_succeeded: Some(true),
                hide_failed: Some(false),
                hide_errored: Some(true),
            },
        };

        let merged = args.apply_to(file_config);
        check!(
            merged
                == Config {
                    runner: runner_config::Config {
                        num_threads: runner_config::NumThreads::new(12),
                        timeout: 0,
                    },
                    report: report_config::Config {
                        format: report_config::Format::Json,
                        hide_succeeded: true,
                        hide_failed: false,
                        hide_errored: true,
                    }
                }
        );
    }
}
