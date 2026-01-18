use std::{env, process::Command, sync::Arc};

use futures::{stream, StreamExt};

use crate::{
    files::TestFile,
    reports::{
        ReportEvent, Reporter, TestFileCompletedReport, TestFileErroredReport, TestFileReport,
        TestSuiteReport,
    },
};

pub trait TestFileRunner {
    fn run(&self, test_file: String) -> TestFileReport;
}

#[cfg(test)]
mockall::mock! {
    pub TestFileRunner {}
    impl TestFileRunner for TestFileRunner {
        fn run(&self, test_file: String) -> TestFileReport;
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
    fn run(&self, test_file: String) -> TestFileReport {
        let nix_tests = format!(
            "import {} {{ lib = (import <nixpkgs> {{}}).lib; }}",
            self.nix_tests_path
        );

        let output = Command::new("nix-instantiate")
            .args(["--eval", "--strict", "--json", &test_file])
            .args(["--arg", "nix-tests", &nix_tests])
            .args(["-A", "result"])
            .output();

        match output {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let data: serde_json::Value =
                    serde_json::from_str(&stdout).expect("Failed to parse JSON");

                serde_json::from_value::<TestFileCompletedReport>(data)
                    .map(|report| report.with_file(test_file.clone()))
                    .map(TestFileReport::Completed)
                    .unwrap_or_else(|e| {
                        TestFileReport::Errored(TestFileErroredReport {
                            file: test_file.clone(),
                            error: format!("Failed to deserialize test report: {}", e),
                        })
                    })
            }
            Ok(output) => TestFileReport::Errored(TestFileErroredReport {
                file: test_file,
                error: String::from_utf8_lossy(&output.stderr).to_string(),
            }),
            Err(e) => TestFileReport::Errored(TestFileErroredReport {
                file: test_file.to_string(),
                error: format!("Failed to execute nix-instantiate: {}", e),
            }),
        }
    }
}

pub struct TestSuiteRunner<TR: TestFileRunner, R: Reporter> {
    pub test_runner: Arc<TR>,
    pub reporter: Arc<R>,
}

impl<TR, R> TestSuiteRunner<TR, R>
where
    TR: TestFileRunner + Send + Sync + 'static,
    R: Reporter + Send + Sync + 'static,
{
    pub fn new(test_runner: Arc<TR>, reporter: Arc<R>) -> Self {
        Self {
            test_runner,
            reporter,
        }
    }

    pub async fn run(&self, test_files: &[TestFile]) -> TestSuiteReport {
        let file_reports: Vec<TestFileReport> = stream::iter(test_files)
            .filter_map(|tf| async move {
                match tf {
                    TestFile::Valid(path) => Some(path.clone()),
                    TestFile::NotFound(path) => {
                        self.reporter
                            .on(&ReportEvent::TestFileNotFound(path.clone()));
                        None
                    }
                    TestFile::Invalid(path) => {
                        self.reporter
                            .on(&ReportEvent::TestFileInvalid(path.clone()));
                        None
                    }
                }
            })
            .map(|path| {
                let runner = self.test_runner.clone();
                tokio::spawn(async move { runner.run(path) })
            })
            .buffer_unordered(10)
            .filter_map(|result| async move { result.ok() })
            .inspect(|report| {
                self.reporter
                    .on(&ReportEvent::TestCompleted(report.clone()));
            })
            .collect()
            .await;

        let suite_report = TestSuiteReport::new(file_reports);

        self.reporter
            .on(&ReportEvent::TestSuiteCompleted(suite_report.clone()));

        suite_report
    }
}

#[cfg(test)]
mod test_suite_runner_tests {
    use std::sync::Arc;

    use mockall::{predicate::eq, Sequence};

    use super::*;
    use crate::reports::{
        MockReporter, ReportEvent, TestFileCompletedReport, TestFileReport, TestSuiteReport,
    };

    #[tokio::test]
    async fn it_runs_valid_tests() {
        let mut test_runner = MockTestFileRunner::new();
        test_runner
            .expect_run()
            .withf(|file| file == "my_test.nix")
            .once()
            .returning(|_| {
                TestFileReport::Completed(TestFileCompletedReport {
                    file: "my_test.nix".to_string(),
                    tests: vec![],
                })
            });

        let mut sequence = Sequence::new();

        let mut reporter = MockReporter::new();
        reporter
            .expect_on()
            .in_sequence(&mut sequence)
            .once()
            .with(eq(ReportEvent::TestCompleted(TestFileReport::Completed(
                TestFileCompletedReport {
                    file: "my_test.nix".to_string(),
                    tests: vec![],
                },
            ))))
            .returning(|_event| {});
        reporter
            .expect_on()
            .once()
            .in_sequence(&mut sequence)
            .with(eq(ReportEvent::TestSuiteCompleted(TestSuiteReport::new(
                vec![TestFileReport::Completed(TestFileCompletedReport {
                    file: "my_test.nix".to_string(),
                    tests: vec![],
                })],
            ))))
            .returning(|_event| {});

        let suite_runner = TestSuiteRunner::new(Arc::new(test_runner), Arc::new(reporter));

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
            .returning(|_event| {});
        reporter
            .expect_on()
            .in_sequence(&mut sequence)
            .once()
            .with(eq(ReportEvent::TestFileInvalid("invalid.nix".to_string())))
            .returning(|_event| {});
        reporter
            .expect_on()
            .once()
            .in_sequence(&mut sequence)
            .with(eq(ReportEvent::TestSuiteCompleted(TestSuiteReport::new(
                vec![],
            ))))
            .returning(|_event| {});

        let suite_runner = TestSuiteRunner::new(Arc::new(test_runner), Arc::new(reporter));

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
    use super::*;
    use crate::reports::{CheckReport, TestFileCompletedReport, TestFileReport, TestReport};
    use assert2::{check, let_assert};
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_temp_nix_file(content: &str) -> (NamedTempFile, String) {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();

        let path = file.path().to_str().unwrap().to_string();

        (file, path)
    }

    #[test]
    fn it_runs_a_simple_test_file() {
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
{
  result = nix-tests.runTests [
    (nix-tests.test "success" {
      context = {
        num = 42;
      };
      checks = helpers: ctx: [
        (helpers.isEq "number equals 42" ctx.num 42)
        (helpers.check "number is even" isEven ctx.num)
      ];
    })

    (nix-tests.test "failure" {
      context = { };
      checks = helpers: _: [
        (helpers.isTrue "failed check" false)
      ];
    })
  ];
}
"#,
        );

        let report = NixTestRunner::new().run(path.clone());

        check!(
            report
                == TestFileReport::Completed(TestFileCompletedReport {
                    file: path.clone(),
                    tests: vec![
                        TestReport {
                            success: true,
                            path: vec!["success".to_string()],
                            location: format!("{}:17", path),
                            checks: vec![
                                CheckReport {
                                    name: "number equals 42".to_string(),
                                    success: true,
                                    failure: None,
                                },
                                CheckReport {
                                    name: "number is even".to_string(),
                                    success: true,
                                    failure: None,
                                },
                            ]
                        },
                        TestReport {
                            success: false,
                            path: vec!["failure".to_string()],
                            location: format!("{}:25", path),
                            checks: vec![CheckReport {
                                name: "failed check".to_string(),
                                success: false,
                                failure: Some("Expected: true\nGot: false".to_string()),
                            }]
                        }
                    ],
                })
        );
    }

    #[test]
    fn it_runs_a_groups_test_file() {
        let (_file, path) = create_temp_nix_file(
            r#"{
  pkgs ? import <nixpkgs> { },
  nix-tests,
}:
{
  result = nix-tests.runTests [
    (nix-tests.group "group 1" [
      (nix-tests.test "test 1" {
        context = { };
        checks = helpers: _: [
          (helpers.isTrue "check 1" true)
        ];
      })
      (nix-tests.test "test 2" {
        context = { };
        checks = helpers: _: [
          (helpers.isTrue "check 2" true)
        ];
      })
    ])
    (nix-tests.group "group 2" [
      (nix-tests.test "test 3" {
        context = { };
        checks = helpers: _: [
          (helpers.isTrue "check 3" true)
        ];
      })
    ])
  ];
}
"#,
        );

        let report = NixTestRunner::new().run(path.clone());

        check!(
            report
                == TestFileReport::Completed(TestFileCompletedReport {
                    file: path.clone(),
                    tests: vec![
                        TestReport {
                            success: true,
                            path: vec!["group 1".to_string(), "test 1".to_string()],
                            location: format!("{}:10", path),
                            checks: vec![CheckReport {
                                name: "check 1".to_string(),
                                success: true,
                                failure: None,
                            },]
                        },
                        TestReport {
                            success: true,
                            path: vec!["group 1".to_string(), "test 2".to_string()],
                            location: format!("{}:16", path),
                            checks: vec![CheckReport {
                                name: "check 2".to_string(),
                                success: true,
                                failure: None,
                            },]
                        },
                        TestReport {
                            success: true,
                            path: vec!["group 2".to_string(), "test 3".to_string()],
                            location: format!("{}:24", path),
                            checks: vec![CheckReport {
                                name: "check 3".to_string(),
                                success: true,
                                failure: None,
                            },]
                        }
                    ],
                })
        );
    }

    #[test]
    fn it_handles_nix_evaluation_errors() {
        let (_file, path) = create_temp_nix_file("invalid_nix_syntax_here");

        let report = NixTestRunner::new().run(path.clone());

        let_assert!(TestFileReport::Errored(TestFileErroredReport { file, error }) = report);
        check!(file == path);
        check!(error.contains("error:"));
    }

    #[test]
    fn it_handles_malformed_json_structure() {
        let (_file, path) = create_temp_nix_file(
            r#"{
  pkgs ? import <nixpkgs> { },
  nix-tests,
}:
{
  result = {
    foo = "bar";
    baz = 123;
  };
}
"#,
        );

        let report = NixTestRunner::new().run(path.clone());

        let_assert!(TestFileReport::Errored(TestFileErroredReport { file, error }) = report);
        check!(file == path);
        check!(error.contains("Failed to deserialize test report"));
    }

    #[test]
    fn it_handles_command_execution_failure() {
        let invalid_path = "test\0file.nix".to_string();
        let report = NixTestRunner::new().run(invalid_path.clone());

        let_assert!(TestFileReport::Errored(TestFileErroredReport { file, error }) = report);
        check!(file == invalid_path);
        check!(error.contains("Failed to execute nix-instantiate"));
    }
}
