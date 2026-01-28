use serde::{Deserialize, Serialize};

use crate::reports::config::Format;

pub mod config {
    use serde::{Deserialize, Serialize};

    use crate::reports::TestFileReport;

    #[derive(Default, Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
    #[serde(rename_all = "kebab-case")]
    pub struct Config {
        #[serde(default)]
        pub format: Format,

        #[serde(default)]
        pub hide_succeeded: bool,

        #[serde(default)]
        pub hide_failed: bool,

        #[serde(default)]
        pub hide_errored: bool,
    }

    impl Config {
        pub fn should_hide_test_report(&self, report: &TestFileReport) -> bool {
            match report {
                TestFileReport::Completed(r) if r.failed_count() == 0 => self.hide_succeeded,
                TestFileReport::Completed(_) => self.hide_failed,
                TestFileReport::Errored(_) => self.hide_errored,
                TestFileReport::TimedOut(_) => self.hide_errored,
            }
        }
    }

    #[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    #[serde(rename_all = "kebab-case")]
    pub enum Format {
        #[default]
        Human,
        Json,
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TestSuiteReport {
    reports: Vec<TestFileReport>,
    elapsed: u128,
}

impl TestSuiteReport {
    pub fn new(reports: Vec<TestFileReport>, elapsed: u128) -> Self {
        Self { reports, elapsed }
    }
    fn processed_files(&self) -> usize {
        self.reports.len()
    }
    fn succeeded_files(&self) -> usize {
        self.reports
            .iter()
            .filter(|report| match report {
                TestFileReport::Completed(report) => report.failed_count() == 0,
                TestFileReport::Errored(_) => false,
                TestFileReport::TimedOut(_) => false,
            })
            .count()
    }
    fn failed_files(&self) -> usize {
        self.reports
            .iter()
            .filter(|report| match report {
                TestFileReport::Completed(report) => report.failed_count() > 0,
                TestFileReport::Errored(_) => false,
                TestFileReport::TimedOut(_) => false,
            })
            .count()
    }
    fn errored_files(&self) -> usize {
        self.reports
            .iter()
            .filter(|report| matches!(report, TestFileReport::Errored(_)))
            .count()
    }
    fn timed_out_files(&self) -> usize {
        self.reports
            .iter()
            .filter(|report| matches!(report, TestFileReport::TimedOut(_)))
            .count()
    }
    fn total_elapsed(&self) -> u128 {
        self.elapsed
    }
    pub fn has_issues(&self) -> bool {
        self.reports.iter().any(|report| match report {
            TestFileReport::Completed(report) => report.failed_count() > 0,
            TestFileReport::Errored(_) => true,
            TestFileReport::TimedOut(_) => true,
        })
    }
}

#[derive(Debug, Serialize, PartialEq, Eq, Clone)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TestFileReport {
    Completed(TestFileCompletedReport),
    Errored(TestFileErroredReport),
    TimedOut(TestFileTimedOutReport),
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct TestFileCompletedReport {
    pub tests: Vec<TestReport>,
    #[serde(skip_deserializing, default)]
    pub file: String,
    #[serde(skip_deserializing, default)]
    pub elapsed: u128,
}

impl TestFileCompletedReport {
    fn failed_count(&self) -> usize {
        self.tests
            .iter()
            .map(|test| test.checks.iter().filter(|check| !check.success).count())
            .sum()
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct TestReport {
    pub success: bool,
    pub path: Vec<String>,
    pub location: String,
    pub checks: Vec<CheckReport>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct CheckReport {
    pub name: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<String>,
    pub location: String,
}

#[derive(Debug, Serialize, PartialEq, Eq, Clone)]
pub struct TestFileErroredReport {
    pub file: String,
    pub error: String,
    pub elapsed: u128,
}

#[derive(Debug, Serialize, PartialEq, Eq, Clone)]
pub struct TestFileTimedOutReport {
    pub file: String,
    pub timeout: u64,
    pub elapsed: u128,
}

#[derive(Debug, PartialEq, Eq, Clone)]
#[allow(clippy::enum_variant_names)]
pub enum ReportEvent {
    TestFileNotFound(String),
    TestFileInvalid(String),
    TestFileCompleted(TestFileReport),
    TestSuiteCompleted(TestSuiteReport),
}

#[cfg_attr(test, mockall::automock)]
pub trait Reporter {
    fn on(&self, report_event: &ReportEvent) -> Option<String>;
}

pub struct JsonReporter {
    config: config::Config,
}

impl JsonReporter {
    pub fn new(config: config::Config) -> Self {
        Self { config }
    }
}

impl Reporter for JsonReporter {
    fn on(&self, report_event: &ReportEvent) -> Option<String> {
        if let ReportEvent::TestFileCompleted(report) = report_event {
            if self.config.should_hide_test_report(report) {
                None
            } else {
                Some(serde_json::to_string(report).unwrap())
            }
        } else {
            None
        }
    }
}

pub struct HumanReporter {
    config: config::Config,
}

impl HumanReporter {
    pub fn new(config: config::Config) -> Self {
        Self { config }
    }

    fn format_report(&self, result: &TestFileReport) -> String {
        let mut output = String::new();

        match result {
            TestFileReport::Completed(report) => {
                output.push_str(&format!("File: {} ({}ms)\n", report.file, report.elapsed));

                for test in &report.tests {
                    let path = test.path.join(" -> ");

                    for check in &test.checks {
                        if check.success {
                            output.push_str(&format!("✓ {} -> {}\n", path, check.name));
                        } else {
                            output.push_str(&format!("✗ {} -> {}\n", path, check.name));
                            if let Some(failure) = &check.failure {
                                output.push_str("    Failure:\n");
                                for line in failure.lines() {
                                    output.push_str(&format!("      {}\n", line));
                                }
                                output.push_str(&format!("      at {}\n", check.location));
                            } else {
                                output.push_str(&format!("    Failed at {}\n", check.location));
                            }
                        }
                    }
                }

                if report.failed_count() > 0 {
                    output.push_str(&format!("FAILED ({} failed)\n", report.failed_count()));
                }
                output.push('\n');
            }
            TestFileReport::Errored(report) => {
                output.push_str(&format!("File: {} ({}ms)\n", report.file, report.elapsed));
                output.push_str(&format!("ERROR: {}\n", report.error));
            }
            TestFileReport::TimedOut(report) => {
                output.push_str(&format!("File: {} ({}ms)\n", report.file, report.elapsed));
                output.push_str(&format!("TIMEOUT: Exceeded {}ms limit\n", report.timeout));
                output.push('\n');
            }
        }

        output
    }
}

impl Reporter for HumanReporter {
    fn on(&self, report_event: &ReportEvent) -> Option<String> {
        match report_event {
            ReportEvent::TestFileNotFound(path) => {
                Some(format!("Warning: '{path}' is not found, skipping.\n"))
            }
            ReportEvent::TestFileInvalid(path) => {
                Some(format!("Warning: '{path}' is not a test file, skipping.\n"))
            }
            ReportEvent::TestFileCompleted(report) => {
                if self.config.should_hide_test_report(report) {
                    None
                } else {
                    Some(self.format_report(report))
                }
            }
            ReportEvent::TestSuiteCompleted(report) => {
                let mut output = String::new();

                if report.processed_files() == 0 {
                    output.push_str("No test files found\n");
                } else if report.failed_files() == 0
                    && report.errored_files() == 0
                    && report.timed_out_files() == 0
                {
                    output.push_str(&format!(
                        "All tests passed ({}ms)\n",
                        report.total_elapsed()
                    ));
                } else {
                    output.push_str(&format!("{} file(s) succeeded\n", report.succeeded_files()));
                    if report.errored_files() > 0 {
                        output
                            .push_str(&format!("{} file(s) had errors\n", report.errored_files()));
                    }
                    if report.failed_files() > 0 {
                        output.push_str(&format!("{} file(s) failed\n", report.failed_files()));
                    }
                    if report.timed_out_files() > 0 {
                        output
                            .push_str(&format!("{} file(s) timed out\n", report.timed_out_files()));
                    }
                    output.push_str(&format!("Total time: {}ms\n", report.total_elapsed()));
                }

                Some(output)
            }
        }
    }
}

pub struct ConfigurableReporter {
    inner: Box<dyn Reporter + Send + Sync>,
}

impl ConfigurableReporter {
    pub fn new(report_config: &config::Config) -> Self {
        let inner: Box<dyn Reporter + Send + Sync> = match report_config.format {
            Format::Human => Box::new(HumanReporter::new(report_config.clone())),
            Format::Json => Box::new(JsonReporter::new(report_config.clone())),
        };
        Self { inner }
    }
}

impl Reporter for ConfigurableReporter {
    fn on(&self, report_event: &ReportEvent) -> Option<String> {
        self.inner.on(report_event)
    }
}

#[cfg(test)]
mod test_suite_report_tests {
    use assert2::check;

    use super::test_helpers::*;

    #[test]
    fn it_counts_files() {
        let report = test_suite_report(
            vec![
                completed_test_file("file1.nix", 0, vec![]),
                completed_test_file("file2.nix", 0, vec![]),
                errored_test_file("file3.nix", "error", 0),
                errored_test_file("file4.nix", "error", 0),
                errored_test_file("file5.nix", "error", 0),
                completed_test_file(
                    "file6.nix",
                    0,
                    vec![failed_test_report(
                        vec!["test"],
                        "file:1",
                        vec![failed_check_report_with_message("check", "failed")],
                    )],
                ),
            ],
            0,
        );

        check!(report.processed_files() == 6);
        check!(report.succeeded_files() == 2);
        check!(report.failed_files() == 1);
        check!(report.errored_files() == 3);
    }

    #[test]
    fn it_handles_empty_report() {
        let report = test_suite_report(vec![], 0);

        check!(report.processed_files() == 0);
        check!(report.succeeded_files() == 0);
        check!(report.failed_files() == 0);
        check!(report.errored_files() == 0);
        check!(report.has_issues() == false);
    }

    #[test]
    fn it_has_no_issues_when_all_tests_pass() {
        let report = test_suite_report(
            vec![
                completed_test_file("file1.nix", 0, vec![]),
                completed_test_file("file2.nix", 0, vec![]),
            ],
            0,
        );

        check!(report.has_issues() == false);
    }

    #[test]
    fn it_has_issues_when_at_least_one_file_failed() {
        let report = test_suite_report(
            vec![
                completed_test_file("file1.nix", 0, vec![]),
                completed_test_file(
                    "file2.nix",
                    0,
                    vec![failed_test_report(
                        vec!["test"],
                        "file:1",
                        vec![failed_check_report_with_message("check", "failed")],
                    )],
                ),
            ],
            0,
        );

        check!(report.has_issues());
    }

    #[test]
    fn it_has_issues_when_at_least_one_file_errored() {
        let report = test_suite_report(
            vec![
                completed_test_file("file1.nix", 0, vec![]),
                errored_test_file("file2.nix", "error", 0),
            ],
            0,
        );

        check!(report.has_issues());
    }
}

#[cfg(test)]
mod human_reporter_tests {
    use assert2::check;

    use crate::reports::config::Config;

    use super::test_helpers::*;
    use super::*;

    #[test]
    fn it_reports_errored_test_file() {
        let reporter = HumanReporter::new(Config::default());
        let event = ReportEvent::TestFileCompleted(errored_test_file(
            "broken.nix",
            "Syntax error at line 5\n",
            50,
        ));

        check!(
            reporter.on(&event).unwrap()
                == "\
File: broken.nix (50ms)
ERROR: Syntax error at line 5

"
        );
    }

    #[test]
    fn it_reports_test_file_invalid() {
        let reporter = HumanReporter::new(Config::default());
        let event = ReportEvent::TestFileInvalid("invalid.nix".to_string());

        check!(
            reporter.on(&event).unwrap()
                == "Warning: 'invalid.nix' is not a test file, skipping.\n"
        );
    }

    #[test]
    fn it_reports_completed_test_with_failures() {
        let reporter = HumanReporter::new(Config::default());
        let event = ReportEvent::TestFileCompleted(completed_test_file(
            "test.nix",
            150,
            vec![failed_test_report(
                vec!["suite", "test1"],
                "test.nix:20",
                vec![failed_check_report_with_message(
                    "should fail",
                    "Expected true but got false",
                )],
            )],
        ));

        check!(
            reporter.on(&event).unwrap()
                == "\
File: test.nix (150ms)
✗ suite -> test1 -> should fail
    Failure:
      Expected true but got false
      at my_test.nix:30
FAILED (1 failed)

"
        );
    }

    #[test]
    fn it_reports_completed_test_with_failure_without_message() {
        let reporter = HumanReporter::new(Config::default());
        let event = ReportEvent::TestFileCompleted(completed_test_file(
            "test.nix",
            75,
            vec![failed_test_report(
                vec!["test2"],
                "test.nix:30",
                vec![failed_check_report("check")],
            )],
        ));

        check!(
            reporter.on(&event).unwrap()
                == "\
File: test.nix (75ms)
✗ test2 -> check
    Failed at my_test.nix:30
FAILED (1 failed)

"
        );
    }

    #[test]
    fn it_reports_test_suite_completed_with_no_files() {
        let reporter = HumanReporter::new(Config::default());
        let event = ReportEvent::TestSuiteCompleted(test_suite_report(vec![], 0));

        check!(
            reporter.on(&event).unwrap()
                == "\
No test files found
"
        );
    }

    #[test]
    fn it_reports_test_suite_completed_all_passing() {
        let reporter = HumanReporter::new(Config::default());
        let event = ReportEvent::TestSuiteCompleted(test_suite_report(
            vec![completed_test_file("test1.nix", 50, vec![])],
            100,
        ));

        check!(
            reporter.on(&event).unwrap()
                == "\
All tests passed (100ms)
"
        );
    }

    #[test]
    fn it_reports_test_suite_completed_with_failures() {
        let reporter = HumanReporter::new(Config::default());
        let event = ReportEvent::TestSuiteCompleted(test_suite_report(
            vec![
                completed_test_file("test1.nix", 50, vec![]),
                completed_test_file(
                    "test2.nix",
                    75,
                    vec![failed_test_report(
                        vec!["test"],
                        "test2.nix:1",
                        vec![failed_check_report("check")],
                    )],
                ),
            ],
            200,
        ));

        check!(
            reporter.on(&event).unwrap()
                == "\
1 file(s) succeeded
1 file(s) failed
Total time: 200ms
"
        );
    }

    #[test]
    fn it_reports_test_suite_completed_with_errors() {
        let reporter = HumanReporter::new(Config::default());
        let event = ReportEvent::TestSuiteCompleted(test_suite_report(
            vec![
                completed_test_file("test1.nix", 50, vec![]),
                errored_test_file("broken.nix", "error", 25),
            ],
            150,
        ));

        check!(
            reporter.on(&event).unwrap()
                == "\
1 file(s) succeeded
1 file(s) had errors
Total time: 150ms
"
        );
    }

    #[test]
    fn it_reports_test_suite_completed_with_mixed_results() {
        let reporter = HumanReporter::new(Config::default());
        let event = ReportEvent::TestSuiteCompleted(test_suite_report(
            vec![
                completed_test_file("passing.nix", 30, vec![]),
                completed_test_file(
                    "failing.nix",
                    40,
                    vec![failed_test_report(
                        vec!["test"],
                        "failing.nix:1",
                        vec![failed_check_report("check")],
                    )],
                ),
                errored_test_file("broken.nix", "syntax error", 20),
            ],
            250,
        ));

        check!(
            reporter.on(&event).unwrap()
                == "\
1 file(s) succeeded
1 file(s) had errors
1 file(s) failed
Total time: 250ms
"
        );
    }

    #[test]
    fn it_hides_succeeded_test_files_when_flag_is_set() {
        let config = config::Config {
            format: Format::Human,
            hide_succeeded: true,
            hide_failed: false,
            hide_errored: false,
        };
        let reporter = HumanReporter::new(config);
        let event = ReportEvent::TestFileCompleted(completed_test_file("test.nix", 50, vec![]));

        check!(reporter.on(&event).is_none());
    }

    #[test]
    fn it_hides_failed_test_files_when_flag_is_set() {
        let config = config::Config {
            format: Format::Human,
            hide_succeeded: false,
            hide_failed: true,
            hide_errored: false,
        };
        let reporter = HumanReporter::new(config);
        let event = ReportEvent::TestFileCompleted(completed_test_file(
            "test.nix",
            75,
            vec![failed_test_report(
                vec!["test"],
                "test.nix:1",
                vec![failed_check_report("check")],
            )],
        ));

        check!(reporter.on(&event).is_none());
    }

    #[test]
    fn it_hides_errored_test_files_when_flag_is_set() {
        let config = config::Config {
            format: Format::Human,
            hide_succeeded: false,
            hide_failed: false,
            hide_errored: true,
        };
        let reporter = HumanReporter::new(config);
        let event = ReportEvent::TestFileCompleted(errored_test_file("broken.nix", "error", 25));

        check!(reporter.on(&event).is_none());
    }

    #[test]
    fn it_always_shows_test_suite_summary_regardless_of_hide_flags() {
        let config = config::Config {
            format: Format::Human,
            hide_succeeded: true,
            hide_failed: true,
            hide_errored: true,
        };
        let reporter = HumanReporter::new(config);
        let event = ReportEvent::TestSuiteCompleted(test_suite_report(
            vec![
                completed_test_file("passing.nix", 30, vec![]),
                completed_test_file(
                    "failing.nix",
                    40,
                    vec![failed_test_report(
                        vec!["test"],
                        "failing.nix:1",
                        vec![failed_check_report("check")],
                    )],
                ),
                errored_test_file("broken.nix", "error", 20),
            ],
            250,
        ));

        let output = reporter.on(&event).unwrap();
        check!(output.contains("1 file(s) succeeded"));
        check!(output.contains("1 file(s) had errors"));
        check!(output.contains("1 file(s) failed"));
        check!(output.contains("Total time: 250ms"));
    }
}

#[cfg(test)]
mod json_reporter_tests {
    use assert2::check;

    use crate::reports::config::Config;

    use super::test_helpers::*;
    use super::*;

    #[test]
    fn it_returns_json_for_completed_test_file() {
        let reporter = JsonReporter::new(Config::default());
        let report = completed_test_file("test.nix", 50, vec![]);
        let event = ReportEvent::TestFileCompleted(report.clone());

        let output = reporter.on(&event).unwrap();
        check!(serde_json::from_str::<serde_json::Value>(&output).is_ok());
        check!(output.contains("\"file\":\"test.nix\""));
    }

    #[test]
    fn it_returns_none_for_non_test_file_completed_events() {
        let reporter = JsonReporter::new(Config::default());
        let event = ReportEvent::TestFileNotFound("test.nix".to_string());

        check!(reporter.on(&event).is_none());
    }

    #[test]
    fn it_hides_succeeded_test_files_when_flag_is_set() {
        let config = config::Config {
            format: Format::Json,
            hide_succeeded: true,
            hide_failed: false,
            hide_errored: false,
        };
        let reporter = JsonReporter::new(config);
        let event = ReportEvent::TestFileCompleted(completed_test_file("test.nix", 50, vec![]));

        check!(reporter.on(&event).is_none());
    }

    #[test]
    fn it_hides_failed_test_files_when_flag_is_set() {
        let config = config::Config {
            format: Format::Json,
            hide_succeeded: false,
            hide_failed: true,
            hide_errored: false,
        };
        let reporter = JsonReporter::new(config);
        let event = ReportEvent::TestFileCompleted(completed_test_file(
            "test.nix",
            75,
            vec![failed_test_report(
                vec!["test"],
                "test.nix:1",
                vec![failed_check_report("check")],
            )],
        ));

        check!(reporter.on(&event).is_none());
    }

    #[test]
    fn it_hides_errored_test_files_when_flag_is_set() {
        let config = config::Config {
            format: Format::Json,
            hide_succeeded: false,
            hide_failed: false,
            hide_errored: true,
        };
        let reporter = JsonReporter::new(config);
        let event = ReportEvent::TestFileCompleted(errored_test_file("broken.nix", "error", 25));

        check!(reporter.on(&event).is_none());
    }

    #[test]
    fn it_shows_succeeded_test_files_when_hide_flag_is_false() {
        let config = config::Config {
            format: Format::Json,
            hide_succeeded: false,
            hide_failed: true,
            hide_errored: true,
        };
        let reporter = JsonReporter::new(config);
        let event = ReportEvent::TestFileCompleted(completed_test_file("test.nix", 50, vec![]));

        check!(reporter.on(&event).is_some());
    }
}

#[cfg(test)]
mod test_helpers {
    use super::*;

    #[allow(dead_code)]
    pub fn passing_check_report(name: &str) -> CheckReport {
        CheckReport {
            name: name.to_string(),
            success: true,
            failure: None,
            location: "my_test.nix:30".to_string(),
        }
    }

    pub fn failed_check_report_with_message(name: &str, failure: &str) -> CheckReport {
        CheckReport {
            name: name.to_string(),
            success: false,
            failure: Some(failure.to_string()),
            location: "my_test.nix:30".to_string(),
        }
    }

    pub fn failed_check_report(name: &str) -> CheckReport {
        CheckReport {
            name: name.to_string(),
            success: false,
            failure: None,
            location: "my_test.nix:30".to_string(),
        }
    }

    #[allow(dead_code)]
    pub fn passing_test_report(
        path: Vec<&str>,
        location: &str,
        checks: Vec<CheckReport>,
    ) -> TestReport {
        TestReport {
            success: checks.iter().all(|c| c.success),
            path: path.iter().map(|s| s.to_string()).collect(),
            location: location.to_string(),
            checks,
        }
    }

    pub fn failed_test_report(
        path: Vec<&str>,
        location: &str,
        checks: Vec<CheckReport>,
    ) -> TestReport {
        TestReport {
            success: false,
            path: path.iter().map(|s| s.to_string()).collect(),
            location: location.to_string(),
            checks,
        }
    }

    pub fn completed_test_file(
        file: &str,
        elapsed: u128,
        tests: Vec<TestReport>,
    ) -> TestFileReport {
        TestFileReport::Completed(TestFileCompletedReport {
            file: file.to_string(),
            elapsed,
            tests,
        })
    }

    pub fn errored_test_file(file: &str, error: &str, elapsed: u128) -> TestFileReport {
        TestFileReport::Errored(TestFileErroredReport {
            file: file.to_string(),
            error: error.to_string(),
            elapsed,
        })
    }

    pub fn test_suite_report(files: Vec<TestFileReport>, elapsed: u128) -> TestSuiteReport {
        TestSuiteReport::new(files, elapsed)
    }
}
