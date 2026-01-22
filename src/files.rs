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

pub trait SearchTestFiles {
    fn find_files_in_dir(
        &self,
        path: &Path,
    ) -> Result<Box<dyn Iterator<Item = String>>, anyhow::Error>;

    fn search_test_files(&self, files: Vec<String>) -> Result<Vec<TestFile>, anyhow::Error> {
        let mut test_files = Vec::new();

        for file in &files {
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

            for found_file in self.find_files_in_dir(path)? {
                test_files.push(TestFile::Valid(found_file));
            }
        }

        test_files.sort();
        test_files.dedup();

        Ok(test_files)
    }
}

pub struct RgSearchTestFiles;

impl SearchTestFiles for RgSearchTestFiles {
    fn find_files_in_dir(
        &self,
        path: &Path,
    ) -> Result<Box<dyn Iterator<Item = String>>, anyhow::Error> {
        let output = Command::new("rg")
            .args(["--files", "--glob", "*_test.nix", path.to_str().unwrap()])
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(Box::new(
            stdout
                .lines()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
                .into_iter(),
        ))
    }
}

pub struct FindSearchTestFiles;

impl SearchTestFiles for FindSearchTestFiles {
    fn find_files_in_dir(
        &self,
        path: &Path,
    ) -> Result<Box<dyn Iterator<Item = String>>, anyhow::Error> {
        let output = Command::new("find")
            .args([path.to_str().unwrap(), "-name", "*_test.nix", "-type", "f"])
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(Box::new(
            stdout
                .lines()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
                .into_iter(),
        ))
    }
}

#[cfg(test)]
mod files_tests {
    use assert2::check;
    use rstest::rstest;
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

    #[rstest]
    #[case(RgSearchTestFiles)]
    #[case(FindSearchTestFiles)]
    fn it_finds_valid_test_files_by_path(#[case] search: impl SearchTestFiles) {
        let (_dir, path) =
            create_temp_dir_with_files(&["file1_test.nix", "file2_test.nix", "file3_test.nix"]);

        let test_files = search.search_test_files(vec![path.clone()]).unwrap();

        check!(
            test_files
                == vec![
                    TestFile::Valid(format!("{path}/file1_test.nix")),
                    TestFile::Valid(format!("{path}/file2_test.nix")),
                    TestFile::Valid(format!("{path}/file3_test.nix")),
                ]
        );
    }

    #[rstest]
    #[case(RgSearchTestFiles)]
    #[case(FindSearchTestFiles)]
    fn it_finds_valid_test_files_by_file(#[case] search: impl SearchTestFiles) {
        let (_dir, path) = create_temp_dir_with_files(&["file_test.nix"]);
        let file_path = format!("{path}/file_test.nix");

        let test_files = search.search_test_files(vec![file_path.clone()]).unwrap();

        check!(test_files == vec![TestFile::Valid(file_path)]);
    }

    #[rstest]
    #[case(RgSearchTestFiles)]
    #[case(FindSearchTestFiles)]
    fn it_finds_empty_when_no_test_files(#[case] search: impl SearchTestFiles) {
        let (_dir, path) = create_temp_dir_with_files(&["regular.nix"]);

        let test_files = search.search_test_files(vec![path]).unwrap();

        check!(test_files == vec![]);
    }

    #[rstest]
    #[case(RgSearchTestFiles)]
    #[case(FindSearchTestFiles)]
    fn it_removes_duplicate_test_files(#[case] search: impl SearchTestFiles) {
        let (_dir, path) = create_temp_dir_with_files(&["file_test.nix"]);
        let file_path = format!("{path}/file_test.nix");

        let test_files = search
            .search_test_files(vec![file_path.clone(), file_path.clone()])
            .unwrap();

        check!(test_files == vec![TestFile::Valid(file_path)]);
    }

    #[rstest]
    #[case(RgSearchTestFiles)]
    #[case(FindSearchTestFiles)]
    fn it_handles_nonexistent_paths(#[case] search: impl SearchTestFiles) {
        let test_files = search
            .search_test_files(vec!["/tmp/not_existing".to_string()])
            .unwrap();

        check!(test_files == vec![TestFile::NotFound("/tmp/not_existing".to_string())]);
    }

    #[rstest]
    #[case(RgSearchTestFiles)]
    #[case(FindSearchTestFiles)]
    fn it_handles_invalid_test_files(#[case] search: impl SearchTestFiles) {
        let (_dir, path) = create_temp_dir_with_files(&["flake.nix"]);
        let file_path = format!("{path}/flake.nix");
        let paths = vec![file_path.clone()];

        let test_files = search.search_test_files(paths).unwrap();

        check!(test_files == vec![TestFile::Invalid(file_path)]);
    }

    #[rstest]
    #[case(RgSearchTestFiles)]
    #[case(FindSearchTestFiles)]
    fn it_handles_mixed_paths(#[case] search: impl SearchTestFiles) {
        let (_dir, path) = create_temp_dir_with_files(&["file1_test.nix", "file2_test.nix"]);

        let nonexistent = "/tmp/not_existing".to_string();
        let paths = vec![path.clone(), nonexistent.clone()];

        let test_files = search.search_test_files(paths).unwrap();

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
