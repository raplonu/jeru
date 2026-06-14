use std::path::{Path, PathBuf};
use std::sync::Mutex;

use jeru::Config;
use tempfile::TempDir;

/// Serialise tests that call `std::env::set_current_dir`, which is process-wide.
pub static CWD_LOCK: Mutex<()> = Mutex::new(());

/// A temporary test environment pointing at a copy of the test fixtures.
pub struct TestEnv {
    pub dir: TempDir,
}

impl TestEnv {
    /// Copy the fixture tree into a fresh temp directory and return a Config
    /// pointed at it. Tests receive Config directly; no env vars or mutexes needed.
    pub fn setup() -> (Self, Config) {
        let dir = tempfile::tempdir().expect("tempdir");

        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        copy_dir(&fixtures, dir.path()).expect("copy fixtures");

        let config = Config {
            projects_dir: dir.path().join("projects"),
            knowledge_dir: dir.path().join("knowledge"),
            cache_dir: dir.path().join("cache"),
            obsidian_mcp_enabled: true,
            obsidian_mcp_url: "http://127.0.0.1:27123/mcp/".to_string(),
            obsidian_api_key_env: "OBSIDIAN_API_KEY".to_string(),
            obsidian_autostart: false,
            obsidian_launch_cmd: "false".to_string(),
        };

        (TestEnv { dir }, config)
    }

    pub fn projects_dir(&self) -> PathBuf {
        self.dir.path().join("projects")
    }

    pub fn project_dir(&self, name: &str) -> PathBuf {
        self.projects_dir().join(name)
    }
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let target = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            std::fs::create_dir_all(&target)?;
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}
