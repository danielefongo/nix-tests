use std::cmp::Ordering;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Eq, Clone)]
pub enum TestFile {
    Valid(String),
    NotFound(String),
    Invalid(String),
}

impl TestFile {
    pub fn rank(&self) -> u8 {
        match self {
            TestFile::Valid(_) => 2,
            TestFile::NotFound(_) => 1,
            TestFile::Invalid(_) => 0,
        }
    }
    pub fn name(&self) -> &str {
        match self {
            TestFile::Valid(f) | TestFile::NotFound(f) | TestFile::Invalid(f) => f,
        }
    }
}

impl Ord for TestFile {
    fn cmp(&self, other: &Self) -> Ordering {
        self.rank()
            .cmp(&other.rank())
            .then_with(|| self.name().cmp(other.name()))
    }
}

impl PartialOrd for TestFile {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for TestFile {
    fn eq(&self, other: &Self) -> bool {
        self.rank() == other.rank() && self.name() == other.name()
    }
}

pub trait FindTestFiles {
    fn find_test_files(&self) -> Result<Vec<TestFile>, anyhow::Error>;
}

impl FindTestFiles for Vec<String> {
    fn find_test_files(&self) -> Result<Vec<TestFile>, anyhow::Error> {
        let mut test_files = Vec::new();

        for file in self {
            let path = Path::new(file);

            if !path.exists() {
                test_files.push(TestFile::NotFound(file.clone()));
                continue;
            }

            if path.is_file() {
                if path.to_string_lossy().ends_with("_test.nix") {
                    test_files.push(TestFile::Valid(file.clone()));
                } else {
                    test_files.push(TestFile::Invalid(file.clone()));
                }
                continue;
            }

            let output = Command::new("rg")
                .args(["--files", "--glob", "*_test.nix", path.to_str().unwrap()])
                .output()?;

            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                test_files.push(TestFile::Valid(line.to_string()));
            }
        }

        test_files.sort();
        test_files.dedup();

        Ok(test_files)
    }
}

#[cfg(test)]
mod files_tests {
    use assert2::check;
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    use super::*;

    fn create_temp_dir_with_files(files: &[&str]) -> (TempDir, String) {
        let dir = TempDir::new().unwrap();
        for file in files {
            let file_path = dir.path().join(file);
            let mut f = fs::File::create(&file_path).unwrap();
            f.write_all(b"test content").unwrap();
        }
        let path = dir.path().to_str().unwrap().to_string();
        (dir, path)
    }

    #[test]
    fn it_finds_valid_test_files_by_path() {
        let (_dir, path) =
            create_temp_dir_with_files(&["file1_test.nix", "file2_test.nix", "file3_test.nix"]);

        let paths = vec![path.clone()];
        let test_files = paths.find_test_files().unwrap();

        check!(
            test_files
                == vec![
                    TestFile::Valid(format!("{path}/file1_test.nix")),
                    TestFile::Valid(format!("{path}/file2_test.nix")),
                    TestFile::Valid(format!("{path}/file3_test.nix")),
                ]
        );
    }

    #[test]
    fn it_finds_valid_test_files_by_file() {
        let (_dir, path) = create_temp_dir_with_files(&["file_test.nix"]);
        let file_path = format!("{path}/file_test.nix");

        let paths = vec![file_path.clone()];
        let test_files = paths.find_test_files().unwrap();

        check!(test_files == vec![TestFile::Valid(file_path)]);
    }

    #[test]
    fn it_finds_empty_when_no_test_files() {
        let (_dir, path) = create_temp_dir_with_files(&["regular.nix"]);

        let paths = vec![path];
        let test_files = paths.find_test_files().unwrap();

        check!(test_files == vec![]);
    }

    #[test]
    fn it_removes_duplicate_test_files() {
        let (_dir, path) = create_temp_dir_with_files(&["file_test.nix"]);
        let file_path = format!("{path}/file_test.nix");

        let paths = vec![file_path.clone(), file_path.clone()];
        let test_files = paths.find_test_files().unwrap();

        check!(test_files == vec![TestFile::Valid(file_path)]);
    }

    #[test]
    fn it_handles_nonexistent_paths() {
        let paths = vec!["/tmp/not_existing".to_string()];
        let test_files = paths.find_test_files().unwrap();

        check!(test_files == vec![TestFile::NotFound("/tmp/not_existing".to_string())]);
    }

    #[test]
    fn it_handles_invalid_test_files() {
        let (_dir, path) = create_temp_dir_with_files(&["flake.nix"]);
        let file_path = format!("{path}/flake.nix");

        let paths = vec![file_path.clone()];
        let test_files = paths.find_test_files().unwrap();

        check!(test_files == vec![TestFile::Invalid(file_path)]);
    }

    #[test]
    fn it_handles_mixed_paths() {
        let (_dir, path) = create_temp_dir_with_files(&["file1_test.nix", "file2_test.nix"]);

        let nonexistent = "/tmp/not_existing".to_string();
        let paths = vec![path.clone(), nonexistent.clone()];
        let test_files = paths.find_test_files().unwrap();

        check!(
            test_files
                == vec![
                    TestFile::NotFound(nonexistent),
                    TestFile::Valid(format!("{path}/file1_test.nix")),
                    TestFile::Valid(format!("{path}/file2_test.nix")),
                ]
        );
    }
}
