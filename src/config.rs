use std::{
    env::current_dir,
    path::{Path, PathBuf},
};

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::reports::config as report_config;
use crate::runners::config as runner_config;

#[derive(Default, Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct Config {
    #[serde(default)]
    pub runner: runner_config::Config,

    #[serde(default)]
    pub report: report_config::Config,
}

impl TryFrom<PathBuf> for Config {
    type Error = anyhow::Error;

    fn try_from(path: PathBuf) -> Result<Self, Self::Error> {
        let content = std::fs::read_to_string(path.clone())
            .context(format!("Failed to read config file: {}", path.display()))?;

        Config::try_from(content)
    }
}

impl TryFrom<String> for Config {
    type Error = anyhow::Error;

    fn try_from(content: String) -> Result<Self, Self::Error> {
        toml::from_str(&content).context(format!("Failed to parse config: {content}"))
    }
}

impl Config {
    pub fn search() -> anyhow::Result<Option<Self>> {
        let cwd = current_dir().context("Failed to get current working directory")?;
        Self::search_in(&cwd)
    }

    pub fn search_in(dir: &Path) -> anyhow::Result<Option<Self>> {
        let config_path = dir.join(".nix-tests.toml");

        if config_path.exists() {
            let config = Config::try_from(config_path.to_path_buf())?;
            return Ok(Some(config));
        }

        if dir.join("flake.lock").exists() || dir.join(".git").exists() {
            return Ok(None);
        }

        if let Some(parent) = dir.parent() {
            Self::search_in(parent)
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod config_parsing_tests {
    use assert2::check;

    use crate::runners::config::NumThreads;

    #[test]
    fn it_parses_config_from_string() {
        let toml_str = r#"
            [runner]
            num-threads = 8
        "#;
        let config = super::Config::try_from(toml_str.to_string()).unwrap();
        check!(config.runner.num_threads == NumThreads::new(8));
    }
}

#[cfg(test)]
mod config_search_tests {
    use std::io::Write;
    use std::{fs, path::PathBuf};

    use assert2::check;
    use rstest::{fixture, rstest};
    use tempfile::TempDir;

    use crate::runners::config::NumThreads;

    use super::*;

    #[rstest]
    fn it_finds_config_in_current_dir(path: PathBuf) {
        create_file(&path, ".nix-tests.toml", b"[runner]\nnum-threads = 4");

        let config = Config::search_in(&path).unwrap().unwrap();

        check!(config.runner.num_threads == NumThreads::new(4));
    }

    #[rstest]
    fn it_finds_config_in_parent_dir(path: PathBuf) {
        create_file(&path, "child/.nix-tests.toml", b"[runner]\nnum-threads = 4");

        let config = Config::search_in(&path.join("child")).unwrap().unwrap();

        check!(config.runner.num_threads == NumThreads::new(4));
    }

    #[rstest]
    fn it_stops_at_flake_lock(path: PathBuf) {
        create_file(&path, "child/another_child/a_file", b"");
        create_file(&path, "child/flake.lock", b"");
        create_file(&path, ".nix-tests.toml", b"");

        let config = Config::search_in(&path.join("child/another_child")).unwrap();

        check!(config.is_none());
    }

    #[rstest]
    fn it_stops_at_git_dir(path: PathBuf) {
        create_file(&path, "child/another_child/a_file", b"");
        create_file(&path, "child/.git", b"");
        create_file(&path, ".nix-tests.toml", b"");

        let config = Config::search_in(&path.join("child/another_child")).unwrap();

        check!(config.is_none());
    }

    #[rstest]
    fn it_searches_multiple_levels_up(path: PathBuf) {
        create_file(&path, "level1/level2/level3/a_file", b"");
        create_file(&path, ".nix-tests.toml", b"[runner]\nnum-threads = 4");

        let config = Config::search_in(&path.join("level1/level2/level3"))
            .unwrap()
            .unwrap();

        check!(config.runner.num_threads == NumThreads::new(4));
    }

    #[rstest]
    fn it_prefers_closest_config(path: PathBuf) {
        create_file(&path, ".nix-tests.toml", b"[runner]\nnum-threads = 4");
        create_file(&path, "child/.nix-tests.toml", b"[runner]\nnum-threads = 8");

        let config = Config::search_in(&path.join("child")).unwrap();

        check!(config.is_some());
        check!(config.unwrap().runner.num_threads == NumThreads::new(8));
    }

    #[fixture]
    fn path() -> PathBuf {
        TempDir::new().unwrap().path().to_path_buf()
    }

    fn create_file(base: &Path, relative_path: &str, content: &[u8]) {
        let file_path = base.join(relative_path);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut file = fs::File::create(&file_path).unwrap();
        file.write_all(content).unwrap();
    }
}
