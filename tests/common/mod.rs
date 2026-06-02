use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use tempfile::TempDir;

// Serialise all tests that touch env vars so they don't interfere with each other.
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// A temporary test environment pointing at a copy of the test fixtures.
///
/// Holds the Mutex guard for the duration of the test: env vars are safe to
/// read and write while the guard is held because no other test can run
/// concurrently.  The guard is released (and env vars cleared) when this value
/// is dropped.
pub struct TestEnv {
    pub dir: TempDir,
    _guard: MutexGuard<'static, ()>,
}

impl TestEnv {
    /// Copy the fixture tree into a fresh temp directory and point the three
    /// override env vars at it.
    pub fn setup() -> Self {
        let guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempfile::tempdir().expect("tempdir");

        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        copy_dir(&fixtures, dir.path()).expect("copy fixtures");

        // SAFETY: we hold ENV_LOCK, so no other thread is reading/writing these
        // vars concurrently.
        unsafe {
            std::env::set_var("JERU_PROJECTS_DIR", dir.path().join("projects"));
            std::env::set_var("JERU_KNOWLEDGE_DIR", dir.path().join("knowledge"));
            std::env::set_var("JERU_CACHE_DIR", dir.path().join("cache"));
        }

        TestEnv { dir, _guard: guard }
    }

    pub fn projects_dir(&self) -> PathBuf {
        self.dir.path().join("projects")
    }

    pub fn project_dir(&self, name: &str) -> PathBuf {
        self.projects_dir().join(name)
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        // SAFETY: we still hold ENV_LOCK.
        unsafe {
            std::env::remove_var("JERU_PROJECTS_DIR");
            std::env::remove_var("JERU_KNOWLEDGE_DIR");
            std::env::remove_var("JERU_CACHE_DIR");
        }
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
