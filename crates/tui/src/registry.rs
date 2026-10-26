//! The TUI project registry: a tiny hand-parsed TOML subset so the workspace
//! keeps its deliberately minimal dependency set (no `toml` crate).
//!
//! ```toml
//! # DecisionSpec TUI registry — managed by `decispec tui --register <path>`.
//!
//! [[project]]
//! name = "tvgo"
//! path = "/home/me/Projects/FPF/tvgo"
//! ```

use std::fmt::Write as _;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectEntry {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Registry {
    pub projects: Vec<ProjectEntry>,
}

impl Registry {
    /// Parse the registry file. Errors carry a `registry.toml:<line>` prefix.
    pub fn parse(src: &str) -> Result<Registry, String> {
        let mut reg = Registry::default();
        for (i, raw) in src.lines().enumerate() {
            let n = i + 1;
            let line = strip_comment(raw);
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if line == "[[project]]" {
                reg.projects.push(ProjectEntry {
                    name: String::new(),
                    path: String::new(),
                });
                continue;
            }
            let Some((k, v)) = line.split_once('=') else {
                return Err(format!(
                    "registry.toml:{n}: expected `key = \"value\"` or `[[project]]`, got `{line}`"
                ));
            };
            let Some(entry) = reg.projects.last_mut() else {
                return Err(format!(
                    "registry.toml:{n}: `{k}` outside a [[project]] block"
                ));
            };
            let val = v.trim().trim_matches('"').to_string();
            match k.trim() {
                "name" => entry.name = val,
                "path" => entry.path = val,
                other => return Err(format!("registry.toml:{n}: unknown key `{other}`")),
            }
        }
        for (i, p) in reg.projects.iter().enumerate() {
            if p.name.is_empty() || p.path.is_empty() {
                return Err(format!(
                    "registry.toml: project #{} is missing `name` or `path`",
                    i + 1
                ));
            }
        }
        Ok(reg)
    }

    /// Serialize back to the canonical registry format (round-trips with `parse`).
    pub fn to_toml(&self) -> String {
        let mut out =
            String::from("# DecisionSpec TUI registry — managed by `decispec tui --register <path>`.\n");
        for p in &self.projects {
            out.push_str("\n[[project]]\n");
            let _ = writeln!(out, "name = \"{}\"", p.name);
            let _ = writeln!(out, "path = \"{}\"", p.path);
        }
        out
    }

    /// Insert or replace an entry (match on name OR path), keeping one entry
    /// per project.
    pub fn upsert(&mut self, entry: ProjectEntry) {
        self.projects
            .retain(|p| p.name != entry.name && p.path != entry.path);
        self.projects.push(entry);
    }
}

/// Strip a `#` comment that appears outside double quotes.
fn strip_comment(raw: &str) -> String {
    let mut out = String::new();
    let mut in_quotes = false;
    for ch in raw.trim().chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                out.push(ch);
            }
            '#' if !in_quotes => break,
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_toml_and_parse_round_trip() {
        let reg = Registry {
            projects: vec![
                ProjectEntry {
                    name: "tvgo".to_string(),
                    path: "/home/me/Projects/FPF/tvgo".to_string(),
                },
                ProjectEntry {
                    name: "verydian".to_string(),
                    path: "/home/me/Projects/FPF/verydian-app".to_string(),
                },
            ],
        };
        let text = reg.to_toml();
        assert!(text.contains("[[project]]"));
        assert_eq!(Registry::parse(&text).unwrap(), reg);
    }

    #[test]
    fn parse_ignores_comments_and_blank_lines() {
        let src = "# header comment\n\n[[project]]\nname = \"a\" # inline\npath = \"/x\"\n";
        let reg = Registry::parse(src).unwrap();
        assert_eq!(
            reg.projects,
            vec![ProjectEntry {
                name: "a".to_string(),
                path: "/x".to_string()
            }]
        );
    }

    #[test]
    fn parse_keeps_hash_inside_quotes() {
        let src = "[[project]]\nname = \"a#b\"\npath = \"/x#y\"\n";
        let reg = Registry::parse(src).unwrap();
        assert_eq!(
            reg.projects,
            vec![ProjectEntry {
                name: "a#b".to_string(),
                path: "/x#y".to_string()
            }]
        );
    }

    #[test]
    fn parse_errors_on_key_outside_project_block() {
        let err = Registry::parse("name = \"a\"\n").unwrap_err();
        assert!(err.contains("registry.toml:1"), "error: {err}");
    }

    #[test]
    fn parse_errors_on_unknown_key_with_line_number() {
        let err = Registry::parse("[[project]]\nname = \"a\"\npath = \"/x\"\nnope = 1\n")
            .unwrap_err();
        assert!(err.contains("registry.toml:4"), "error: {err}");
        assert!(err.contains("nope"), "error: {err}");
    }

    #[test]
    fn parse_errors_on_entry_missing_path() {
        let err = Registry::parse("[[project]]\nname = \"a\"\n").unwrap_err();
        assert!(err.contains("missing"), "error: {err}");
    }

    #[test]
    fn parse_errors_on_garbage_line() {
        let err = Registry::parse("[[project]]\nwhat is this\n").unwrap_err();
        assert!(err.contains("registry.toml:2"), "error: {err}");
    }

    #[test]
    fn parse_empty_source_is_empty_registry() {
        assert_eq!(Registry::parse("").unwrap(), Registry::default());
    }

    #[test]
    fn upsert_replaces_same_name_and_same_path() {
        let mut reg = Registry {
            projects: vec![
                ProjectEntry {
                    name: "tvgo".to_string(),
                    path: "/old".to_string(),
                },
                ProjectEntry {
                    name: "other".to_string(),
                    path: "/x".to_string(),
                },
            ],
        };
        reg.upsert(ProjectEntry {
            name: "tvgo".to_string(),
            path: "/new".to_string(),
        });
        assert_eq!(reg.projects.len(), 2);
        let tvgo = reg.projects.iter().find(|p| p.name == "tvgo").unwrap();
        assert_eq!(tvgo.path, "/new");
        // same path under a new name also replaces, no duplicate path
        reg.upsert(ProjectEntry {
            name: "renamed".to_string(),
            path: "/x".to_string(),
        });
        assert_eq!(reg.projects.len(), 2);
        assert_eq!(reg.projects[1].name, "renamed");
    }
}
