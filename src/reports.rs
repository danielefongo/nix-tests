use serde::{Deserialize, Serialize};

use crate::reports::config::Format;

pub mod config {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
    #[serde(rename_all = "kebab-case")]
    pub struct Config {
        #[serde(default)]
        pub format: Format,
    }

    impl Config {
        fn default_format() -> Format {
            Format::Human
        }
    }

    impl Default for Config {
        fn default() -> Self {
            Self {
                format: Self::default_format(),
            }
        }
    }

    #[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    #[serde(rename_all = "kebab-case")]
    pub enum Format {
        #[default]
        Json,
        Human,
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
            })
            .count()
    }
    fn failed_files(&self) -> usize {
        self.reports
            .iter()
            .filter(|report| match report {
                TestFileReport::Completed(report) => report.failed_count() > 0,
                TestFileReport::Errored(_) => false,
            })
            .count()
    }
    fn errored_files(&self) -> usize {
        self.reports
            .iter()
            .filter(|report| matches!(report, TestFileReport::Errored(_)))
            .count()
    }
    fn total_elapsed(&self) -> u128 {
        self.elapsed
    }
    pub fn has_issues(&self) -> bool {
        self.reports.iter().any(|report| match report {
            TestFileReport::Completed(report) => report.failed_count() > 0,
            TestFileReport::Errored(_) => true,
        })
    }
}

#[derive(Debug, Serialize, PartialEq, Eq, Clone)]
#[serde(untagged)]
pub enum TestFileReport {
    Completed(TestFileCompletedReport),
    Errored(TestFileErroredReport),
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
    pub fn with_file(mut self, file: String) -> Self {
        self.file = file;
        self
    }
    pub fn with_elapsed(mut self, elapsed: u128) -> Self {
        self.elapsed = elapsed;
        self
    }
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
}

#[derive(Debug, Serialize, PartialEq, Eq, Clone)]
pub struct TestFileErroredReport {
    pub file: String,
    pub error: String,
    pub elapsed: u128,
}

#[derive(Debug, PartialEq, Eq, Clone)]
#[allow(clippy::enum_variant_names)]
pub enum ReportEvent {
    TestFileNotFound(String),
    TestFileInvalid(String),
    TestCompleted(TestFileReport),
    TestSuiteCompleted(TestSuiteReport),
}

#[cfg_attr(test, mockall::automock)]
pub trait Reporter {
    fn on(&self, report_event: &ReportEvent);
}

pub struct JsonReporter;

impl Reporter for JsonReporter {
    fn on(&self, report_event: &ReportEvent) {
        if let ReportEvent::TestCompleted(report) = report_event {
            println!("{}", serde_json::to_string(report).unwrap());
        }
    }
}

pub struct HumanReporter;

impl HumanReporter {
    fn print_report(&self, result: &TestFileReport) {
        match result {
            TestFileReport::Completed(report) => {
                println!("File: {} ({}ms)", report.file, report.elapsed);

                for test in &report.tests {
                    let path = test.path.join(" -> ");

                    for check in &test.checks {
                        if check.success {
                            println!("✓ {} -> {}", path, check.name);
                        } else {
                            println!("✗ {} -> {}", path, check.name);
                            if let Some(failure) = &check.failure {
                                println!("    Failure:");
                                for line in failure.lines() {
                                    println!("      {}", line);
                                }
                                println!("      at {}", test.location);
                            } else {
                                println!("    Failed at {}", test.location);
                            }
                        }
                    }
                }

                if report.failed_count() > 0 {
                    println!("FAILED ({} failed)", report.failed_count())
                }
                println!();
            }
            TestFileReport::Errored(report) => {
                println!("File: {} ({}ms)", report.file, report.elapsed);
                println!("ERROR: {}", report.error);
            }
        }
    }
}

impl Reporter for HumanReporter {
    fn on(&self, report_event: &ReportEvent) {
        match report_event {
            ReportEvent::TestFileNotFound(path) => {
                eprintln!("Warning: '{}' is not found, skipping.\n", path);
            }
            ReportEvent::TestFileInvalid(path) => {
                eprintln!("Warning: '{}' is not a test file, skipping.\n", path);
            }
            ReportEvent::TestCompleted(report) => {
                self.print_report(report);
            }
            ReportEvent::TestSuiteCompleted(report) => {
                if report.processed_files() == 0 {
                    println!("No test files found");
                } else if report.failed_files() == 0 && report.errored_files() == 0 {
                    println!("All tests passed ({}ms)", report.total_elapsed());
                } else {
                    println!("{} file(s) succeeded", report.succeeded_files());
                    if report.errored_files() > 0 {
                        println!("{} file(s) had errors", report.errored_files());
                    }
                    if report.failed_files() > 0 {
                        println!("{} file(s) failed", report.failed_files());
                    }
                    println!("Total time: {}ms", report.total_elapsed());
                }
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
            Format::Human => Box::new(HumanReporter),
            Format::Json => Box::new(JsonReporter),
        };
        Self { inner }
    }
}

impl Reporter for ConfigurableReporter {
    fn on(&self, report_event: &ReportEvent) {
        self.inner.on(report_event);
    }
}

#[cfg(test)]
mod test_suite_report_tests {
    use assert2::check;

    use super::*;

    #[test]
    fn it_counts_files() {
        let report = TestSuiteReport::new(
            vec![
                succeded_test_file(),
                succeded_test_file(),
                errored_test_file(),
                errored_test_file(),
                errored_test_file(),
                failed_test_file(),
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
        let report = TestSuiteReport::new(vec![], 0);

        check!(report.processed_files() == 0);
        check!(report.succeeded_files() == 0);
        check!(report.failed_files() == 0);
        check!(report.errored_files() == 0);
        check!(report.has_issues() == false);
    }

    #[test]
    fn it_has_no_issues_when_all_tests_pass() {
        let report = TestSuiteReport::new(vec![succeded_test_file(), succeded_test_file()], 0);

        check!(report.has_issues() == false);
    }

    #[test]
    fn it_has_issues_when_at_least_one_file_failed() {
        let report = TestSuiteReport::new(vec![succeded_test_file(), failed_test_file()], 0);

        check!(report.has_issues());
    }

    #[test]
    fn it_has_issues_when_at_least_one_file_errored() {
        let report = TestSuiteReport::new(vec![succeded_test_file(), errored_test_file()], 0);

        check!(report.has_issues());
    }

    fn succeded_test_file() -> TestFileReport {
        TestFileReport::Completed(TestFileCompletedReport {
            tests: vec![],
            file: "file".to_string(),
            elapsed: 0,
        })
    }

    fn failed_test_file() -> TestFileReport {
        TestFileReport::Completed(TestFileCompletedReport {
            tests: vec![failed_test_report()],
            file: "file".to_string(),
            elapsed: 0,
        })
    }

    fn errored_test_file() -> TestFileReport {
        TestFileReport::Errored(TestFileErroredReport {
            file: "file".to_string(),
            error: "error".to_string(),
            elapsed: 0,
        })
    }

    fn failed_test_report() -> TestReport {
        TestReport {
            success: false,
            path: vec!["test".to_string()],
            location: "file:1".to_string(),
            checks: vec![failed_check_report()],
        }
    }

    fn failed_check_report() -> CheckReport {
        CheckReport {
            name: "check".to_string(),
            success: false,
            failure: Some("failed".to_string()),
        }
    }
}
