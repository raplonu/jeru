mod common;

use common::{CWD_LOCK, TestEnv};
use jeru::Kind;
use std::fs;

// ── project listing ──────────────────────────────────────────────────────────

#[test]
fn list_projects_returns_sorted_names() {
    let (_env, config) = TestEnv::setup();
    let projects = jeru::list_projects(&config).unwrap();
    let names: Vec<_> = projects.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["alpha", "beta"]);
}

#[test]
fn list_projects_empty_when_no_dir() {
    let (env, config) = TestEnv::setup();
    std::fs::remove_dir_all(env.projects_dir()).unwrap();
    let projects = jeru::list_projects(&config).unwrap();
    assert!(projects.is_empty());
}

// ── manifest loading ─────────────────────────────────────────────────────────

#[test]
fn load_manifest_minimal() {
    let (_env, config) = TestEnv::setup();
    let m = jeru::load_manifest(&config, "alpha").unwrap();
    assert_eq!(m.name, "alpha");
    assert_eq!(m.repos, ["~/code/alpha-repo"]);
    assert!(m.primary_repo.is_none());
    assert!(m.knowledge_sets.is_empty());
    assert!(m.resources.is_empty());
}

#[test]
fn load_manifest_full() {
    let (_env, config) = TestEnv::setup();
    let m = jeru::load_manifest(&config, "beta").unwrap();
    assert_eq!(m.name, "beta");
    assert_eq!(m.primary_repo.as_deref(), Some("~/code/beta-main"));
    assert_eq!(m.knowledge_sets, ["docs", "notes"]);
    assert_eq!(m.repos, ["~/code/beta-main", "~/code/beta-api"]);
    assert_eq!(m.resources, ["~/refs/beta"]);
}

#[test]
fn load_manifest_missing_project_returns_error() {
    let (_env, config) = TestEnv::setup();
    assert!(jeru::load_manifest(&config, "does-not-exist").is_err());
}

// ── CLAUDE.md init ───────────────────────────────────────────────────────────

#[test]
fn init_claude_md_writes_file() {
    let (env, config) = TestEnv::setup();
    let path = jeru::init_claude_md(&config, "alpha", false).unwrap();
    assert_eq!(path, env.project_dir("alpha").join("CLAUDE.md"));
    assert!(path.exists());
    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.contains("alpha"));
}

#[test]
fn init_claude_md_refuses_overwrite_without_force() {
    let (_env, config) = TestEnv::setup();
    // beta already has a CLAUDE.md in the fixture
    assert!(jeru::init_claude_md(&config, "beta", false).is_err());
}

#[test]
fn init_claude_md_force_overwrites() {
    let (_env, config) = TestEnv::setup();
    let path = jeru::init_claude_md(&config, "beta", true).unwrap();
    assert!(path.exists());
    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.contains("beta"));
}

// ── settings.json ────────────────────────────────────────────────────────────

#[test]
fn write_settings_creates_file() {
    let (_env, config) = TestEnv::setup();
    let path = jeru::write_settings(&config, "alpha").unwrap();
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
    let (_env, config) = TestEnv::setup();
    let path = jeru::write_settings(&config, "beta").unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let dirs: Vec<String> = v["permissions"]["additionalDirectories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d.as_str().unwrap().to_string())
        .collect();
    // primary_repo + 2 repos (deduped) + 2 knowledge sets + 1 resource + 1 project knowledge folder
    assert_eq!(dirs.len(), 6);
}

#[test]
fn additional_directories_deduplicates_primary_repo() {
    let (_env, config) = TestEnv::setup();
    // beta: primary_repo = ~/code/beta-main, repos = [~/code/beta-main, ~/code/beta-api]
    // primary_repo appears in repos, so it should be deduplicated to one entry.
    let m = jeru::load_manifest(&config, "beta").unwrap();
    let dirs = jeru::additional_directories(&config, &m).unwrap();
    let primary = jeru::expand_tilde("~/code/beta-main")
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert_eq!(dirs.iter().filter(|d| **d == primary).count(), 1);
}

// ── .mcp.json ────────────────────────────────────────────────────────────────

#[test]
fn write_mcp_json_creates_obsidian_server() {
    let (env, config) = TestEnv::setup();
    let path = jeru::write_mcp_json(&config, "alpha").unwrap().unwrap();
    assert_eq!(path, env.project_dir("alpha").join(".mcp.json"));
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(v["mcpServers"]["obsidian"]["type"], "http");
    assert_eq!(
        v["mcpServers"]["obsidian"]["headers"]["Authorization"],
        "Bearer ${OBSIDIAN_API_KEY}"
    );
}

#[test]
fn claude_md_references_obsidian_mcp_server() {
    let (env, config) = TestEnv::setup();
    jeru::init_claude_md(&config, "alpha", true).unwrap();
    let content =
        std::fs::read_to_string(env.project_dir("alpha").join("CLAUDE.md")).unwrap();
    assert!(content.contains("obsidian` MCP server"));
}

// ── workon / resolve_project ─────────────────────────────────────────────────

#[test]
fn use_project_sets_current_project() {
    let (_env, config) = TestEnv::setup();
    jeru::use_project(&config, "alpha").unwrap();
    let current = jeru::current_project(&config).unwrap();
    assert_eq!(current.as_deref(), Some("alpha"));
}

#[test]
fn resolve_project_falls_back_to_current() {
    let (_env, config) = TestEnv::setup();
    jeru::use_project(&config, "beta").unwrap();
    let name = jeru::resolve_project(&config, None).unwrap();
    assert_eq!(name, "beta");
}

#[test]
fn use_project_unknown_project_returns_error() {
    let (_env, config) = TestEnv::setup();
    assert!(jeru::use_project(&config, "ghost").is_err());
}

// ── create_project ───────────────────────────────────────────────────────────

#[test]
fn create_project_makes_dir_and_manifest() {
    let (env, config) = TestEnv::setup();
    let dir = jeru::create_project(&config, "gamma", "gamma", false).unwrap();
    assert!(dir.is_dir());
    assert_eq!(dir, env.project_dir("gamma"));
    let m = jeru::load_manifest(&config, "gamma").unwrap();
    assert_eq!(m.name, "gamma");
    assert_eq!(m.knowledge_location, "gamma");
}

#[test]
fn create_project_fails_if_manifest_already_exists() {
    let (_env, config) = TestEnv::setup();
    assert!(jeru::create_project(&config, "alpha", "alpha", false).is_err());
}

#[test]
fn create_project_non_empty_requires_force() {
    let (env, config) = TestEnv::setup();
    // Create a non-empty dir without a manifest
    let dir = env.projects_dir().join("delta");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("somefile.txt"), "data").unwrap();

    assert!(jeru::create_project(&config, "delta", "delta", false).is_err());
    assert!(jeru::create_project(&config, "delta", "delta", true).is_ok());
}

// ── write_workspace ──────────────────────────────────────────────────────────

#[test]
fn write_workspace_creates_file_with_folders() {
    let (env, config) = TestEnv::setup();
    let path = jeru::write_workspace(&config, "alpha").unwrap();
    assert!(path.exists());
    assert_eq!(path, env.project_dir("alpha").join("alpha.code-workspace"));
    let v: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    let folders = v["folders"].as_array().unwrap();
    assert!(!folders.is_empty());
}

#[test]
fn write_workspace_errors_when_no_repos() {
    let (env, config) = TestEnv::setup();
    // Create a project with no repos
    let dir = env.projects_dir().join("empty");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("project.yml"),
        "name: empty\n",
    )
    .unwrap();
    let err = jeru::write_workspace(&config, "empty").unwrap_err();
    assert!(matches!(err, jeru::Error::NoRepos(_)));
}

// ── add_to_project ───────────────────────────────────────────────────────────

#[test]
fn add_repo_appends_to_manifest() {
    let (_env, config) = TestEnv::setup();
    jeru::add_to_project(&config, "alpha", "~/code/new-repo", Kind::Repo).unwrap();
    let m = jeru::load_manifest(&config, "alpha").unwrap();
    let home = dirs::home_dir().unwrap();
    let expected = home.join("code/new-repo").to_string_lossy().into_owned();
    assert!(m.repos.contains(&expected), "repos: {:?}", m.repos);
}

#[test]
fn add_resource_appends_to_manifest() {
    let (_env, config) = TestEnv::setup();
    jeru::add_to_project(&config, "alpha", "~/docs/spec.md", Kind::Resource).unwrap();
    let m = jeru::load_manifest(&config, "alpha").unwrap();
    let home = dirs::home_dir().unwrap();
    let expected = home.join("docs/spec.md").to_string_lossy().into_owned();
    assert!(m.resources.contains(&expected), "resources: {:?}", m.resources);
}

#[test]
fn add_knowledge_extracts_id() {
    let (env, config) = TestEnv::setup();
    let knowledge_path = env.dir.path().join("knowledge/ml-notes");
    std::fs::create_dir_all(&knowledge_path).unwrap();
    let path_str = knowledge_path.to_string_lossy().into_owned();

    jeru::add_to_project(&config, "alpha", &path_str, Kind::Knowledge).unwrap();
    let m = jeru::load_manifest(&config, "alpha").unwrap();
    assert!(m.knowledge_sets.contains(&"ml-notes".to_string()));
}

#[test]
fn add_duplicate_returns_error() {
    let (_env, config) = TestEnv::setup();
    jeru::add_to_project(&config, "alpha", "~/code/dup", Kind::Repo).unwrap();
    assert!(jeru::add_to_project(&config, "alpha", "~/code/dup", Kind::Repo).is_err());
}

// ── remove_from_project ──────────────────────────────────────────────────────

#[test]
fn remove_repo_by_tilde_path() {
    let (_env, config) = TestEnv::setup();
    jeru::add_to_project(&config, "alpha", "~/code/to-remove", Kind::Repo).unwrap();
    // Remove using the same tilde path — must normalise before comparing
    jeru::remove_from_project(&config, "alpha", "~/code/to-remove", Kind::Repo).unwrap();
    let m = jeru::load_manifest(&config, "alpha").unwrap();
    let home = dirs::home_dir().unwrap();
    let abs = home.join("code/to-remove").to_string_lossy().into_owned();
    assert!(!m.repos.contains(&abs));
}

#[test]
fn remove_resource_by_tilde_path() {
    let (_env, config) = TestEnv::setup();
    jeru::add_to_project(&config, "alpha", "~/docs/spec.md", Kind::Resource).unwrap();
    jeru::remove_from_project(&config, "alpha", "~/docs/spec.md", Kind::Resource).unwrap();
    let m = jeru::load_manifest(&config, "alpha").unwrap();
    let home = dirs::home_dir().unwrap();
    let abs = home.join("docs/spec.md").to_string_lossy().into_owned();
    assert!(!m.resources.contains(&abs));
}

#[test]
fn remove_nonexistent_entry_returns_error() {
    let (_env, config) = TestEnv::setup();
    assert!(
        jeru::remove_from_project(&config, "alpha", "~/code/ghost", Kind::Repo).is_err()
    );
}

// ── detect_kind ──────────────────────────────────────────────────────────────

#[test]
fn detect_kind_knowledge_path() {
    let (env, config) = TestEnv::setup();
    let kpath = env.dir.path().join("knowledge/docs");
    std::fs::create_dir_all(&kpath).unwrap();
    let path_str = kpath.to_string_lossy().into_owned();
    assert_eq!(jeru::detect_kind(&config, &path_str).unwrap(), Kind::Knowledge);
}

#[test]
fn detect_kind_directory_is_repo() {
    let (env, config) = TestEnv::setup();
    let repo = env.dir.path().join("some-repo");
    std::fs::create_dir_all(&repo).unwrap();
    assert_eq!(
        jeru::detect_kind(&config, &repo.to_string_lossy()).unwrap(),
        Kind::Repo
    );
}

#[test]
fn detect_kind_file_extension_is_resource() {
    let (_env, config) = TestEnv::setup();
    assert_eq!(
        jeru::detect_kind(&config, "~/notes/spec.md").unwrap(),
        Kind::Resource
    );
}

#[test]
fn detect_kind_directory_with_extension_is_repo() {
    let (env, config) = TestEnv::setup();
    // A directory named with an extension — should still be detected as Repo
    // because the directory check takes priority over the extension check.
    let dir = env.dir.path().join("my-lib.rs");
    std::fs::create_dir_all(&dir).unwrap();
    assert_eq!(
        jeru::detect_kind(&config, &dir.to_string_lossy()).unwrap(),
        Kind::Repo
    );
}

// ── relative path handling ────────────────────────────────────────────────────

#[test]
fn add_repo_relative_path_stored_as_absolute() {
    let (env, config) = TestEnv::setup();
    let _cwd = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(env.dir.path()).unwrap();

    jeru::add_to_project(&config, "alpha", "projects/alpha", Kind::Repo).unwrap();

    std::env::set_current_dir(&original_dir).unwrap();

    let m = jeru::load_manifest(&config, "alpha").unwrap();
    let expected = env.dir.path().join("projects/alpha").to_string_lossy().into_owned();
    assert!(m.repos.contains(&expected), "repos: {:?}", m.repos);
}

#[test]
fn add_repo_dot_slash_stored_as_absolute() {
    let (env, config) = TestEnv::setup();
    let _cwd = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(env.dir.path()).unwrap();

    jeru::add_to_project(&config, "alpha", "./projects/alpha", Kind::Repo).unwrap();

    std::env::set_current_dir(&original_dir).unwrap();

    let m = jeru::load_manifest(&config, "alpha").unwrap();
    assert!(
        m.repos.iter().any(|r| {
            let p = std::path::Path::new(r);
            p.is_absolute() && r.contains("projects/alpha")
        }),
        "repos: {:?}",
        m.repos
    );
}

#[test]
fn add_resource_relative_path_stored_as_absolute() {
    let (env, config) = TestEnv::setup();
    let _cwd = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(env.dir.path()).unwrap();

    // Create a dummy resource file in the temp dir
    let resource = env.dir.path().join("spec.md");
    std::fs::write(&resource, "# spec").unwrap();

    jeru::add_to_project(&config, "alpha", "spec.md", Kind::Resource).unwrap();

    std::env::set_current_dir(&original_dir).unwrap();

    let m = jeru::load_manifest(&config, "alpha").unwrap();
    let expected = resource.to_string_lossy().into_owned();
    assert!(m.resources.contains(&expected), "resources: {:?}", m.resources);
}

#[test]
fn add_duplicate_via_absolute_after_relative_returns_error() {
    let (env, config) = TestEnv::setup();
    let _cwd = CWD_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(env.dir.path()).unwrap();

    jeru::add_to_project(&config, "alpha", "projects/alpha", Kind::Repo).unwrap();

    // Adding the same path as absolute should be detected as duplicate
    let abs = env.dir.path().join("projects/alpha").to_string_lossy().into_owned();
    let result = jeru::add_to_project(&config, "alpha", &abs, Kind::Repo);

    std::env::set_current_dir(&original_dir).unwrap();

    assert!(result.is_err(), "expected duplicate error");
}

// ── roadmap ───────────────────────────────────────────────────────────────────

#[test]
fn init_claude_md_includes_roadmap_when_file_exists() {
    let (env, config) = TestEnv::setup();
    // Create a ROADMAP.md in the project dir
    let roadmap = env.project_dir("alpha").join("ROADMAP.md");
    fs::write(&roadmap, "## Goals\n- [ ] do something\n").unwrap();

    let path = jeru::init_claude_md(&config, "alpha", false).unwrap();
    let content = fs::read_to_string(path).unwrap();
    assert!(content.contains("ROADMAP.md"));
    assert!(content.contains("Roadmap"));
}

#[test]
fn init_claude_md_no_roadmap_section_when_file_missing() {
    let (_env, config) = TestEnv::setup();
    let path = jeru::init_claude_md(&config, "alpha", false).unwrap();
    let content = fs::read_to_string(path).unwrap();
    assert!(!content.contains("Roadmap"));
}
