use crate::config::Config;
use crate::error::Result;
use crate::manifest::Manifest;
use crate::project::{expand_tilde, project_dir};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueKind {
    MissingManifest,
    MissingRepo,
    MissingKnowledge,
    MissingResource,
    MissingKnowledgeLocation,
}

impl IssueKind {
    pub fn tag(self) -> &'static str {
        match self {
            IssueKind::MissingManifest => "missing-manifest",
            IssueKind::MissingRepo => "missing-repo",
            IssueKind::MissingKnowledge => "missing-knowledge",
            IssueKind::MissingResource => "missing-resource",
            IssueKind::MissingKnowledgeLocation => "missing-knowledge-location",
        }
    }
}

#[derive(Debug)]
pub struct Issue {
    pub kind: IssueKind,
    pub message: String,
}

/// Validate a named project and return any issues found.
///
/// Returns `Ok(vec![])` for a clean project. Only returns `Err` for I/O
/// errors unrelated to the validation checks themselves.
pub fn validate_project(config: &Config, name: &str) -> Result<Vec<Issue>> {
    let dir = project_dir(config, name);
    if !dir.is_dir() {
        return Ok(vec![Issue {
            kind: IssueKind::MissingManifest,
            message: "project directory does not exist".to_string(),
        }]);
    }

    let manifest = match Manifest::load_from_dir(&dir) {
        Ok(m) => m,
        Err(_) => {
            return Ok(vec![Issue {
                kind: IssueKind::MissingManifest,
                message: "no manifest (project.yml) found".to_string(),
            }]);
        }
    };

    check_manifest(config, &manifest)
}

fn check_manifest(config: &Config, manifest: &Manifest) -> Result<Vec<Issue>> {
    let mut issues = Vec::new();

    for repo in &manifest.repos {
        let path = expand_tilde(repo)?;
        if !path.is_dir() {
            issues.push(Issue {
                kind: IssueKind::MissingRepo,
                message: format!("repo '{repo}' does not exist"),
            });
        }
    }

    if let Some(repo) = &manifest.primary_repo {
        let path = expand_tilde(repo)?;
        if !path.is_dir() {
            issues.push(Issue {
                kind: IssueKind::MissingRepo,
                message: format!("primary_repo '{repo}' does not exist"),
            });
        }
    }

    for ks in &manifest.knowledge_sets {
        let path = config.knowledge_dir.join(ks);
        if !path.is_dir() {
            issues.push(Issue {
                kind: IssueKind::MissingKnowledge,
                message: format!("knowledge set '{ks}' does not exist"),
            });
        }
    }

    for res in &manifest.resources {
        let path = expand_tilde(res)?;
        if !path.exists() {
            issues.push(Issue {
                kind: IssueKind::MissingResource,
                message: format!("resource '{res}' does not exist"),
            });
        }
    }

    let kl_path = config
        .knowledge_dir
        .join("project")
        .join(&manifest.knowledge_location);
    if !kl_path.is_dir() {
        issues.push(Issue {
            kind: IssueKind::MissingKnowledgeLocation,
            message: format!(
                "knowledge location '{}' does not exist ({})",
                manifest.knowledge_location,
                kl_path.display()
            ),
        });
    }

    Ok(issues)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    struct Env {
        _dir: TempDir,
        config: Config,
    }

    impl Env {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let config = Config {
                projects_dir: dir.path().join("projects"),
                knowledge_dir: dir.path().join("knowledge"),
                cache_dir: dir.path().join("cache"),
            };
            std::fs::create_dir_all(&config.projects_dir).unwrap();
            std::fs::create_dir_all(&config.knowledge_dir).unwrap();
            Env { _dir: dir, config }
        }

        fn project_dir(&self, name: &str) -> PathBuf {
            self.config.projects_dir.join(name)
        }

        fn write_manifest(&self, name: &str, content: &str) {
            let dir = self.project_dir(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("project.yml"), content).unwrap();
        }
    }

    #[test]
    fn clean_manifest_returns_no_issues() {
        let env = Env::new();
        let kl = env.config.knowledge_dir.join("project").join("myproj");
        std::fs::create_dir_all(&kl).unwrap();
        env.write_manifest("myproj", "name: myproj\nknowledge_location: myproj\n");

        let issues = validate_project(&env.config, "myproj").unwrap();
        assert!(issues.is_empty(), "expected no issues, got: {issues:?}");
    }

    #[test]
    fn missing_project_dir_reports_missing_manifest() {
        let env = Env::new();
        let issues = validate_project(&env.config, "ghost").unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, IssueKind::MissingManifest);
    }

    #[test]
    fn empty_project_dir_reports_missing_manifest() {
        let env = Env::new();
        std::fs::create_dir_all(env.project_dir("empty")).unwrap();
        let issues = validate_project(&env.config, "empty").unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, IssueKind::MissingManifest);
    }

    #[test]
    fn missing_repo_is_reported() {
        let env = Env::new();
        let kl = env.config.knowledge_dir.join("project").join("p");
        std::fs::create_dir_all(&kl).unwrap();
        env.write_manifest(
            "p",
            &format!(
                "name: p\nknowledge_location: p\nrepos:\n  - {}/no-such-repo\n",
                env.config.projects_dir.display()
            ),
        );
        let issues = validate_project(&env.config, "p").unwrap();
        assert!(issues.iter().any(|i| i.kind == IssueKind::MissingRepo));
    }

    #[test]
    fn missing_primary_repo_is_reported() {
        let env = Env::new();
        let kl = env.config.knowledge_dir.join("project").join("p");
        std::fs::create_dir_all(&kl).unwrap();
        env.write_manifest(
            "p",
            &format!(
                "name: p\nknowledge_location: p\nprimary_repo: {}/no-such\n",
                env.config.projects_dir.display()
            ),
        );
        let issues = validate_project(&env.config, "p").unwrap();
        assert!(issues.iter().any(|i| i.kind == IssueKind::MissingRepo));
    }

    #[test]
    fn missing_knowledge_set_is_reported() {
        let env = Env::new();
        let kl = env.config.knowledge_dir.join("project").join("p");
        std::fs::create_dir_all(&kl).unwrap();
        env.write_manifest(
            "p",
            "name: p\nknowledge_location: p\nknowledge_sets:\n  - nonexistent/set\n",
        );
        let issues = validate_project(&env.config, "p").unwrap();
        assert!(issues.iter().any(|i| i.kind == IssueKind::MissingKnowledge));
    }

    #[test]
    fn missing_resource_is_reported() {
        let env = Env::new();
        let kl = env.config.knowledge_dir.join("project").join("p");
        std::fs::create_dir_all(&kl).unwrap();
        env.write_manifest(
            "p",
            &format!(
                "name: p\nknowledge_location: p\nresources:\n  - {}/no-such-file.pdf\n",
                env.config.projects_dir.display()
            ),
        );
        let issues = validate_project(&env.config, "p").unwrap();
        assert!(issues.iter().any(|i| i.kind == IssueKind::MissingResource));
    }

    #[test]
    fn missing_knowledge_location_is_reported() {
        let env = Env::new();
        env.write_manifest("p", "name: p\nknowledge_location: missing-loc\n");
        let issues = validate_project(&env.config, "p").unwrap();
        assert!(issues
            .iter()
            .any(|i| i.kind == IssueKind::MissingKnowledgeLocation));
    }
}
