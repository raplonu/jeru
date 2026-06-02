mod common;

use common::TestEnv;
use jeru::Kind;
use std::fs;

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
fn use_project_sets_current_project() {
    let _env = TestEnv::setup();
    jeru::use_project("alpha").unwrap();
    let current = jeru::current_project().unwrap();
    assert_eq!(current.as_deref(), Some("alpha"));
}

#[test]
fn resolve_project_falls_back_to_current() {
    let _env = TestEnv::setup();
    jeru::use_project("beta").unwrap();
    let name = jeru::resolve_project(None).unwrap();
    assert_eq!(name, "beta");
}

#[test]
fn use_project_unknown_project_returns_error() {
    let _env = TestEnv::setup();
    assert!(jeru::use_project("ghost").is_err());
}

// ── add_to_project ───────────────────────────────────────────────────────────

#[test]
fn add_repo_appends_to_manifest() {
    let _env = TestEnv::setup();
    jeru::add_to_project("alpha", "~/code/new-repo", Kind::Repo).unwrap();
    let m = jeru::load_manifest("alpha").unwrap();
    assert!(m.repos.contains(&"~/code/new-repo".to_string()));
}

#[test]
fn add_resource_appends_to_manifest() {
    let _env = TestEnv::setup();
    jeru::add_to_project("alpha", "~/docs/spec.md", Kind::Resource).unwrap();
    let m = jeru::load_manifest("alpha").unwrap();
    assert!(m.resources.contains(&"~/docs/spec.md".to_string()));
}

#[test]
fn add_knowledge_extracts_id() {
    let env = TestEnv::setup();
    let knowledge_path = env.dir.path().join("knowledge/ml-notes");
    std::fs::create_dir_all(&knowledge_path).unwrap();
    let path_str = knowledge_path.to_string_lossy().into_owned();

    jeru::add_to_project("alpha", &path_str, Kind::Knowledge).unwrap();
    let m = jeru::load_manifest("alpha").unwrap();
    assert!(m.knowledge_sets.contains(&"ml-notes".to_string()));
}

#[test]
fn add_duplicate_returns_error() {
    let _env = TestEnv::setup();
    jeru::add_to_project("alpha", "~/code/dup", Kind::Repo).unwrap();
    assert!(jeru::add_to_project("alpha", "~/code/dup", Kind::Repo).is_err());
}

#[test]
fn detect_kind_knowledge_path() {
    let env = TestEnv::setup();
    let kpath = env.dir.path().join("knowledge/docs");
    std::fs::create_dir_all(&kpath).unwrap();
    let path_str = kpath.to_string_lossy().into_owned();
    assert_eq!(jeru::detect_kind(&path_str).unwrap(), Kind::Knowledge);
}

#[test]
fn detect_kind_directory_is_repo() {
    let env = TestEnv::setup();
    let repo = env.dir.path().join("some-repo");
    std::fs::create_dir_all(&repo).unwrap();
    assert_eq!(
        jeru::detect_kind(&repo.to_string_lossy()).unwrap(),
        Kind::Repo
    );
}

#[test]
fn detect_kind_file_extension_is_resource() {
    let _env = TestEnv::setup();
    assert_eq!(
        jeru::detect_kind("~/notes/spec.md").unwrap(),
        Kind::Resource
    );
}

// ── roadmap ───────────────────────────────────────────────────────────────────

#[test]
fn roadmap_default_path_is_in_project_dir() {
    let env = TestEnv::setup();
    let path = jeru::roadmap::effective_path("alpha").unwrap();
    assert_eq!(path, env.project_dir("alpha").join("ROADMAP.md"));
}

#[test]
fn roadmap_link_stores_path_in_manifest() {
    let _env = TestEnv::setup();
    jeru::roadmap::link("alpha", "~/notes/alpha-roadmap.md").unwrap();
    let m = jeru::load_manifest("alpha").unwrap();
    assert_eq!(m.roadmap.as_deref(), Some("~/notes/alpha-roadmap.md"));
}

#[test]
fn roadmap_unlink_clears_path() {
    let _env = TestEnv::setup();
    jeru::roadmap::link("alpha", "~/notes/alpha-roadmap.md").unwrap();
    jeru::roadmap::unlink("alpha").unwrap();
    let m = jeru::load_manifest("alpha").unwrap();
    assert!(m.roadmap.is_none());
}

#[test]
fn roadmap_link_changes_effective_path() {
    let env = TestEnv::setup();
    let custom = env.dir.path().join("notes/alpha-roadmap.md");
    let custom_str = custom.to_string_lossy().into_owned();
    jeru::roadmap::link("alpha", &custom_str).unwrap();
    assert_eq!(jeru::roadmap::effective_path("alpha").unwrap(), custom);
}

#[test]
fn init_claude_md_includes_roadmap_when_file_exists() {
    let env = TestEnv::setup();
    // Create a ROADMAP.md in the project dir
    let roadmap = env.project_dir("alpha").join("ROADMAP.md");
    fs::write(&roadmap, "## Goals\n- [ ] do something\n").unwrap();

    let path = jeru::init_claude_md("alpha", false).unwrap();
    let content = fs::read_to_string(path).unwrap();
    assert!(content.contains("ROADMAP.md"));
    assert!(content.contains("Roadmap"));
}

#[test]
fn init_claude_md_no_roadmap_section_when_file_missing() {
    let _env = TestEnv::setup();
    let path = jeru::init_claude_md("alpha", false).unwrap();
    let content = fs::read_to_string(path).unwrap();
    assert!(!content.contains("Roadmap"));
}
