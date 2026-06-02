mod common;

use common::TestEnv;

// ── project listing ──────────────────────────────────────────────────────────

#[test]
fn list_projects_returns_sorted_names() {
    let _env = TestEnv::setup();
    let projects = jeru::list_projects().unwrap();
    let names: Vec<_> = projects.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["alpha", "beta"]);
}

#[test]
fn list_projects_empty_when_no_dir() {
    let _env = TestEnv::setup();
    std::fs::remove_dir_all(_env.projects_dir()).unwrap();
    let projects = jeru::list_projects().unwrap();
    assert!(projects.is_empty());
}

// ── manifest loading ─────────────────────────────────────────────────────────

#[test]
fn load_manifest_minimal() {
    let _env = TestEnv::setup();
    let m = jeru::load_manifest("alpha").unwrap();
    assert_eq!(m.name, "alpha");
    assert_eq!(m.repos, ["~/code/alpha-repo"]);
    assert!(m.primary_repo.is_none());
    assert!(m.knowledge_sets.is_empty());
    assert!(m.resources.is_empty());
}

#[test]
fn load_manifest_full() {
    let _env = TestEnv::setup();
    let m = jeru::load_manifest("beta").unwrap();
    assert_eq!(m.name, "beta");
    assert_eq!(m.primary_repo.as_deref(), Some("~/code/beta-main"));
    assert_eq!(m.knowledge_sets, ["docs", "notes"]);
    assert_eq!(m.repos, ["~/code/beta-main", "~/code/beta-api"]);
    assert_eq!(m.resources, ["~/refs/beta"]);
}

#[test]
fn load_manifest_missing_project_returns_error() {
    let _env = TestEnv::setup();
    assert!(jeru::load_manifest("does-not-exist").is_err());
}

// ── CLAUDE.md init ───────────────────────────────────────────────────────────

#[test]
fn init_claude_md_writes_file() {
    let env = TestEnv::setup();
    let path = jeru::init_claude_md("alpha", false).unwrap();
    assert_eq!(path, env.project_dir("alpha").join("CLAUDE.md"));
    assert!(path.exists());
    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.contains("alpha"));
}

#[test]
fn init_claude_md_refuses_overwrite_without_force() {
    let _env = TestEnv::setup();
    // beta already has a CLAUDE.md in the fixture
    assert!(jeru::init_claude_md("beta", false).is_err());
}

#[test]
fn init_claude_md_force_overwrites() {
    let _env = TestEnv::setup();
    let path = jeru::init_claude_md("beta", true).unwrap();
    assert!(path.exists());
    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.contains("beta"));
}

// ── settings.json ────────────────────────────────────────────────────────────

#[test]
fn write_settings_creates_file() {
    let _env = TestEnv::setup();
    let path = jeru::write_settings("alpha").unwrap();
    assert!(path.exists());
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let dirs = v["permissions"]["additionalDirectories"]
        .as_array()
        .unwrap();
    assert!(!dirs.is_empty());
}

#[test]
fn write_settings_includes_all_linked_dirs() {
    let _env = TestEnv::setup();
    let path = jeru::write_settings("beta").unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let dirs: Vec<String> = v["permissions"]["additionalDirectories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d.as_str().unwrap().to_string())
        .collect();
    // primary_repo + 2 repos (deduped) + 2 knowledge sets + 1 resource
    assert_eq!(dirs.len(), 5);
}

// ── workon / resolve_project ─────────────────────────────────────────────────

#[test]
fn workon_sets_current_project() {
    let _env = TestEnv::setup();
    jeru::workon("alpha").unwrap();
    let current = jeru::current_project().unwrap();
    assert_eq!(current.as_deref(), Some("alpha"));
}

#[test]
fn resolve_project_falls_back_to_current() {
    let _env = TestEnv::setup();
    jeru::workon("beta").unwrap();
    let name = jeru::resolve_project(None).unwrap();
    assert_eq!(name, "beta");
}

#[test]
fn workon_unknown_project_returns_error() {
    let _env = TestEnv::setup();
    assert!(jeru::workon("ghost").is_err());
}
