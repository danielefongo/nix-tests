use std::{env, future::Future, sync::Arc, time::Instant};

use futures::{stream, StreamExt};
use tokio::process::Command;

use crate::{
    files::TestFile,
    reports::{
        ReportEvent, Reporter, TestFileCompletedReport, TestFileErroredReport, TestFileReport,
        TestReport, TestSuiteReport,
    },
};

pub mod config {
    use serde::{Deserialize, Serialize};

    #[derive(Default, Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
    #[serde(rename_all = "kebab-case")]
    pub struct Config {
        #[serde(default)]
        pub num_threads: NumThreads,
    }

    #[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
    #[serde(transparent)]
    pub struct NumThreads(usize);

    impl NumThreads {
        pub fn new(num: usize) -> Self {
            Self(num)
        }

        pub fn get(&self) -> usize {
            self.0
        }
    }

    impl Default for NumThreads {
        fn default() -> Self {
            Self(num_cpus::get())
        }
    }

    impl From<usize> for NumThreads {
        fn from(value: usize) -> Self {
            Self(value)
        }
    }
}

pub trait TestFileRunner {
    fn run(&self, test_file: String) -> impl Future<Output = TestFileReport> + Send;
}

#[cfg(test)]
mockall::mock! {
    pub TestFileRunner {}
    impl TestFileRunner for TestFileRunner {
        fn run(&self, test_file: String) -> impl Future<Output = TestFileReport> + Send;
    }
    impl Clone for TestFileRunner {
        fn clone(&self) -> Self {}
    }
}

#[derive(Clone)]
pub struct NixTestRunner {
    nix_tests_path: String,
}

impl NixTestRunner {
    pub fn new() -> Self {
        let nix_tests_path = env::var("NIX_TESTS_LIB_PATH")
            .expect("NIX_TESTS_LIB_PATH environment variable not set");

        Self { nix_tests_path }
    }
}

impl TestFileRunner for NixTestRunner {
    async fn run(&self, test_file: String) -> TestFileReport {
        let start = Instant::now();

        let nix_tests = format!("import {} {{}}", self.nix_tests_path);

        let output = Command::new("nix-instantiate")
            .args(["--eval", "--strict", "--json", &test_file])
            .args(["--arg", "nix-tests", &nix_tests])
            .args(["-A", "tests"])
            .output()
            .await;

        let elapsed = start.elapsed().as_millis();

        match output {
            Ok(output) if output.status.success() => {
                match serde_json::from_slice::<Vec<TestReport>>(&output.stdout) {
                    Ok(reports) => TestFileReport::Completed(TestFileCompletedReport {
                        file: test_file,
                        tests: reports,
                        elapsed,
                    }),
                    Err(e) => TestFileReport::Errored(TestFileErroredReport {
                        file: test_file,
                        error: format!("Failed to deserialize test report: {}", e),
                        elapsed,
                    }),
                }
            }
            Ok(output) => TestFileReport::Errored(TestFileErroredReport {
                file: test_file,
                error: String::from_utf8_lossy(&output.stderr).to_string(),
                elapsed,
            }),
            Err(e) => TestFileReport::Errored(TestFileErroredReport {
                file: test_file,
                error: format!("Failed to execute nix-instantiate: {}", e),
                elapsed,
            }),
        }
    }
}

pub struct TestSuiteRunner<TR: TestFileRunner, R: Reporter> {
    pub test_runner: Arc<TR>,
    pub reporter: R,
    pub config: config::Config,
}

impl<TR, R> TestSuiteRunner<TR, R>
where
    TR: TestFileRunner + Send + Sync + 'static,
    R: Reporter + Send + Sync,
{
    pub fn new(test_runner: Arc<TR>, reporter: R, run_config: config::Config) -> Self {
        Self {
            test_runner,
            reporter,
            config: run_config,
        }
    }

    pub async fn run(&self, test_files: &[TestFile]) -> TestSuiteReport {
        let start = std::time::Instant::now();

        let file_reports: Vec<TestFileReport> = stream::iter(test_files)
            .filter_map(|tf| async move {
                match tf {
                    TestFile::Valid(path) => Some(path.clone()),
                    TestFile::NotFound(path) => {
                        self.report(&ReportEvent::TestFileNotFound(path.clone()));
                        None
                    }
                    TestFile::Invalid(path) => {
                        self.report(&ReportEvent::TestFileInvalid(path.clone()));
                        None
                    }
                }
            })
            .map(|path| {
                let runner = self.test_runner.clone();
                async move { runner.run(path).await }
            })
            .buffer_unordered(self.config.num_threads.get())
            .inspect(|report| {
                self.report(&ReportEvent::TestFileCompleted(report.clone()));
            })
            .collect()
            .await;

        let elapsed = start.elapsed().as_millis();
        let suite_report = TestSuiteReport::new(file_reports, elapsed);

        self.report(&ReportEvent::TestSuiteCompleted(suite_report.clone()));

        suite_report
    }

    fn report(&self, event: &ReportEvent) {
        if let Some(message) = self.reporter.on(event) {
            print!("{}", message);
        }
    }
}

#[cfg(test)]
mod test_suite_runner_tests {
    use std::sync::Arc;

    use futures::FutureExt;
    use mockall::{predicate::eq, Sequence};

    use super::*;
    use crate::{
        reports::{
            MockReporter, ReportEvent, TestFileCompletedReport, TestFileReport, TestSuiteReport,
        },
        runners::config::Config,
    };

    #[tokio::test]
    async fn it_runs_valid_tests() {
        let mut test_runner = MockTestFileRunner::new();
        test_runner
            .expect_run()
            .withf(|file| file == "my_test.nix")
            .once()
            .return_once(|_| {
                async {
                    TestFileReport::Completed(TestFileCompletedReport {
                        file: "my_test.nix".to_string(),
                        tests: vec![],
                        elapsed: 0,
                    })
                }
                .boxed()
            });

        let mut sequence = Sequence::new();

        let mut reporter = MockReporter::new();
        reporter
            .expect_on()
            .in_sequence(&mut sequence)
            .once()
            .with(eq(ReportEvent::TestFileCompleted(
                TestFileReport::Completed(TestFileCompletedReport {
                    file: "my_test.nix".to_string(),
                    tests: vec![],
                    elapsed: 0,
                }),
            )))
            .returning(|_event| None);
        reporter
            .expect_on()
            .once()
            .in_sequence(&mut sequence)
            .with(eq(ReportEvent::TestSuiteCompleted(TestSuiteReport::new(
                vec![TestFileReport::Completed(TestFileCompletedReport {
                    file: "my_test.nix".to_string(),
                    tests: vec![],
                    elapsed: 0,
                })],
                0,
            ))))
            .returning(|_event| None);

        let suite_runner = TestSuiteRunner::new(Arc::new(test_runner), reporter, Config::default());

        suite_runner
            .run(&[crate::files::TestFile::Valid("my_test.nix".to_string())])
            .await;
    }

    #[tokio::test]
    async fn it_skips_invalid_and_not_found_tests() {
        let mut test_runner = MockTestFileRunner::new();
        test_runner.expect_run().never();

        let mut sequence = Sequence::new();
        let mut reporter = MockReporter::new();
        reporter
            .expect_on()
            .in_sequence(&mut sequence)
            .once()
            .with(eq(ReportEvent::TestFileNotFound("missing.nix".to_string())))
            .returning(|_event| None);
        reporter
            .expect_on()
            .in_sequence(&mut sequence)
            .once()
            .with(eq(ReportEvent::TestFileInvalid("invalid.nix".to_string())))
            .returning(|_event| None);
        reporter
            .expect_on()
            .once()
            .in_sequence(&mut sequence)
            .with(eq(ReportEvent::TestSuiteCompleted(TestSuiteReport::new(
                vec![],
                0,
            ))))
            .returning(|_event| None);

        let suite_runner = TestSuiteRunner::new(Arc::new(test_runner), reporter, Config::default());

        suite_runner
            .run(&[
                crate::files::TestFile::NotFound("missing.nix".to_string()),
                crate::files::TestFile::Invalid("invalid.nix".to_string()),
            ])
            .await;
    }
}

#[cfg(test)]
mod runner_tests {
    use std::io::Write;

    use assert2::{check, let_assert};
    use tempfile::NamedTempFile;

    use super::*;
    use crate::reports::{CheckReport, TestFileReport, TestReport};

    fn create_temp_nix_file(content: &str) -> (NamedTempFile, String) {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let path = file.path().to_str().unwrap().to_string();

        (file, path)
    }

    #[tokio::test]
    async fn it_runs_a_simple_test_file() {
        let (_file, path) = create_temp_nix_file(
            r#"{
  pkgs ? import <nixpkgs> { },
  nix-tests,
}:
let
  lib = pkgs.lib;

  # Custom check
  isEven = x: if lib.mod x 2 == 0 then true else "${builtins.toString x} is not even";
in
nix-tests.runTests {
  "success" = {
    context = _: {
      num = 42;
    };
    checks = helpers: ctx: {
      "number equals 42" = helpers.isEq ctx.num 42;
      "number is even" = helpers.check isEven ctx.num;
    };
  };

  "failure" = {
    context = _: { };
    checks = helpers: _: {
      "failed check" = helpers.isTrue false;
    };
  };
}
"#,
        );

        let report = NixTestRunner::new().run(path.clone()).await;

        let_assert!(TestFileReport::Completed(file_report) = report);
        check!(file_report.file == path);
        check!(file_report.elapsed > 0);
        check!(
            file_report.tests
                == vec![
                    TestReport {
                        success: true,
                        path: vec!["success".to_string()],
                        location: format!("{}:12", path),
                        checks: vec![
                            CheckReport {
                                name: "number equals 42".to_string(),
                                success: true,
                                failure: None,
                                location: format!("{}:17", path),
                            },
                            CheckReport {
                                name: "number is even".to_string(),
                                success: true,
                                failure: None,
                                location: format!("{}:18", path),
                            },
                        ]
                    },
                    TestReport {
                        success: false,
                        path: vec!["failure".to_string()],
                        location: format!("{}:22", path),
                        checks: vec![CheckReport {
                            name: "failed check".to_string(),
                            success: false,
                            failure: Some("Expected: true\nGot: false".to_string()),
                            location: format!("{}:25", path),
                        }]
                    },
                ]
        );
    }

    #[tokio::test]
    async fn it_runs_a_groups_test_file() {
        let (_file, path) = create_temp_nix_file(
            r#"{
  pkgs ? import <nixpkgs> { },
  nix-tests,
}:
nix-tests.runTests {
  "group 1" = {
    "test 1" = {
      context = _: { };
      checks = helpers: _: {
        "check 1" = helpers.isTrue true;
      };
    };
    "test 2" = {
      context = _: { };
      checks = helpers: _: {
        "check 2" = helpers.isTrue true;
      };
    };
  };
  "group 2" = {
    "test 3" = {
      context = _: { };
      checks = helpers: _: {
        "check 3" = helpers.isTrue true;
      };
    };
  };
}
"#,
        );

        let report = NixTestRunner::new().run(path.clone()).await;

        let_assert!(TestFileReport::Completed(file_report) = report);
        check!(file_report.file == path);
        check!(file_report.elapsed > 0);
        check!(
            file_report.tests
                == vec![
                    TestReport {
                        success: true,
                        path: vec!["group 1".to_string(), "test 1".to_string()],
                        location: format!("{}:7", path),
                        checks: vec![CheckReport {
                            name: "check 1".to_string(),
                            success: true,
                            failure: None,
                            location: format!("{}:10", path),
                        },]
                    },
                    TestReport {
                        success: true,
                        path: vec!["group 1".to_string(), "test 2".to_string()],
                        location: format!("{}:13", path),
                        checks: vec![CheckReport {
                            name: "check 2".to_string(),
                            success: true,
                            failure: None,
                            location: format!("{}:16", path),
                        },]
                    },
                    TestReport {
                        success: true,
                        path: vec!["group 2".to_string(), "test 3".to_string()],
                        location: format!("{}:21", path),
                        checks: vec![CheckReport {
                            name: "check 3".to_string(),
                            success: true,
                            failure: None,
                            location: format!("{}:24", path),
                        },]
                    }
                ]
        );
    }

    #[tokio::test]
    async fn it_handles_nix_evaluation_errors() {
        let (_file, path) = create_temp_nix_file("invalid_nix_syntax_here");

        let report = NixTestRunner::new().run(path.clone()).await;

        let_assert!(TestFileReport::Errored(file_report) = report);
        check!(file_report.file == path);
        check!(file_report.error.contains("error:"));
        check!(file_report.elapsed > 0);
    }

    #[tokio::test]
    async fn it_handles_malformed_json_structure() {
        let (_file, path) = create_temp_nix_file(
            r#"{
  pkgs ? import <nixpkgs> { },
  nix-tests,
}:
{
  tests = {
    foo = "bar";
    baz = 123;
  };
}
"#,
        );

        let report = NixTestRunner::new().run(path.clone()).await;

        let_assert!(TestFileReport::Errored(file_report) = report);
        check!(file_report.file == path);
        check!(file_report.elapsed > 0);
        check!(file_report
            .error
            .contains("Failed to deserialize test report"));
    }

    #[tokio::test]
    async fn it_handles_command_execution_failure() {
        let invalid_path = "test\0file.nix".to_string();
        let report = NixTestRunner::new().run(invalid_path.clone()).await;

        let_assert!(TestFileReport::Errored(file_report) = report);
        check!(file_report.file == invalid_path);
        check!(file_report.elapsed >= 0);
        check!(file_report
            .error
            .contains("Failed to execute nix-instantiate"));
    }
}
