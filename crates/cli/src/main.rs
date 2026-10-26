//! `decispec` binary: init / check / gen / test / gate / query.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use clap::Parser;
use decispec_codegen::{CodegenConfig, render_all_for};
use decispec_gate::{
    AdapterRecord, AdapterStatus, Filter, GateInput, Layer as GateLayer, Manifest, Verdict,
};
use decispec_ir::{Layer, Status, Workspace};
use decispec_store::{DecisionRun, RunRecord, Store};
use decispec_tui::app::{Action, App, Input, ProjectData};
use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

#[derive(Debug, thiserror::Error)]
enum CliError {
    #[error("{0}")]
    Msg(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Gate(#[from] decispec_gate::GateError),
    #[error(transparent)]
    Extract(#[from] decispec_extract::ExtractError),
    #[error(transparent)]
    Store(#[from] decispec_store::StoreError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

type CliResult<T> = Result<T, CliError>;

/// The operation completed and there is nothing to report.
const EXIT_OK: u8 = 0;
/// The operation completed and reported findings (gate FAIL, check spec errors, a failed toolchain, a query miss).
const EXIT_FINDINGS: u8 = 1;
/// The operation could not be completed (bad CLI usage, unusable workspace, I/O or engine errors).
const EXIT_CANNOT_COMPLETE: u8 = 2;

#[derive(Parser)]
#[command(
    name = "decispec",
    version,
    about = "DecisionSpec: spec-driven compliance compiler and gate"
)]
struct Cli {
    #[command(subcommand)]
    command: CommandKind,
}

#[derive(clap::Subcommand)]
enum CommandKind {
    /// Initialize a new DecisionSpec project (decispec.toml, specs/, tests/glue/)
    Init {
        /// Write a demo spec (specs/0001-auth.spec) + glue stubs
        #[arg(long)]
        demo: bool,
        /// Scaffold the specs/ tree from stored .spec files and/or store
        /// directories (a directory contributes its direct .spec children);
        /// validated as a workspace before anything is written
        #[arg(long, value_name = "PATH")]
        from: Vec<PathBuf>,
    },
    /// Parse specs/**/*.spec and report errors (dup IDs, unknown refs, ...)
    Check {
        /// Also apply the authoring-style lint (docs/SPEC_AUTHORING.md):
        /// EARS requirement text, decision context/consequences shape,
        /// snake_case scenario names, every scenario claimed by a requirement
        #[arg(long)]
        strict: bool,
    },
    /// Generate diagrams and test artifacts from the IR
    Gen {
        /// Only write artifacts of specs whose id starts with these prefixes
        /// (comma-separated, e.g. CMD,DSL); workspace-scoped outputs and the
        /// stale sweep are skipped
        #[arg(long, value_name = "PREFIX[,PREFIX...]")]
        only: Option<String>,
    },
    /// Run available toolchains and collect JUnit XML + manifest
    Test,
    /// Compliance gate: traceability matrix + verdict
    Gate {
        /// Print the machine-readable report to stdout as well
        #[arg(long)]
        json: bool,
        /// Consume JUnit results from this dir instead of running tests first
        #[arg(long)]
        results: Option<PathBuf>,
        /// Treat SKIPPED rows as FAIL: a toolchain that never ran, or a JUnit
        /// case reported as <skipped> (GATE-008)
        #[arg(long)]
        strict_skipped: bool,
        /// TOML file of rows allowed to stay skipped under --strict-skipped
        /// (the ratchet, GATE-009): [[skips]] tables with decision, item, and
        /// a reason; `requirement` optional
        #[arg(long, value_name = "PATH")]
        allow_skips: Option<PathBuf>,
        /// Print only decisions whose id starts with these prefixes
        /// (comma-separated, e.g. CMD,DSL) — presentation-only
        #[arg(long, value_name = "PREFIX[,PREFIX...]")]
        prefix: Option<String>,
        /// Print only decisions with this status
        /// (accepted|proposed|rejected|superseded) — presentation-only
        #[arg(long, value_name = "STATUS")]
        status: Option<String>,
        /// Print only rows that failed — presentation-only
        #[arg(long)]
        only_failing: bool,
        /// Print only UNCOVERED / NO-ARTIFACTS rows — presentation-only
        #[arg(long)]
        only_uncovered: bool,
    },
    /// Print the decision (or requirement/scenario) subtree: the agent view.
    /// An id containing `*` is a glob listing matching decisions one line each
    Query {
        id: String,
        /// Only list decisions with this status
        /// (accepted|proposed|rejected|superseded)
        #[arg(long, value_name = "STATUS")]
        status: Option<String>,
    },
    /// Print the per-decision proof rollup from the last gate report:
    /// what percentage of the specs are proven by test (PROOF-001..005)
    Coverage {
        /// Exit 1 when the headline percentage is below this floor (CI hook)
        #[arg(long, value_parser = clap::value_parser!(u64).range(0..=100))]
        min: Option<u64>,
    },
    /// Bootstrap a draft spec from an existing Python/JS/TS project
    /// (containers from the directory layout, rels from the import graph)
    Extract {
        /// Project directory to analyze (default: the DecisionSpec project root
        /// found by walking up from cwd)
        path: Option<PathBuf>,
        /// Print the draft spec to stdout instead of writing specs/extracted.spec
        #[arg(long, conflicts_with = "to")]
        stdout: bool,
        /// Write <dir>/<project-dirname>.spec into an external store directory
        /// (never into the analyzed project's own specs/ tree); the target is
        /// never overwritten — an existing file is exit 2, byte-identical
        #[arg(long, value_name = "DIR")]
        to: Option<PathBuf>,
    },
    /// Serve MCP over stdio (NDJSON JSON-RPC 2.0) for AI agents
    Mcp,
    /// Write a single-file, GitHub-renderable decision index (markdown with
    /// embedded mermaid diagrams and decision tables)
    Index {
        /// Output path relative to the project root (default:
        /// .decispec/docs/decisions.md — the gitignored generated area)
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Read-only spec cockpit over registered projects (ratatui TUI).
    /// Registered projects are never written; generated artifacts are
    /// rendered in memory for display only.
    Tui {
        /// Register a project directory in the TUI registry, then exit
        #[arg(long, value_name = "DIR")]
        register: Option<PathBuf>,
    },
}

// ---------------------------------------------------------------------------
// Project discovery + config
// ---------------------------------------------------------------------------

fn find_project_root() -> CliResult<PathBuf> {
    let mut dir = std::env::current_dir()?;
    loop {
        if dir.join("decispec.toml").is_file() {
            return Ok(dir);
        }
        if !dir.pop() {
            return Err(CliError::Msg(
                "decispec.toml not found (walked up from cwd); run `decispec init` first"
                    .to_string(),
            ));
        }
    }
}

#[derive(Debug, Clone)]
struct Config {
    project: String,
    lang: String,
    glue_module: String,
    rust_glue: String,
    /// `[stack.fitness] root_package`: python package import-linter is rooted
    /// at. Empty means unconfigured.
    root_package: String,
    /// `[stack.fitness.packages]` container-id -> python module path.
    container_packages: BTreeMap<String, String>,
    /// `[runners]` tool -> command override (program + fixed prefix args).
    /// Empty means every adapter uses its built-in default command.
    runners: BTreeMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            project: "my-project".to_string(),
            lang: "python".to_string(),
            glue_module: "tests.glue".to_string(),
            rust_glue: "tests/glue/rust".to_string(),
            root_package: String::new(),
            container_packages: BTreeMap::new(),
            runners: BTreeMap::new(),
        }
    }
}

/// Minimal TOML reader: just the flat `key = "value"` pairs DecisionSpec writes.
/// Inline comments (`#` outside quotes) are stripped.
fn parse_config(src: &str) -> Config {
    let mut cfg = Config::default();
    let mut section = String::new();
    for raw in src.lines() {
        let mut line = String::new();
        let mut in_quotes = false;
        for ch in raw.trim().chars() {
            match ch {
                '"' => {
                    in_quotes = !in_quotes;
                    line.push(ch);
                }
                '#' if !in_quotes => break,
                _ => line.push(ch),
            }
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].trim().to_string();
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let v = v.trim().trim_matches('"').to_string();
        match (section.as_str(), k.trim()) {
            ("project", "name") => cfg.project = v,
            ("stack", "lang") => cfg.lang = v,
            ("stack", "glue_module") => cfg.glue_module = v,
            ("stack", "rust_glue") => cfg.rust_glue = v,
            ("stack.fitness", "root_package") => cfg.root_package = v,
            ("stack.fitness.packages", container) => {
                cfg.container_packages.insert(container.to_string(), v);
            }
            ("runners", tool) => {
                cfg.runners.insert(tool.to_string(), v);
            }
            _ => {}
        }
    }
    cfg
}

fn load_config(root: &Path) -> CliResult<Config> {
    let src = std::fs::read_to_string(root.join("decispec.toml"))?;
    Ok(parse_config(&src))
}

// ---------------------------------------------------------------------------
// Workspace loading
// ---------------------------------------------------------------------------

fn spec_files(root: &Path) -> CliResult<Vec<PathBuf>> {
    let mut out = Vec::new();
    let dir = root.join("specs");
    if dir.is_dir() {
        collect_spec_files(&dir, &mut out)?;
    }
    out.sort();
    Ok(out)
}

fn collect_spec_files(dir: &Path, out: &mut Vec<PathBuf>) -> CliResult<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_spec_files(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "spec") {
            out.push(path);
        }
    }
    Ok(())
}

/// Parse + validate all specs. On any error, returns the list of
/// already-formatted `file:line: message` strings.
fn load_workspace(root: &Path) -> Result<Workspace, Vec<String>> {
    let files = match spec_files(root) {
        Ok(f) => f,
        Err(e) => return Err(vec![format!("cannot read specs/: {e}")]),
    };
    if files.is_empty() {
        return Err(vec!["no .spec files found under specs/".to_string()]);
    }
    let mut units = Vec::new();
    let mut errors = Vec::new();
    for f in &files {
        let rel = f
            .strip_prefix(root)
            .unwrap_or(f)
            .to_string_lossy()
            .to_string();
        let src = match std::fs::read_to_string(f) {
            Ok(s) => s,
            Err(e) => {
                errors.push(format!("{rel}: cannot read file: {e}"));
                continue;
            }
        };
        match decispec_parse::parse_file(&rel, &src) {
            Ok(u) => units.push(u),
            Err(e) => errors.push(e.to_string()),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let ws = decispec_parse::merge(units);
    let diags: Vec<String> = decispec_parse::validate(&ws)
        .iter()
        .map(|d| d.to_string())
        .collect();
    if !diags.is_empty() {
        return Err(diags);
    }
    Ok(ws)
}

fn print_errors(errors: &[String]) {
    eprintln!("{} error(s):", errors.len());
    for e in errors {
        eprintln!("  {e}");
    }
}

/// Load the workspace for a command that cannot proceed without one: prints
/// the diagnostics and turns them into an error, so the process exits with
/// "could not complete" instead of a findings code.
fn load_workspace_or_bail(root: &Path) -> CliResult<Workspace> {
    match load_workspace(root) {
        Ok(ws) => Ok(ws),
        Err(errors) => {
            print_errors(&errors);
            Err(CliError::Msg(
                "specs failed to parse/validate — run `decispec check` for details".to_string(),
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// init
// ---------------------------------------------------------------------------

const DEMO_SPEC: &str = r#"
# Demo spec for DecisionSpec. See README.md for the full DSL reference.
decision AUTH-001 "Use OAuth2 client credentials for service-to-service auth" {
  status: accepted                # accepted | proposed | rejected | superseded
  context: "Internal services need machine-to-machine auth without shared secrets"
  consequences: "+ no shared secrets in code; - token latency on cold start"
}

model {
  container api "Orders API"
  container auth "Auth Service"
  rel api -> auth: "requests token"
  flow svc_auth {
    api -> auth: "POST /token"
    alt valid {
      auth --> api: "200 + access token"
    } else {
      auth --> api: "401 unauthorized"
    }
  }
}

spec AUTH-001 {
  requirement AUTH-001-R1 (EARS) {
    text: "WHEN a service presents valid client credentials THEN the auth service SHALL return an access token"
    layers: [unit, contract, e2e]
    scenarios: [valid_token]
  }
  requirement AUTH-001-R2 (EARS) {
    text: "WHEN credentials are invalid THEN the auth service SHALL reject with 401"
    layers: [unit]
    scenarios: [invalid_token]
  }
  scenario valid_token {
    given: "valid client credentials"
    when: "a token request is made"
    then: "the response status is 200 and a token is returned"
  }
  scenario invalid_token {
    given: "invalid client credentials"
    when: "a token request is made"
    then: "the response status is 401"
  }
  invariant token_ttl {
    expr: "access_token.ttl_seconds <= 3600"
    layers: [unit]
  }
  infra_policy no_hardcoded_secrets {
    description: "No secrets committed in source or config"
    layers: [infra]
  }
}
"#;

const DEMO_GLUE: &str = r#""""Glue functions for the DecisionSpec demo spec AUTH-001.

Replace each body with a real check that exercises your code. The generated
tests in tests/generated/unit/ call these functions by name.
"""


def valid_token():
    raise NotImplementedError("implement per spec AUTH-001")


def invalid_token():
    raise NotImplementedError("implement per spec AUTH-001")


def invariant_token_ttl():
    raise NotImplementedError("implement per spec AUTH-001")
"#;

/// The .spec files an `init --from` argument list selects: each file argument
/// that ends in .spec, plus the direct .spec children of each directory
/// argument (one level deep — a nested store layout needs one argument per
/// directory, PORT-002 consequences).
fn select_store_specs(paths: &[PathBuf]) -> Result<Vec<(String, PathBuf)>, String> {
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    for p in paths {
        if p.is_dir() {
            let mut children: Vec<PathBuf> = std::fs::read_dir(p)
                .map_err(|e| format!("cannot read store directory {}: {e}", p.display()))?
                .flatten()
                .map(|e| e.path())
                .filter(|c| c.is_file() && c.extension().is_some_and(|e| e == "spec"))
                .collect();
            children.sort();
            for c in children {
                out.push((c.file_name().unwrap().to_string_lossy().to_string(), c));
            }
        } else if p.is_file() && p.extension().is_some_and(|e| e == "spec") {
            out.push((p.file_name().unwrap().to_string_lossy().to_string(), p.clone()));
        } else {
            return Err(format!(
                "--from argument {} is not a .spec file or a store directory",
                p.display()
            ));
        }
    }
    Ok(out)
}

/// Parse + validate the selected store files as one workspace. `Ok` carries
/// (file name, bytes) ready to copy; `Err` carries file:line diagnostics —
/// init must not write a half-born project from a broken store (PORT-002-R2).
fn validate_store_specs(files: &[(String, PathBuf)]) -> Result<Vec<(String, String)>, Vec<String>> {
    let mut units = Vec::new();
    let mut errors = Vec::new();
    for (_name, path) in files {
        let src = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                errors.push(format!("{}: cannot read file: {e}", path.display()));
                continue;
            }
        };
        match decispec_parse::parse_file(&path.to_string_lossy(), &src) {
            Ok(u) => units.push(u),
            Err(e) => errors.push(e.to_string()),
        }
    }
    if errors.is_empty() {
        let ws = decispec_parse::merge(units);
        errors.extend(
            decispec_parse::validate(&ws)
                .iter()
                .map(|d| d.to_string()),
        );
    }
    if errors.is_empty() {
        Ok(files
            .iter()
            .map(|(name, path)| {
                (
                    name.clone(),
                    std::fs::read_to_string(path).expect("read once above"),
                )
            })
            .collect())
    } else {
        Err(errors)
    }
}

fn cmd_init(root: &Path, demo: bool, from: Vec<PathBuf>) -> CliResult<ExitCode> {
    if root.join("decispec.toml").exists() {
        return Err(CliError::Msg(format!(
            "{} already contains a decispec.toml",
            root.display()
        )));
    }
    // Clobber refusal comes before any store read: an existing specs/ tree is
    // left byte-identical (PORT-002-R1).
    if let Ok(existing) = std::fs::read_dir(root.join("specs"))
        && existing.flatten().any(|e| {
            e.path().is_file() && e.path().extension().is_some_and(|x| x == "spec")
        })
    {
        return Err(CliError::Msg(format!(
            "{} already contains .spec files — init --from never clobbers an existing specs/ tree",
            root.display()
        )));
    }

    // The store is validated as a workspace BEFORE the scaffold is written:
    // a broken store file produces diagnostics, not a half-born project.
    let mut copies: Vec<(String, String)> = Vec::new();
    if !from.is_empty() {
        let selected = select_store_specs(&from).map_err(CliError::Msg)?;
        copies = validate_store_specs(&selected).map_err(|diags| {
            eprintln!("{} error(s) in stored specs:", diags.len());
            for d in &diags {
                eprintln!("  {d}");
            }
            CliError::Msg("stored specs failed validation — nothing was written".to_string())
        })?;
    }
    if demo {
        let demo_name = "0001-auth.spec".to_string();
        if copies.iter().any(|(n, _)| *n == demo_name) {
            return Err(CliError::Msg(
                "--from selects a file named 0001-auth.spec, which --demo also writes — pick one"
                    .to_string(),
            ));
        }
        copies.push((demo_name, DEMO_SPEC.trim_start_matches('\n').to_string()));
    }
    let mut seen = std::collections::BTreeSet::new();
    for (name, _) in &copies {
        if !seen.insert(name) {
            return Err(CliError::Msg(format!(
                "--from selects the file name {name} twice — store entries must have distinct names"
            )));
        }
    }

    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "my-project".to_string());

    std::fs::create_dir_all(root.join("specs"))?;
    std::fs::create_dir_all(root.join("tests/glue"))?;
    std::fs::create_dir_all(root.join(".decispec"))?;

    let toml = format!(
        "[project]\nname = \"{name}\"\n\n[stack]\nlang = \"python\"              # python | rust (more later)\nglue_module = \"tests.glue\"   # python glue import path\n\n[runners]\n# optional per-tool overrides; built-in defaults otherwise\n"
    );
    std::fs::write(root.join("decispec.toml"), toml)?;

    std::fs::write(root.join("tests/glue/conftest.py"), "")?;
    let glue = if demo { DEMO_GLUE } else { "" };
    std::fs::write(root.join("tests/glue/__init__.py"), glue)?;

    for (file_name, text) in &copies {
        std::fs::write(root.join("specs").join(file_name), text)?;
    }

    // .gitignore entries (append only what is missing).
    let gitignore_path = root.join(".gitignore");
    let existing = std::fs::read_to_string(&gitignore_path).unwrap_or_default();
    let mut additions = String::new();
    for entry in ["target/", "tests/generated/", ".decispec/", "__pycache__/"] {
        if !existing.lines().any(|l| l.trim() == entry) {
            additions.push_str(entry);
            additions.push('\n');
        }
    }
    if !additions.is_empty() {
        let mut content = existing;
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(&additions);
        std::fs::write(&gitignore_path, content)?;
    }

    println!("initialized DecisionSpec project in {}", root.display());
    println!("  decispec.toml, specs/, tests/glue/, .decispec/");
    if demo {
        println!("  demo: specs/0001-auth.spec + tests/glue/__init__.py stubs");
    }
    if !copies.is_empty() {
        println!("  imported {} spec file(s) from the store", copies.len());
    }
    println!("next: decispec check && decispec gen");
    Ok(ExitCode::from(EXIT_OK))
}

// ---------------------------------------------------------------------------
// check
// ---------------------------------------------------------------------------

fn cmd_check(root: &Path, strict: bool) -> CliResult<ExitCode> {
    match load_workspace(root) {
        Ok(ws) => {
            let reqs: usize = ws.specs.iter().map(|s| s.requirements.len()).sum();
            let scens: usize = ws.specs.iter().map(|s| s.scenarios.len()).sum();
            let invs: usize = ws.specs.iter().map(|s| s.invariants.len()).sum();
            let pols: usize = ws.specs.iter().map(|s| s.infra_policies.len()).sum();
            println!(
                "ok: {} decision(s), {} model(s), {} spec(s), {reqs} requirement(s), {scens} scenario(s), {invs} invariant(s), {pols} infra policy(ies)",
                ws.decisions.len(),
                ws.models.len(),
                ws.specs.len()
            );
            if strict {
                let lints = decispec_parse::lint(&ws);
                if !lints.is_empty() {
                    eprintln!("{} style finding(s):", lints.len());
                    for l in &lints {
                        eprintln!("  {l}");
                    }
                    return Ok(ExitCode::from(EXIT_FINDINGS));
                }
            }
            Ok(ExitCode::from(EXIT_OK))
        }
        // Reporting malformed specs IS check's job: those are findings.
        Err(errors) => {
            print_errors(&errors);
            Ok(ExitCode::from(EXIT_FINDINGS))
        }
    }
}

// ---------------------------------------------------------------------------
// gen
// ---------------------------------------------------------------------------

/// Split a comma-separated `--prefix`/`--only` value into trimmed,
/// non-empty entries (shared shape across gate and gen).
fn split_prefixes(s: &str) -> Vec<String> {
    s.split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect()
}

fn cmd_gen(root: &Path, only: Option<&[String]>) -> CliResult<ExitCode> {
    let ws = load_workspace_or_bail(root)?;
    let cfg = load_config(root)?;
    if let Some(prefixes) = only {
        let matched = ws
            .specs
            .iter()
            .any(|s| prefixes.iter().any(|p| s.id.starts_with(p.as_str())));
        if !matched {
            println!("no decision/spec matches --only {}", prefixes.join(","));
            return Ok(ExitCode::from(EXIT_FINDINGS));
        }
    }
    let removed = write_artifacts(root, &ws, &cfg, only)?;
    for r in &removed {
        println!("removed stale {r}");
    }
    Ok(ExitCode::from(EXIT_OK))
}

/// Render `ws`, write every artifact under `root`, then (only after all
/// writes succeeded) sweep stale generated files. Returns the sorted
/// project-relative paths of removed stale artifacts.
/// With `only` set, only matching specs' artifacts are written and the
/// stale sweep is skipped entirely (a partial render must not delete the
/// rest).
fn write_artifacts(
    root: &Path,
    ws: &Workspace,
    cfg: &Config,
    only: Option<&[String]>,
) -> CliResult<Vec<String>> {
    let prefixes = only.unwrap_or(&[]);
    let files = render_all_for(
        ws,
        &CodegenConfig {
            lang: cfg.lang.clone(),
            glue_module: cfg.glue_module.clone(),
            rust_glue: cfg.rust_glue.clone(),
            root_package: cfg.root_package.clone(),
            container_packages: cfg.container_packages.clone(),
        },
        prefixes,
    );
    let mut diagrams = 0usize;
    let mut tests = 0usize;
    for f in &files {
        let path = root.join(&f.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, &f.content)?;
        if f.path.starts_with(".decispec/diagrams/") {
            diagrams += 1;
        } else {
            tests += 1;
        }
        println!("wrote {}", f.path);
    }
    if only.is_some() {
        println!(
            "done: {tests} test artifact(s) written for --only; workspace outputs and the stale sweep skipped (run plain 'decispec gen' to reconcile)"
        );
        return Ok(Vec::new());
    }
    let removed = sweep_stale_artifacts(root, &files)?;
    println!(
        "done: {diagrams} diagram file(s) under .decispec/diagrams/, {tests} test artifact(s) under tests/generated/ (tests/glue/ untouched)"
    );
    Ok(removed)
}

/// Delete generated artifacts whose spec item disappeared: files under the
/// managed trees (`.decispec/diagrams/`, `tests/generated/<layer>/`) that
/// carry the GENERATED header but are not in the current render set. The
/// legacy `docs/diagrams/` tree is swept too, one-time GEN-008 migration:
/// its header-bearing files are removed, hand-edited files survive. Only
/// sweep directories gen manages; never create them. Returns the sorted
/// removed paths (project-relative, forward slashes).
fn sweep_stale_artifacts(
    root: &Path,
    files: &[decispec_codegen::GeneratedFile],
) -> CliResult<Vec<String>> {
    let rendered: std::collections::HashSet<&str> = files.iter().map(|f| f.path.as_str()).collect();
    let mut removed = Vec::new();
    // gen writes flat files under .decispec/diagrams/.
    sweep_dir(
        root,
        &root.join(".decispec/diagrams"),
        false,
        &rendered,
        &mut removed,
    )?;
    // GEN-008 migration: repos that committed the old tree get its generated
    // files removed; files without the header are never touched.
    sweep_dir(
        root,
        &root.join("docs/diagrams"),
        false,
        &rendered,
        &mut removed,
    )?;
    // tests/generated/<layer>/, recursively (unit/rust/); results/ is not a
    // layer dir and is never swept.
    for layer in Layer::ALL {
        let dir = root.join("tests/generated").join(layer.dir());
        sweep_dir(root, &dir, true, &rendered, &mut removed)?;
    }
    removed.sort();
    Ok(removed)
}

fn sweep_dir(
    root: &Path,
    dir: &Path,
    recursive: bool,
    rendered: &std::collections::HashSet<&str>,
    removed: &mut Vec<String>,
) -> CliResult<()> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(()); // absent: gen manages this tree but must not create it
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if recursive {
                sweep_dir(root, &path, true, rendered, removed)?;
            }
            continue;
        }
        if !path.is_file() {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if !decispec_codegen::is_managed_path(&rel) || rendered.contains(rel.as_str()) {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue; // unreadable / non-utf-8: never delete what we cannot inspect
        };
        if !decispec_codegen::has_generated_header(&rel, &content) {
            continue;
        }
        std::fs::remove_file(&path)?;
        removed.push(rel);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// test (adapters)
// ---------------------------------------------------------------------------

fn on_path(name: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| {
        let candidate = dir.join(name);
        candidate.is_file()
    })
}

fn dir_has_files_with_ext(dir: &Path, ext: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries
        .flatten()
        .any(|e| e.path().is_file() && e.path().extension().is_some_and(|x| x == ext))
}

fn tail(s: &str, n: usize) -> String {
    s.lines()
        .filter(|l| !l.trim().is_empty())
        .collect::<Vec<_>>()
        .iter()
        .rev()
        .take(n)
        .rev()
        .map(|l| l.trim())
        .collect::<Vec<_>>()
        .join(" | ")
}

struct CmdOutput {
    ok: bool,
    stdout: String,
    stderr: String,
}

/// Run a tool. A spawn failure (program missing) is NOT an error: it lands in
/// `ok: false` with the os error as stderr, so callers record a skipped or
/// failed adapter instead of aborting `decispec test`.
fn run_cmd(root: &Path, program: &str, args: &[String]) -> CmdOutput {
    let spawned = Command::new(program)
        .args(args)
        .current_dir(root)
        .output();
    match spawned {
        Ok(out) => CmdOutput {
            ok: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).to_string(),
            stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        },
        Err(e) => CmdOutput {
            ok: false,
            stdout: String::new(),
            stderr: format!("{program}: {e}"),
        },
    }
}

/// The command for one tool: a `[runners]` override wins over the built-in
/// default. The override string splits on whitespace: first token = program
/// (relative paths resolve against the project root — `run_cmd` sets cwd),
/// remaining tokens = fixed prefix args the layer-specific args append to.
fn runner_command(cfg: &Config, key: &str, default: &[&str]) -> (String, Vec<String>) {
    if let Some(cmd) = cfg.runners.get(key) {
        let mut tokens = cmd.split_whitespace();
        if let Some(program) = tokens.next() {
            return (
                program.to_string(),
                tokens.map(str::to_string).collect(),
            );
        }
    }
    (
        default[0].to_string(),
        default[1..].iter().map(|s| s.to_string()).collect(),
    )
}

/// Just the program name for on_path-style adapters (first override token,
/// or the built-in default).
fn runner_program(cfg: &Config, key: &str, default: &str) -> String {
    cfg.runners
        .get(key)
        .and_then(|cmd| cmd.split_whitespace().next().map(str::to_string))
        .unwrap_or_else(|| default.to_string())
}

/// `toolchain not installed (...)` reason naming the actual command tried.
fn not_installed(program: &str, prefix: &[String]) -> String {
    let mut cmd = vec![program.to_string()];
    cmd.extend(prefix.iter().cloned());
    format!("toolchain not installed ({})", cmd.join(" "))
}

fn skipped(name: &str, layer: GateLayer, reason: impl Into<String>) -> AdapterRecord {
    AdapterRecord {
        name: name.to_string(),
        layer,
        status: AdapterStatus::Skipped,
        reason: Some(reason.into()),
        result_files: Vec::new(),
    }
}

/// Run every adapter whose toolchain is installed and whose generated
/// artifacts exist. Skipped (not installed) is NOT a failure.
fn run_adapters(root: &Path, cfg: &Config) -> CliResult<Manifest> {
    let results_dir = root.join("tests/generated/results");
    std::fs::create_dir_all(&results_dir)?;
    let mut adapters = Vec::new();

    // unit / pytest
    let unit_dir = root.join("tests/generated/unit");
    if !dir_has_files_with_ext(&unit_dir, "py") {
        adapters.push(skipped(
            "pytest",
            GateLayer::Unit,
            "no generated unit tests",
        ));
    } else {
        let (program, prefix) = runner_command(cfg, "pytest", &["python3", "-m", "pytest"]);
        let probe = run_cmd(root, &program, &args_of(&prefix, &["--version"]));
        if !probe.ok {
            adapters.push(skipped(
                "pytest",
                GateLayer::Unit,
                not_installed(&program, &prefix),
            ));
        } else {
            let xml_rel = "tests/generated/results/pytest.xml";
            let out = run_cmd(
                root,
                &program,
                &args_of(
                    &prefix,
                    &["tests/generated/unit", "-q", "--junitxml", xml_rel],
                ),
            );
            let mut result_files = Vec::new();
            if root.join(xml_rel).is_file() {
                result_files.push("pytest.xml".to_string());
            }
            adapters.push(AdapterRecord {
                name: "pytest".to_string(),
                layer: GateLayer::Unit,
                status: if out.ok {
                    AdapterStatus::Ran
                } else {
                    AdapterStatus::Failed
                },
                reason: if out.ok {
                    None
                } else {
                    Some(format!("pytest exited non-zero: {}", tail(&out.stderr, 3)))
                },
                result_files,
            });
        }
    }

    // contract / gherkin (cucumber preferred, karate fallback)
    let contract_dir = root.join("tests/generated/contract");
    let cucumber = runner_program(cfg, "cucumber", "cucumber");
    let karate = runner_program(cfg, "karate", "karate");
    if !dir_has_files_with_ext(&contract_dir, "feature") {
        adapters.push(skipped(
            "cucumber",
            GateLayer::Contract,
            "no generated contract tests",
        ));
    } else if on_path(&cucumber) {
        let xml_rel = "tests/generated/results/contract.xml";
        let out = run_cmd(
            root,
            &cucumber,
            &args_of(&[], &["tests/generated/contract", "--format", "junit", "--out", xml_rel]),
        );
        let mut result_files = Vec::new();
        if root.join(xml_rel).is_file() {
            result_files.push("contract.xml".to_string());
        }
        adapters.push(AdapterRecord {
            name: "cucumber".to_string(),
            layer: GateLayer::Contract,
            status: if out.ok {
                AdapterStatus::Ran
            } else {
                AdapterStatus::Failed
            },
            reason: if out.ok {
                None
            } else {
                Some(format!(
                    "cucumber exited non-zero: {}",
                    tail(&out.stderr, 3)
                ))
            },
            result_files,
        });
    } else if on_path(&karate) {
        let out = run_cmd(root, &karate, &["tests/generated/contract".to_string()]);
        let mut result_files = Vec::new();
        let xml_rel = "tests/generated/results/contract.xml";
        if out.ok && !out.stdout.trim().is_empty() {
            std::fs::write(root.join(xml_rel), &out.stdout)?;
            result_files.push("contract.xml".to_string());
        }
        adapters.push(AdapterRecord {
            name: "karate".to_string(),
            layer: GateLayer::Contract,
            status: if out.ok {
                AdapterStatus::Ran
            } else {
                AdapterStatus::Failed
            },
            reason: if out.ok {
                None
            } else {
                Some(format!(
                    "karate exited non-zero: {}",
                    tail(&out.stderr, 3)
                ))
            },
            result_files,
        });
    } else {
        adapters.push(skipped(
            "cucumber",
            GateLayer::Contract,
            format!("toolchain not installed ({cucumber} or {karate})"),
        ));
    }

    // e2e / playwright
    let e2e_dir = root.join("tests/generated/e2e");
    if !dir_has_files_with_ext(&e2e_dir, "ts") {
        adapters.push(skipped(
            "playwright",
            GateLayer::E2e,
            "no generated e2e tests",
        ));
    } else {
        let (program, prefix) = runner_command(
            cfg,
            "playwright",
            &["npx", "--no-install", "playwright"],
        );
        let probe = run_cmd(root, &program, &args_of(&prefix, &["--version"]));
        if !probe.ok {
            adapters.push(skipped(
                "playwright",
                GateLayer::E2e,
                not_installed(&program, &prefix),
            ));
        } else {
            let out = run_cmd(
                root,
                &program,
                &args_of(
                    &prefix,
                    &[
                        "test",
                        "tests/generated/e2e",
                        "--reporter=junit",
                    ],
                ),
            );
            // junit reporter prints XML to stdout; newer versions write results.xml.
            let xml_rel = "tests/generated/results/playwright.xml";
            let mut result_files = Vec::new();
            if !out.stdout.contains("<testsuite") && root.join("results.xml").is_file() {
                std::fs::rename(root.join("results.xml"), root.join(xml_rel))?;
            }
            if root.join(xml_rel).is_file() {
                result_files.push("playwright.xml".to_string());
            } else if out.stdout.contains("<testsuite") {
                std::fs::write(root.join(xml_rel), &out.stdout)?;
                result_files.push("playwright.xml".to_string());
            }
            adapters.push(AdapterRecord {
                name: "playwright".to_string(),
                layer: GateLayer::E2e,
                status: if out.ok {
                    AdapterStatus::Ran
                } else {
                    AdapterStatus::Failed
                },
                reason: if out.ok {
                    None
                } else {
                    Some(format!(
                        "playwright exited non-zero: {}",
                        tail(&out.stderr, 3)
                    ))
                },
                result_files,
            });
        }
    }

    // infra / conftest (OPA-style)
    let infra_dir = root.join("tests/generated/infra");
    let conftest = runner_program(cfg, "conftest", "conftest");
    if !dir_has_files_with_ext(&infra_dir, "rego") {
        adapters.push(skipped(
            "conftest",
            GateLayer::Infra,
            "no generated infra policies",
        ));
    } else if on_path(&conftest) {
        let out = run_cmd(root, &conftest, &["test".to_string(), "tests/generated/infra".to_string()]);
        adapters.push(AdapterRecord {
            name: "conftest".to_string(),
            layer: GateLayer::Infra,
            status: if out.ok {
                AdapterStatus::Ran
            } else {
                AdapterStatus::Failed
            },
            reason: if out.ok {
                None
            } else {
                Some(format!(
                    "conftest violations or error: {}",
                    tail(&(out.stdout.clone() + &out.stderr), 3)
                ))
            },
            result_files: Vec::new(),
        });
    } else {
        adapters.push(skipped(
            "conftest",
            GateLayer::Infra,
            format!("toolchain not installed ({conftest})"),
        ));
    }

    // fitness / import-linter + dependency-cruiser
    let fitness_dir = root.join("tests/generated/fitness");
    let has_fitness = fitness_dir.join("dependency-cruiser.cjs").is_file();
    let lint_imports = runner_program(cfg, "import-linter", "lint-imports");
    let depcruise = runner_program(cfg, "dependency-cruiser", "depcruise");
    if !has_fitness {
        adapters.push(skipped(
            "import-linter",
            GateLayer::Fitness,
            "no generated fitness artifacts",
        ));
        adapters.push(skipped(
            "dependency-cruiser",
            GateLayer::Fitness,
            "no generated fitness artifacts",
        ));
    } else {
        if on_path(&lint_imports) {
            let out = run_cmd(
                root,
                &lint_imports,
                &[
                    "--config".to_string(),
                    "tests/generated/fitness/.importlinter".to_string(),
                ],
            );
            adapters.push(AdapterRecord {
                name: "import-linter".to_string(),
                layer: GateLayer::Fitness,
                status: if out.ok {
                    AdapterStatus::Ran
                } else {
                    AdapterStatus::Failed
                },
                reason: if out.ok {
                    None
                } else {
                    Some(format!(
                        "import-linter contract broken: {}",
                        tail(&(out.stdout.clone() + &out.stderr), 3)
                    ))
                },
                result_files: Vec::new(),
            });
        } else {
            adapters.push(skipped(
                "import-linter",
                GateLayer::Fitness,
                format!("toolchain not installed ({lint_imports})"),
            ));
        }
        if on_path(&depcruise) {
            if root.join("src").is_dir() {
                let out = run_cmd(
                    root,
                    &depcruise,
                    &[
                        "--config".to_string(),
                        "tests/generated/fitness/dependency-cruiser.cjs".to_string(),
                        "src".to_string(),
                    ],
                );
                adapters.push(AdapterRecord {
                    name: "dependency-cruiser".to_string(),
                    layer: GateLayer::Fitness,
                    status: if out.ok {
                        AdapterStatus::Ran
                    } else {
                        AdapterStatus::Failed
                    },
                    reason: if out.ok {
                        None
                    } else {
                        Some(format!(
                            "dependency-cruiser violations: {}",
                            tail(&(out.stdout.clone() + &out.stderr), 3)
                        ))
                    },
                    result_files: Vec::new(),
                });
            } else {
                adapters.push(skipped(
                    "dependency-cruiser",
                    GateLayer::Fitness,
                    "no src directory to analyse",
                ));
            }
        } else {
            adapters.push(skipped(
                "dependency-cruiser",
                GateLayer::Fitness,
                format!("toolchain not installed ({depcruise})"),
            ));
        }
    }

    Ok(Manifest { adapters })
}

/// `prefix` (fixed runner args) followed by the layer-specific `rest`.
fn args_of(prefix: &[String], rest: &[&str]) -> Vec<String> {
    prefix
        .iter()
        .cloned()
        .chain(rest.iter().map(|s| s.to_string()))
        .collect()
}

fn cmd_test(root: &Path) -> CliResult<ExitCode> {
    let cfg = load_config(root)?;
    let manifest = run_adapters(root, &cfg)?;
    std::fs::write(
        root.join("tests/generated/results/manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;
    let mut any_failed = false;
    for a in &manifest.adapters {
        match a.status {
            AdapterStatus::Ran => println!("{} [{}]: ran", a.name, a.layer.as_str()),
            AdapterStatus::Skipped => println!(
                "{} [{}]: skipped ({})",
                a.name,
                a.layer.as_str(),
                a.reason.as_deref().unwrap_or("no reason")
            ),
            AdapterStatus::Failed => {
                any_failed = true;
                println!(
                    "{} [{}]: FAILED ({})",
                    a.name,
                    a.layer.as_str(),
                    a.reason.as_deref().unwrap_or("no reason")
                );
            }
        }
    }
    println!("manifest: tests/generated/results/manifest.json");
    Ok(if any_failed {
        ExitCode::from(EXIT_FINDINGS)
    } else {
        ExitCode::from(EXIT_OK)
    })
}

// ---------------------------------------------------------------------------
// gate
// ---------------------------------------------------------------------------

/// Parse the console-filter flags into a gate Filter. Unknown statuses are
/// bad CLI usage (exit 2 via the error path), never a findings code.
fn parse_status(s: &str) -> CliResult<Status> {
    Ok(match s {
        "accepted" => Status::Accepted,
        "proposed" => Status::Proposed,
        "rejected" => Status::Rejected,
        "superseded" => Status::Superseded,
        other => {
            return Err(CliError::Msg(format!(
                "unknown status '{other}', expected one of: accepted, proposed, rejected, superseded"
            )));
        }
    })
}

fn parse_filter(
    prefix: Option<&str>,
    status: Option<&str>,
    only_failing: bool,
    only_uncovered: bool,
) -> CliResult<Filter> {
    let prefixes = prefix.map(split_prefixes).unwrap_or_default();
    let status = match status {
        None => None,
        Some(s) => Some(parse_status(s)?),
    };
    Ok(Filter {
        prefixes,
        status,
        only_failing,
        only_uncovered,
    })
}

fn print_matrix(report: &decispec_gate::GateReport, filter: &Filter) {
    println!("═══ DECISIONSPEC GATE ═══ project: {}", report.project);
    if !filter.is_empty() {
        println!("(console filter active — verdict and report.json stay full)");
    }
    // The printed matrix is the filtered view; verdict, counts, failures and
    // warnings below always come from the full report.
    let view = decispec_gate::filtered_report(report, filter);
    let rows: Vec<&decispec_gate::MatrixRow> = view
        .decisions
        .iter()
        .flat_map(|d| d.rows.iter())
        .collect();
    if rows.is_empty() {
        if filter.is_empty() {
            println!("(no traceability rows)");
        } else {
            println!("(no rows match the filter)");
        }
    } else {
        let mut w = [8usize, 12, 20, 8, 11];
        for r in &rows {
            w[0] = w[0].max(r.decision.len());
            w[1] = w[1].max(r.requirement.len());
            w[2] = w[2].max(r.item.len());
        }
        println!(
            "{:<w0$}  {:<w1$}  {:<w2$}  {:<w3$}  {:<w4$}  artifacts",
            "decision",
            "requirement",
            "item",
            "layer",
            "result",
            w0 = w[0],
            w1 = w[1],
            w2 = w[2],
            w3 = w[3],
            w4 = w[4]
        );
        for r in &rows {
            println!(
                "{:<w0$}  {:<w1$}  {:<w2$}  {:<w3$}  {:<w4$}  {}",
                r.decision,
                r.requirement,
                r.item,
                r.layer.as_str(),
                r.result.as_str(),
                r.artifacts.join(", "),
                w0 = w[0],
                w1 = w[1],
                w2 = w[2],
                w3 = w[3],
                w4 = w[4]
            );
        }
    }
    for d in &view.decisions {
        if d.rows.is_empty() {
            println!("decision {} ({}) -> outcome: {}", d.id, d.status, d.outcome);
        }
    }
    if !report.failures.is_empty() {
        println!("\nfailures:");
        for f in &report.failures {
            println!("  ✗ {f}");
        }
    }
    if !report.warnings.is_empty() {
        println!("\nwarnings:");
        for w in &report.warnings {
            println!("  ! {w}");
        }
    }
    let c = &report.counts;
    println!(
        "\nverdict: {} ({} rows: {} passed, {} failed, {} uncovered, {} skipped, {} no-artifacts)",
        if report.verdict == Verdict::Pass {
            "PASS"
        } else {
            "FAIL"
        },
        c.rows,
        c.passed,
        c.failed,
        c.uncovered,
        c.skipped,
        c.no_artifacts
    );
    println!("{}", coverage_line(&report.coverage()));
}

fn cmd_gate(
    root: &Path,
    json: bool,
    results: Option<PathBuf>,
    strict_skipped: bool,
    allow_skips: Option<PathBuf>,
    filter: &decispec_gate::Filter,
) -> CliResult<ExitCode> {
    let cfg = load_config(root)?;
    let ws = load_workspace_or_bail(root)?;

    let allowances = match &allow_skips {
        Some(path) => {
            let full = if path.is_absolute() {
                path.clone()
            } else {
                root.join(path)
            };
            let src = std::fs::read_to_string(&full).map_err(|e| {
                CliError::Msg(format!("cannot read allowlist {}: {e}", full.display()))
            })?;
            decispec_gate::parse_allowlist(&src)
                .map_err(|e| CliError::Msg(format!("{}: {e}", full.display())))?
        }
        None => Vec::new(),
    };

    let results_dir = match results {
        Some(dir) => {
            let dir = if dir.is_absolute() {
                dir
            } else {
                root.join(dir)
            };
            if !dir.is_dir() {
                return Err(CliError::Msg(format!(
                    "results dir {} does not exist",
                    dir.display()
                )));
            }
            dir
        }
        None => {
            let manifest = run_adapters(root, &cfg)?;
            let dir = root.join("tests/generated/results");
            std::fs::write(
                dir.join("manifest.json"),
                serde_json::to_string_pretty(&manifest)?,
            )?;
            dir
        }
    };

    let report = decispec_gate::evaluate(&GateInput {
        ws: &ws,
        generated_dir: &root.join("tests/generated"),
        results_dir: &results_dir,
        project: cfg.project.clone(),
        strict_skipped,
        allow_skips: allowances,
    })?;

    // Always persist the report + record the run.
    std::fs::create_dir_all(root.join(".decispec"))?;
    let report_json = serde_json::to_string_pretty(&report)?;
    std::fs::write(root.join(".decispec/report.json"), &report_json)?;
    let store = Store::open(&root.join(".decispec/decispec.db"))?;
    store.record_run(&RunRecord {
        ts: decispec_store::now_utc(),
        verdict: serde_json::to_string(&report.verdict)?
            .trim_matches('"')
            .to_string(),
        total_rows: report.counts.rows,
        passed: report.counts.passed,
        failed: report.counts.failed,
        uncovered: report.counts.uncovered,
        skipped: report.counts.skipped,
        report_json: report_json.clone(),
        decisions: report
            .decisions
            .iter()
            .map(|d| DecisionRun {
                decision_id: d.id.clone(),
                outcome: d.outcome.clone(),
            })
            .collect(),
    })?;

    print_matrix(&report, filter);
    println!(".decispec/report.json written; run recorded in .decispec/decispec.db");
    if json {
        // Filters narrow the printed view only; the on-disk report above is
        // always the full one.
        if filter.is_empty() {
            println!("{}", report_json);
        } else {
            println!(
                "{}",
                serde_json::to_string_pretty(&decispec_gate::filtered_report(&report, filter))?
            );
        }
    }
    Ok(if report.verdict == Verdict::Pass {
        ExitCode::from(EXIT_OK)
    } else {
        ExitCode::from(EXIT_FINDINGS)
    })
}

// ---------------------------------------------------------------------------
// coverage (PROOF-001..005): the proof rollup over the last gate report
// ---------------------------------------------------------------------------

/// Extract the rollup from report.json content. Pure — the command wrapper
/// owns the file read so this stays testable without a project. Errors mean
/// the report is unreadable, so the command cannot complete (exit 2).
fn coverage_from_report(
    src: &str,
) -> CliResult<(String, decispec_gate::coverage::Coverage)> {
    let value: serde_json::Value = serde_json::from_str(src)?;
    let project = value["project"].as_str().unwrap_or("?").to_string();
    let empty = Vec::new();
    let decisions = value["decisions"].as_array().unwrap_or(&empty);
    let mut owned: Vec<(String, Status, usize, usize)> = Vec::new();
    for d in decisions {
        let id = d["id"]
            .as_str()
            .ok_or_else(|| CliError::Msg("report decision is missing its id".into()))?
            .to_string();
        let status_s = d["status"].as_str().ok_or_else(|| {
            CliError::Msg(format!("report decision {id} is missing its status"))
        })?;
        let status = Status::parse(status_s).ok_or_else(|| {
            CliError::Msg(format!("report decision {id} has unknown status {status_s:?}"))
        })?;
        let rows = d["rows"].as_array().map(Vec::as_slice).unwrap_or(&[]);
        let rows_total = rows.len();
        // Only "pass" is proof; anything else (fail/uncovered/skipped/
        // no-artifacts, or a result absent from an older report) is not.
        let rows_passed = rows
            .iter()
            .filter(|r| r["result"].as_str() == Some("pass"))
            .count();
        owned.push((id, status, rows_total, rows_passed));
    }
    let evidence = owned
        .iter()
        .map(|(id, status, rows_total, rows_passed)| {
            decispec_gate::coverage::DecisionEvidence {
                id,
                status: *status,
                rows_total: *rows_total,
                rows_passed: *rows_passed,
            }
        })
        .collect::<Vec<_>>();
    Ok((project, decispec_gate::coverage::rollup(&evidence)))
}

/// The shared headline line: `coverage: N/M decisions proven (P%)`. Printed
/// by `decispec coverage` and by the gate summary, so both surfaces carry
/// byte-identical arithmetic (PROOF-005).
fn coverage_line(cov: &decispec_gate::coverage::Coverage) -> String {
    format!(
        "coverage: {}/{} decisions proven ({}%)",
        cov.proven, cov.gatable, cov.percent
    )
}

/// Full console rendering: header naming the run the numbers reflect, one
/// marked line per gatable decision, then the headline line.
fn render_coverage(
    project: &str,
    cov: &decispec_gate::coverage::Coverage,
    run_label: &str,
) -> String {
    let mut out = format!(
        "═══ DECISIONSPEC COVERAGE ═══ project: {project} (from gate run {run_label})\n"
    );
    for d in &cov.decisions {
        out.push_str(&format!("{} {} {}\n", d.state.symbol(), d.id, d.state.as_str()));
    }
    out.push_str(&coverage_line(cov));
    out.push('\n');
    out
}

/// `secs` since the Unix epoch as a UTC calendar stamp — no chrono on purpose
/// (dependencies stay minimal); the civil-from-days conversion is the
/// standard Howard Hinnant algorithm.
fn format_utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let hh = rem / 3_600;
    let mi = (rem % 3_600) / 60;
    let ss = rem % 60;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y0 = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let dd = doy - (153 * mp + 2) / 5 + 1;
    let mm = if mp < 10 { mp + 3 } else { mp - 9 };
    let yy = if mm <= 2 { y0 + 1 } else { y0 };
    format!("{yy:04}-{mm:02}-{dd:02} {hh:02}:{mi:02}:{ss:02} UTC")
}

/// mtime of the report file as a UTC stamp — the "which run is this" half of
/// PROOF-001-R1 (report.json itself carries no timestamp).
fn report_mtime_utc(path: &Path) -> CliResult<String> {
    let mtime = std::fs::metadata(path)?.modified()?;
    let secs = mtime
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| CliError::Msg("report mtime predates the unix epoch".into()))?
        .as_secs();
    Ok(format_utc(secs))
}

fn cmd_coverage(root: &Path, min: Option<u64>) -> CliResult<ExitCode> {
    let path = root.join(".decispec/report.json");
    let src = std::fs::read_to_string(&path).map_err(|_| {
        CliError::Msg("no gate report at .decispec/report.json — run `decispec gate` first".into())
    })?;
    let run_label = report_mtime_utc(&path)?;
    let (project, cov) = coverage_from_report(&src)?;
    print!("{}", render_coverage(&project, &cov, &run_label));
    if let Some(floor) = min {
        if cov.percent as u64 >= floor {
            return Ok(ExitCode::from(EXIT_OK));
        }
        println!(
            "coverage: {}% is below --min {}%",
            cov.percent, floor
        );
        return Ok(ExitCode::from(EXIT_FINDINGS));
    }
    Ok(ExitCode::from(EXIT_OK))
}

// ---------------------------------------------------------------------------
// query
// ---------------------------------------------------------------------------

/// Glob matching for `decispec query`: `*` matches any run of characters
/// (including empty), anchored at both ends; multiple `*` allowed. A pattern
/// without `*` is plain exact equality — the pre-F40 behavior.
fn id_glob_match(pattern: &str, id: &str) -> bool {
    if !pattern.contains('*') {
        return pattern == id;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    let mut rest = id;
    for (i, seg) in parts.iter().enumerate() {
        let first = i == 0;
        let last = i + 1 == parts.len();
        if first && last {
            return rest == *seg;
        }
        if first {
            if let Some(tail) = rest.strip_prefix(seg) {
                rest = tail;
            } else {
                return false;
            }
        } else if last {
            if !rest.ends_with(seg) {
                return false;
            }
        } else if let Some(pos) = rest.find(seg) {
            rest = &rest[pos + seg.len()..];
        } else {
            return false;
        }
    }
    true
}

/// Decisions whose id matches `pattern`, optionally narrowed to `status`,
/// sorted by id for a deterministic listing.
fn matching_decisions<'a>(
    ws: &'a Workspace,
    pattern: &str,
    status: Option<Status>,
) -> Vec<&'a decispec_ir::Decision> {
    let mut hits: Vec<&decispec_ir::Decision> = ws
        .decisions
        .iter()
        .filter(|d| id_glob_match(pattern, &d.id) && status.is_none_or(|s| d.status == s))
        .collect();
    hits.sort_by(|a, b| a.id.cmp(&b.id));
    hits
}

fn cmd_query(root: &Path, id: &str, status: Option<&Status>) -> CliResult<ExitCode> {
    let ws = match load_workspace(root) {
        Ok(ws) => ws,
        Err(errors) => {
            print_errors(&errors);
            return Ok(ExitCode::from(EXIT_FINDINGS));
        }
    };

    // Pattern mode: a `*` in the argument lists matching decisions compactly.
    if id.contains('*') {
        let hits = matching_decisions(&ws, id, status.copied());
        if hits.is_empty() {
            println!("no decision matches pattern '{id}'");
            return Ok(ExitCode::from(EXIT_FINDINGS));
        }
        for d in hits {
            println!("{} ({}) {}", d.id, d.status.as_str(), d.title);
        }
        return Ok(ExitCode::from(EXIT_OK));
    }

    // Resolve requirement/scenario/invariant/policy IDs to their decision.
    let decision_id = if ws.decision(id).is_some() {
        id.to_string()
    } else {
        let mut found: Option<String> = None;
        for sp in &ws.specs {
            let hit = sp.requirements.iter().any(|r| r.id == id)
                || sp.scenarios.iter().any(|s| s.id == id)
                || sp.invariants.iter().any(|i| i.id == id)
                || sp.infra_policies.iter().any(|p| p.id == id);
            if hit {
                found = Some(sp.id.clone());
                break;
            }
        }
        match found {
            Some(d) => d,
            None => {
                println!("no decision, requirement, scenario, invariant or policy named '{id}'");
                return Ok(ExitCode::from(EXIT_FINDINGS));
            }
        }
    };

    let decision = ws.decision(&decision_id).expect("checked above");
    if status.is_some_and(|want| decision.status != *want) {
        println!("no decision matches pattern '{id}'");
        return Ok(ExitCode::from(EXIT_FINDINGS));
    }
    println!(
        "{} ({}) {}",
        decision.id,
        decision.status.as_str(),
        decision.title
    );
    if let Some(ctx) = &decision.context {
        println!("  context: {ctx}");
    }
    if let Some(cons) = &decision.consequences {
        println!("  consequences: {cons}");
    }
    if let Some(by) = &decision.superseded_by {
        println!("  superseded_by: {by}");
    }

    let spec = ws.spec_for_decision(&decision_id);
    if let Some(sp) = spec {
        println!("  spec: {}:{}", sp.src.file, sp.src.line);
    }

    // Model: flows from the spec's file, or all flows if none are co-located.
    let mut flows: Vec<&decispec_ir::Flow> = spec
        .map(|sp| ws.flows_in_file(&sp.src.file))
        .unwrap_or_default();
    if flows.is_empty() {
        flows = ws.all_flows().collect();
    }
    if !flows.is_empty() {
        println!("  flows:");
        for f in flows {
            println!(
                "    {} -> .decispec/diagrams/{}.mmd, .decispec/diagrams/{}.puml",
                f.id, f.id, f.id
            );
        }
    }

    if let Some(sp) = spec {
        if !sp.requirements.is_empty() {
            println!("  requirements:");
        }
        for req in &sp.requirements {
            let style = req
                .style
                .as_ref()
                .map(|s| format!(" ({s})"))
                .unwrap_or_default();
            let layers: Vec<&str> = req.layers.iter().map(|l| l.as_str()).collect();
            println!("    {}{} [{}]", req.id, style, layers.join(", "));
            println!("      \"{}\"", req.text);
            for scen_id in &req.scenarios {
                let scen = match sp.scenario(scen_id) {
                    Some(s) => s,
                    None => continue,
                };
                let scen_layers = sp.layers_for_scenario(scen_id);
                let sl: Vec<&str> = scen_layers.iter().map(|l| l.as_str()).collect();
                println!("      scenario {} [{}]", scen.id, sl.join(", "));
                println!(
                    "        given {} | when {} | then {}",
                    scen.given, scen.when, scen.then
                );
                for layer in &scen_layers {
                    for art in scenario_artifacts(&sp.id, &scen.id, *layer) {
                        println!("        [{}] {}", layer.as_str(), art);
                    }
                }
            }
        }
        for inv in &sp.invariants {
            let layers: Vec<&str> = inv.layers.iter().map(|l| l.as_str()).collect();
            println!(
                "  invariant {} [{}] {}",
                inv.id,
                layers.join(", "),
                inv.expr
            );
            for layer in &inv.layers {
                if *layer == Layer::Unit {
                    println!(
                        "    [unit] tests/generated/unit/{}",
                        decispec_ir::names::invariant_test_file(&sp.id, &inv.id)
                    );
                }
            }
        }
        for policy in &sp.infra_policies {
            let layers: Vec<&str> = policy.layers.iter().map(|l| l.as_str()).collect();
            println!(
                "  infra_policy {} [{}] {}",
                policy.id,
                layers.join(", "),
                policy.description
            );
            if policy.layers.contains(&Layer::Infra) {
                println!(
                    "    [infra] tests/generated/infra/{}",
                    decispec_ir::names::infra_policy_file(&policy.id)
                );
            }
        }
    }

    let store = Store::open(&root.join(".decispec/decispec.db"))?;
    match store.latest_decision_outcome(&decision_id)? {
        Some((verdict, ts)) => println!("  last gate: {verdict} ({ts})"),
        None => println!("  last gate: no recorded run"),
    }
    Ok(ExitCode::from(EXIT_OK))
}

fn scenario_artifacts(spec_id: &str, scenario_id: &str, layer: Layer) -> Vec<String> {
    match layer {
        Layer::Unit => vec![format!(
            "tests/generated/unit/{}",
            decispec_ir::names::unit_test_file(spec_id, scenario_id)
        )],
        Layer::Contract => vec![format!(
            "tests/generated/contract/{}",
            decispec_ir::names::contract_feature_file(spec_id)
        )],
        Layer::E2e => vec![format!(
            "tests/generated/e2e/{}",
            decispec_ir::names::e2e_spec_file(scenario_id)
        )],
        _ => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// extract
// ---------------------------------------------------------------------------

fn cmd_extract(root: &Path, stdout: bool, to: Option<PathBuf>) -> CliResult<ExitCode> {
    // Checked before any analysis so the conflict writes and prints nothing
    // (clap also rejects the pair on the CLI surface; this pins the contract
    // at the command seam, PORT-001-R2).
    if stdout && to.is_some() {
        return Err(CliError::Msg(
            "--to and --stdout are mutually exclusive — pick a store directory or stdout, not both"
                .to_string(),
        ));
    }
    // DecisionSpec's own scaffold must not shape the draft: `init` writes the
    // glue package (default tests/glue) and `gen` writes tests/generated, so
    // a project that ran the quickstart would otherwise draft its harness
    // instead of its code (the glue .py files can even flip the detected
    // language on a tie). A pre-DecisionSpec project has no decispec.toml:
    // fall back to the default glue location there.
    let cfg = if root.join("decispec.toml").is_file() {
        load_config(root)?
    } else {
        Config::default()
    };
    let mut exclude = vec![PathBuf::from("tests/generated")];
    let glue_dir = cfg.glue_module.replace('.', "/");
    if !glue_dir.is_empty() {
        exclude.push(PathBuf::from(glue_dir));
    }
    let extracted = decispec_extract::analyze(root, &exclude)?;
    for w in &extracted.warnings {
        eprintln!("warning: {w}");
    }
    let spec_text = decispec_extract::render_spec(&extracted);
    let fitness = decispec_extract::render_fitness_toml(&extracted);

    if stdout {
        print!("{spec_text}");
        eprintln!("{fitness}");
        return Ok(ExitCode::from(EXIT_OK));
    }

    // PORT-001: an external store directory takes the draft under the
    // analyzed project's directory name — the store, not the analyzed repo,
    // owns the file.
    if let Some(dir) = &to {
        let dirname = root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "extracted".to_string());
        std::fs::create_dir_all(dir)?;
        let target = dir.join(format!("{dirname}.spec"));
        if target.exists() {
            return Err(CliError::Msg(format!(
                "{} already exists — the store file is never overwritten",
                target.display()
            )));
        }
        std::fs::write(&target, &spec_text)?;
        println!("wrote {}", target.display());
        println!("review the draft (decisions from ADRs + structure from code), then add requirements");
        println!("{fitness}");
        return Ok(ExitCode::from(EXIT_OK));
    }

    let specs_dir = root.join("specs");
    std::fs::create_dir_all(&specs_dir)?;
    let target = specs_dir.join("extracted.spec");
    if target.exists() {
        return Err(CliError::Msg(format!(
            "{} already exists — delete it or re-run with --stdout",
            target.display()
        )));
    }
    std::fs::write(&target, &spec_text)?;
    println!("wrote {}", target.display());
    println!("review the draft (decisions from ADRs + structure from code), then add requirements");
    println!("{fitness}");
    Ok(ExitCode::from(EXIT_OK))
}

// ---------------------------------------------------------------------------
// mcp
// ---------------------------------------------------------------------------

/// Run the MCP server over stdio until EOF. Blocking by design — this
/// command IS the agent connection.
fn cmd_mcp() -> CliResult<ExitCode> {
    decispec_mcp::serve()?;
    Ok(ExitCode::from(EXIT_OK))
}

// ---------------------------------------------------------------------------
// index
// ---------------------------------------------------------------------------

fn cmd_index(root: &Path, out: Option<PathBuf>) -> CliResult<ExitCode> {
    let ws = load_workspace_or_bail(root)?;
    let mut file = decispec_codegen::render_index(&ws);
    if let Some(out) = out {
        file.path = out.to_string_lossy().replace('\\', "/");
    }
    let path = root.join(&file.path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, &file.content)?;
    println!("wrote {}", file.path);
    Ok(ExitCode::from(EXIT_OK))
}

// ---------------------------------------------------------------------------
// tui — read-only spec cockpit (decispec-tui holds the state + widgets; this
// module owns the registry file, workspace loading, and the terminal)
// ---------------------------------------------------------------------------

/// `~/.config/decispec/registry.toml`, honoring `$XDG_CONFIG_HOME`.
fn registry_path() -> CliResult<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME")
        && !xdg.is_empty()
    {
        return Ok(PathBuf::from(xdg).join("decispec").join("registry.toml"));
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| CliError::Msg("cannot determine home directory (HOME unset)".to_string()))?;
    Ok(home.join(".config").join("decispec").join("registry.toml"))
}

/// First-run seeds: well-known entries, suggested only when the directory
/// actually exists on this machine (AGENT_TEAMS.md F31: tvgo + verydian).
fn default_seed_entries(home: &Path) -> Vec<decispec_tui::registry::ProjectEntry> {
    [("tvgo", "Projects/FPF/tvgo"), ("verydian", "Projects/FPF/verydian-app")]
        .iter()
        .filter_map(|(name, rel)| {
            let path = home.join(rel);
            path.is_dir().then(|| decispec_tui::registry::ProjectEntry {
                name: name.to_string(),
                path: path.to_string_lossy().to_string(),
            })
        })
        .collect()
}

/// Read the registry. A missing file means "no projects yet", not an error.
fn read_registry(path: &Path) -> CliResult<decispec_tui::registry::Registry> {
    match std::fs::read_to_string(path) {
        Ok(src) => decispec_tui::registry::Registry::parse(&src).map_err(CliError::Msg),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Ok(decispec_tui::registry::Registry::default())
        }
        Err(e) => Err(e.into()),
    }
}

/// Registry for an interactive session: on first run, persist the given
/// seeds (well-known projects that exist on this machine) so the file is
/// inspectable/editable afterwards.
fn load_or_seed_registry(
    path: &Path,
    seeds: Vec<decispec_tui::registry::ProjectEntry>,
) -> CliResult<decispec_tui::registry::Registry> {
    match std::fs::read_to_string(path) {
        Ok(src) => decispec_tui::registry::Registry::parse(&src).map_err(CliError::Msg),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let reg = decispec_tui::registry::Registry { projects: seeds };
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, reg.to_toml())?;
            Ok(reg)
        }
        Err(e) => Err(e.into()),
    }
}

fn seeds_for_this_machine() -> Vec<decispec_tui::registry::ProjectEntry> {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    default_seed_entries(&home)
}

fn save_registry(path: &Path, reg: &decispec_tui::registry::Registry) -> CliResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, reg.to_toml())?;
    Ok(())
}

/// Register a project: canonicalize the path, take its display name from
/// `[project].name`, upsert into the registry.
fn register_project(
    reg_path: &Path,
    dir: &Path,
) -> CliResult<decispec_tui::registry::ProjectEntry> {
    let root = dir
        .canonicalize()
        .map_err(|e| CliError::Msg(format!("{}: {e}", dir.display())))?;
    if !root.join("decispec.toml").is_file() {
        return Err(CliError::Msg(format!(
            "{} has no decispec.toml — not a DecisionSpec project",
            root.display()
        )));
    }
    let cfg = load_config(&root)?;
    let entry = decispec_tui::registry::ProjectEntry {
        name: cfg.project,
        path: root.to_string_lossy().to_string(),
    };
    let mut reg = read_registry(reg_path)?;
    reg.upsert(entry.clone());
    save_registry(reg_path, &reg)?;
    Ok(entry)
}

/// `~/...` paths in the registry expand against `$HOME`.
fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(path)
}

/// Load one registered project for the cockpit: workspace (or its `file:line`
/// diagnostics), spec sources (kept even when the workspace is broken — that
/// is exactly when the editor is needed), codegen settings, last gate report.
/// Never fails as a whole — a broken project becomes an error-state card.
fn load_tui_project(entry: &decispec_tui::registry::ProjectEntry) -> ProjectData {
    let root = expand_home(&entry.path);
    if !root.join("decispec.toml").is_file() {
        let message = format!("{} has no decispec.toml", root.display());
        return ProjectData::error(entry.name.clone(), root, message, Vec::new());
    }
    let sources = read_spec_sources(&root);
    match load_workspace(&root) {
        Ok(ws) => {
            let cfg = load_config(&root).unwrap_or_default();
            let codegen = CodegenConfig {
                lang: cfg.lang,
                glue_module: cfg.glue_module,
                rust_glue: cfg.rust_glue,
                root_package: cfg.root_package,
                container_packages: cfg.container_packages,
            };
            // A missing/corrupt report just means "no gate yet".
            let gate = std::fs::read_to_string(root.join(".decispec/report.json"))
                .ok()
                .and_then(|src| decispec_tui::snapshot::GateSnapshot::parse(&src).ok());
            ProjectData::loaded(entry.name.clone(), root, ws, sources, codegen, gate)
        }
        Err(errors) => ProjectData::error(
            entry.name.clone(),
            root,
            errors.join("; "),
            sources,
        ),
    }
}

/// Every spec file's project-relative path and text, best effort: unreadable
/// files are skipped (their parse error already surfaced via load_workspace).
fn read_spec_sources(root: &Path) -> Vec<(String, String)> {
    let Ok(files) = spec_files(root) else {
        return Vec::new();
    };
    files
        .iter()
        .filter_map(|f| {
            let rel = f
                .strip_prefix(root)
                .unwrap_or(f)
                .to_string_lossy()
                .replace('\\', "/");
            std::fs::read_to_string(f).ok().map(|src| (rel, src))
        })
        .collect()
}

fn cmd_tui(register: Option<PathBuf>) -> CliResult<ExitCode> {
    let reg_path = registry_path()?;
    if let Some(dir) = register {
        let entry = register_project(&reg_path, &dir)?;
        println!("registered {} -> {}", entry.name, entry.path);
        println!("registry: {}", reg_path.display());
        return Ok(ExitCode::from(EXIT_OK));
    }
    let registry = load_or_seed_registry(&reg_path, seeds_for_this_machine())?;
    let projects: Vec<ProjectData> = registry.projects.iter().map(load_tui_project).collect();
    let mut app = App::new(projects);
    run_tui_loop(&mut app, &reg_path)?;
    Ok(ExitCode::from(EXIT_OK))
}

/// Crossterm owns the terminal; `decispec-tui` stays pure. The terminal is
/// restored on every exit path before the error (if any) propagates.
/// Restores the terminal even when setup fails midway (F31 review nit):
/// `raw`/`alt` track what actually succeeded, and Drop undoes exactly that.
struct TerminalGuard {
    raw: bool,
    alt: bool,
}

impl TerminalGuard {
    fn new() -> Self {
        TerminalGuard {
            raw: false,
            alt: false,
        }
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if self.raw {
            let _ = disable_raw_mode();
        }
        if self.alt {
            let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
        }
    }
}

fn run_tui_loop(app: &mut App, reg_path: &Path) -> CliResult<()> {
    let mut guard = TerminalGuard::new();
    enable_raw_mode().map_err(|e| CliError::Msg(format!("terminal setup: {e}")))?;
    guard.raw = true;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    guard.alt = true;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let result = tui_loop(app, reg_path, &mut terminal);
    disable_raw_mode()?;
    guard.raw = false;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    guard.alt = false;
    terminal.show_cursor()?;
    result
}

fn tui_loop(
    app: &mut App,
    reg_path: &Path,
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
) -> CliResult<()> {
    loop {
        terminal.draw(|f| decispec_tui::views::render(app, f))?;
        if !event::poll(std::time::Duration::from_millis(200))? {
            continue;
        }
        let Some(input) = translate_event(event::read()?) else {
            continue;
        };
        match app.update(input) {
            Action::Quit => return Ok(()),
            Action::Reload => reload_tui_projects(app, reg_path),
            Action::SaveSpec { ref file, ref text } => {
                let Some(project) = app.current() else { continue };
                let root = project.path.clone();
                match save_spec_file(&root, file, text) {
                    Ok(msg) => {
                        refresh_project(app, app.project);
                        app.mark_saved();
                        app.status = msg;
                    }
                    Err(e) => app.status = format!("save failed: {e}"),
                }
            }
            Action::CreatePr => {
                let Some(project) = app.current() else { continue };
                match create_spec_pr(&project.path, &project.name) {
                    Ok(url) => app.status = format!("draft PR: {url}"),
                    Err(e) => app.status = format!("PR failed: {e}"),
                }
            }
            Action::None => {}
        }
    }
}

/// Write an edited spec file into the registered project, then re-check the
/// whole workspace (the consistency half of "consistent and testable").
/// Ok carries the outcome: a summary, or a loud WARNING when the write
/// succeeded but the workspace no longer checks. Err means unwritable.
fn save_spec_file(root: &Path, file: &str, text: &str) -> Result<String, String> {
    use std::path::Component;
    let rel = Path::new(file);
    let escapes = rel.is_absolute()
        || !rel.starts_with("specs")
        || rel.components()
            .any(|c| matches!(c, Component::ParentDir));
    if escapes {
        return Err(format!("{file}: refusing to write outside specs/"));
    }
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    match load_workspace(root) {
        Ok(ws) => Ok(format!(
            "saved {file} — ok: {} decision(s), {} spec(s)",
            ws.decisions.len(),
            ws.specs.len()
        )),
        // written, but the project no longer checks: loud warning, not an
        // error-under-"save failed:" — the file IS what the buffer said.
        Err(errors) => Ok(format!(
            "saved {file} — WARNING: workspace check fails: {}",
            errors.join("; ")
        )),
    }
}

/// Reload one project's data in place after a save (workspace, sources,
/// gate) so the rest of the cockpit sees the new state immediately.
fn refresh_project(app: &mut App, idx: usize) {
    let Some(p) = app.projects.get(idx) else {
        return;
    };
    let entry = decispec_tui::registry::ProjectEntry {
        name: p.name.clone(),
        path: p.path.to_string_lossy().to_string(),
    };
    app.projects[idx] = load_tui_project(&entry);
}

/// Everything that must hold before we touch git: `gh_ok` is injected so the
/// checks are testable without PATH games.
fn pr_readiness(root: &Path, gh_ok: bool) -> Result<(), String> {
    if !root.join(".git").is_dir() {
        return Err(format!(
            "{} is not a git repository",
            root.display()
        ));
    }
    let status = git_output(root, &["status", "--porcelain"])?;
    if status.trim().is_empty() {
        return Err("no uncommitted changes — save a spec edit first".to_string());
    }
    if !gh_ok {
        return Err("gh is not installed or not on PATH".to_string());
    }
    Ok(())
}

fn gh_available() -> bool {
    std::process::Command::new("gh")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// `spec/tui-YYYYMMDD-HHMMSS` (UTC) — one branch per cockpit PR.
fn pr_branch_name(now: std::time::SystemTime) -> String {
    let secs = now
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (y, mo, d, h, mi, s) = civil_from_unix(secs);
    format!("spec/tui-{y:04}{mo:02}{d:02}-{h:02}{mi:02}{s:02}")
}

/// Days-since-epoch -> civil date (Howard Hinnant's algorithm), UTC. The
/// +719_468 shift moves day 0 to 0000-03-01, which is what the era math
/// expects.
fn civil_from_unix(secs: u64) -> (i64, u32, u32, u32, u32, u32) {
    let z = (secs / 86_400) as i64 + 719_468;
    let secs_of_day = secs % 86_400;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (
        y,
        m as u32,
        d as u32,
        (secs_of_day / 3_600) as u32,
        ((secs_of_day % 3_600) / 60) as u32,
        (secs_of_day % 60) as u32,
    )
}

/// Create a draft PR for the project's uncommitted spec changes: branch,
/// stage only `specs/`, commit, push, `gh pr create --draft` (base `main`).
/// Never merges; every failure is a plain Err for the status bar.
fn create_spec_pr(root: &Path, project_name: &str) -> Result<String, String> {
    create_spec_pr_with(root, project_name, gh_available())
}

/// `gh_ok` is injected so tests exercise the git flow deterministically
/// whatever `gh` the ambient machine has.
fn create_spec_pr_with(root: &Path, project_name: &str, gh_ok: bool) -> Result<String, String> {
    pr_readiness(root, gh_ok)?;
    let branch = pr_branch_name(std::time::SystemTime::now());
    let title = format!("spec: {project_name} edits from decispec tui");
    git(root, &["checkout", "-b", &branch])?;
    git(root, &["add", "specs"])?;
    let staged = git_output(root, &["diff", "--cached", "--name-only"])?;
    if staged.trim().is_empty() {
        // leave the worktree as we found it
        let _ = git(root, &["reset"]);
        let _ = git(root, &["checkout", "-"]);
        let _ = git(root, &["branch", "-d", &branch]);
        return Err("changes are outside specs/ — refusing to PR non-spec files".to_string());
    }
    git(root, &["commit", "-m", &title])?;
    git(root, &["push", "-u", "origin", &branch]).map_err(|e| format!("push failed: {e}"))?;
    let out = std::process::Command::new("gh")
        .args([
            "pr",
            "create",
            "--draft",
            "--base",
            "main",
            "--title",
            &title,
            "--body",
            "Draft opened by `decispec tui` (agent-assisted). Specs only; review before merging.",
        ])
        .current_dir(root)
        .output()
        .map_err(|e| format!("gh pr create: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "gh pr create failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn git(root: &Path, args: &[&str]) -> Result<(), String> {
    git_output(root, args).map(|_| ())
}

fn git_output(root: &Path, args: &[&str]) -> Result<String, String> {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|e| format!("git {}: {e}", args.join(" ")))?;
    if !out.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// `r` re-reads every registered project from disk (specs change under the
/// cockpit all the time). Registry read errors land in the status bar; the
/// session keeps its previous data.
fn reload_tui_projects(app: &mut App, reg_path: &Path) {
    match load_or_seed_registry(reg_path, seeds_for_this_machine()) {
        Ok(registry) => {
            app.projects = registry.projects.iter().map(load_tui_project).collect();
            app.project = app.project.min(app.projects.len().saturating_sub(1));
            app.status = format!("reloaded {} project(s)", app.projects.len());
        }
        Err(e) => app.status = format!("reload failed: {e}"),
    }
}

/// crossterm event -> terminal-independent input. Only key presses count
/// (no repeat/release); control-modified chars become `Ctrl(c)`; resize and
/// mouse events just trigger a redraw.
fn translate_event(ev: Event) -> Option<Input> {
    use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
    match ev {
        Event::Key(key) if key.kind == KeyEventKind::Press => {
            let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
            match key.code {
                KeyCode::Char(c) if ctrl => Some(Input::Ctrl(c.to_ascii_lowercase())),
                KeyCode::Char(c) => Some(Input::Char(c)),
                KeyCode::Up => Some(Input::Up),
                KeyCode::Down => Some(Input::Down),
                KeyCode::Left => Some(Input::Left),
                KeyCode::Right => Some(Input::Right),
                KeyCode::Tab => Some(Input::Tab),
                KeyCode::Enter => Some(Input::Enter),
                KeyCode::Esc => Some(Input::Esc),
                KeyCode::Home => Some(Input::Home),
                KeyCode::End => Some(Input::End),
                KeyCode::PageUp => Some(Input::PageUp),
                KeyCode::PageDown => Some(Input::PageDown),
                KeyCode::Backspace => Some(Input::Backspace),
                KeyCode::Delete => Some(Input::Delete),
                _ => None,
            }
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn run(cli: Cli) -> CliResult<ExitCode> {
    match cli.command {
        CommandKind::Init { demo, from } => {
            let root = std::env::current_dir()?;
            cmd_init(&root, demo, from)
        }
        CommandKind::Check { strict } => {
            let root = find_project_root()?;
            cmd_check(&root, strict)
        }
        CommandKind::Gen { only } => {
            let root = find_project_root()?;
            let only = match only {
                None => None,
                Some(s) => {
                    let list = split_prefixes(&s);
                    if list.is_empty() {
                        return Err(CliError::Msg(
                            "--only requires at least one non-empty prefix".into(),
                        ));
                    }
                    Some(list)
                }
            };
            cmd_gen(&root, only.as_deref())
        }
        CommandKind::Test => {
            let root = find_project_root()?;
            cmd_test(&root)
        }
        CommandKind::Gate {
            json,
            results,
            strict_skipped,
            allow_skips,
            prefix,
            status,
            only_failing,
            only_uncovered,
        } => {
            let root = find_project_root()?;
            let filter = parse_filter(prefix.as_deref(), status.as_deref(), only_failing, only_uncovered)?;
            cmd_gate(&root, json, results, strict_skipped, allow_skips, &filter)
        }
        CommandKind::Query { id, status } => {
            let root = find_project_root()?;
            let status = match status.as_deref() {
                None => None,
                Some(s) => Some(parse_status(s)?),
            };
            cmd_query(&root, &id, status.as_ref())
        }
        CommandKind::Coverage { min } => {
            let root = find_project_root()?;
            cmd_coverage(&root, min)
        }
        CommandKind::Extract { path, stdout, to } => {
            let root = match path {
                Some(p) if p.is_dir() => p,
                Some(p) => {
                    return Err(CliError::Msg(format!(
                        "{} is not an existing directory",
                        p.display()
                    )));
                }
                None => find_project_root()?,
            };
            cmd_extract(&root, stdout, to)
        }
        CommandKind::Mcp => cmd_mcp(),
        CommandKind::Index { out } => {
            let root = find_project_root()?;
            cmd_index(&root, out)
        }
        CommandKind::Tui { register } => cmd_tui(register),
    }
}

/// Map a command outcome to the process exit code: an error means DecisionSpec
/// could not complete the operation, so it is never a findings code.
fn exit_code(result: CliResult<ExitCode>) -> ExitCode {
    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(EXIT_CANNOT_COMPLETE)
        }
    }
}

fn main() -> ExitCode {
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    // Bad CLI usage means the command never ran (help/version keep exit 0).
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) if e.use_stderr() => {
            e.print().ok();
            return ExitCode::from(EXIT_CANNOT_COMPLETE);
        }
        Err(e) => e.exit(),
    };
    exit_code(run(cli))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_parser_strips_inline_comments() {
        let toml = "[project]\nname = \"demo\"   # my project\n\n[stack]\nlang = \"python\"              # python | rust\nglue_module = \"tests.glue\"   # python glue import path\n";
        let cfg = parse_config(toml);
        assert_eq!(cfg.project, "demo");
        assert_eq!(cfg.lang, "python");
        assert_eq!(cfg.glue_module, "tests.glue");
        assert_eq!(cfg.rust_glue, "tests/glue/rust");
    }

    #[test]
    fn config_parser_keeps_defaults_for_missing_keys() {
        let cfg = parse_config("[project]\nname = \"x\"\n");
        assert_eq!(cfg.lang, "python");
        assert_eq!(cfg.glue_module, "tests.glue");
    }

    #[test]
    fn config_reads_fitness_package_map() {
        let cfg = parse_config(
            "[project]\nname = \"demo\"\n\n[stack.fitness]\nroot_package = \"myapp\"\n\n[stack.fitness.packages]\napi = \"myapp.api\"\nauth = \"myapp.auth\"\n",
        );
        assert_eq!(cfg.root_package, "myapp");
        assert_eq!(
            cfg.container_packages.get("api").map(String::as_str),
            Some("myapp.api")
        );
        assert_eq!(
            cfg.container_packages.get("auth").map(String::as_str),
            Some("myapp.auth")
        );
        assert_eq!(cfg.container_packages.len(), 2);
    }

    // -- F11: [runners] overrides + probe spawn errors ------------------------

    #[test]
    fn config_reads_runners_table() {
        let cfg = parse_config(
            "[runners]\npytest = \"backend/.venv/bin/python -m pytest\"\ndepcruise = \"npx depcruise\"\n",
        );
        assert_eq!(
            cfg.runners.get("pytest").map(String::as_str),
            Some("backend/.venv/bin/python -m pytest")
        );
        assert_eq!(
            cfg.runners.get("depcruise").map(String::as_str),
            Some("npx depcruise")
        );
        assert_eq!(cfg.runners.len(), 2);
        // no table -> empty map, built-in defaults apply
        assert!(parse_config("[project]\nname = \"x\"\n").runners.is_empty());
    }

    /// A fake `pytest`: answers `--version`, writes the JUnit xml on a real
    /// run, and drops a marker proving it ran. Root-relative program path on
    /// purpose — overrides must resolve against the project root.
    const FAKE_PYTEST: &str = r#"#!/bin/sh
for a in "$@"; do
  if [ "$a" = "--version" ]; then
    echo "fake-pytest 9.9.9"
    exit 0
  fi
done
out=""
prev=""
for a in "$@"; do
  if [ "$prev" = "--junitxml" ]; then out="$a"; fi
  prev="$a"
done
if [ -n "$out" ]; then
  printf '%s' '<testsuite name="fake-pytest"></testsuite>' > "$out"
fi
touch .ran-by-fake-pytest
exit 0
"#;

    fn project_with_fake_pytest() -> (PathBuf, Config) {
        use std::os::unix::fs::PermissionsExt;

        let root = temp_project();
        std::fs::create_dir_all(root.join("tests/generated/unit")).unwrap();
        std::fs::write(root.join("tests/generated/unit/test_x.py"), "def test_x():\n    pass\n").unwrap();
        std::fs::create_dir_all(root.join("tools")).unwrap();
        let script = root.join("tools/fake-pytest");
        std::fs::write(&script, FAKE_PYTEST).unwrap();
        let mut perms = std::fs::metadata(&script).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script, perms).unwrap();
        let cfg = parse_config("[runners]\npytest = \"tools/fake-pytest\"\n");
        (root, cfg)
    }

    #[test]
    fn adapter_uses_runner_override_for_probe_and_run() {
        let (root, cfg) = project_with_fake_pytest();

        let manifest = run_adapters(&root, &cfg).unwrap();
        let pytest = manifest
            .adapters
            .iter()
            .find(|a| a.name == "pytest")
            .expect("pytest adapter missing");
        assert_eq!(pytest.status, AdapterStatus::Ran);
        assert_eq!(
            pytest.reason, None,
            "a runner override that works must not record a skip reason"
        );
        assert_eq!(pytest.result_files, vec!["pytest.xml".to_string()]);
        // the override — not the system python3 — did the work
        assert!(root.join(".ran-by-fake-pytest").is_file(), "fake runner did not run");
        assert!(root.join("tests/generated/results/pytest.xml").is_file());

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn runner_override_missing_binary_skips_quietly_and_test_completes() {
        let root = temp_project();
        std::fs::write(
            root.join("decispec.toml"),
            "[project]\nname = \"t\"\n\n[runners]\npytest = \"no/such/binary\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("tests/generated/unit")).unwrap();
        std::fs::write(root.join("tests/generated/unit/test_x.py"), "def test_x():\n    pass\n").unwrap();
        let cfg = parse_config(
            &std::fs::read_to_string(root.join("decispec.toml")).unwrap(),
        );

        // the spawn error becomes a skip, never a bail: adapters still complete
        let manifest = run_adapters(&root, &cfg).unwrap();
        let pytest = manifest
            .adapters
            .iter()
            .find(|a| a.name == "pytest")
            .expect("pytest adapter missing");
        assert_eq!(pytest.status, AdapterStatus::Skipped);
        assert!(
            pytest
                .reason
                .as_deref()
                .unwrap_or("")
                .contains("toolchain not installed"),
            "reason: {:?}",
            pytest.reason
        );
        assert!(pytest.reason.as_deref().unwrap_or("").contains("no/such/binary"));

        // ...and `decispec test` still exits 0 with a manifest on disk
        assert_eq!(cmd_test(&root).unwrap(), ExitCode::from(EXIT_OK));
        let manifest: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(root.join("tests/generated/results/manifest.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest["adapters"][0]["name"], serde_json::json!("pytest"));
        assert_eq!(
            manifest["adapters"][0]["status"],
            serde_json::json!("skipped")
        );

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn config_fitness_defaults_are_empty_and_unmapped() {
        let cfg = parse_config("[project]\nname = \"demo\"\n");
        assert!(cfg.root_package.is_empty());
        assert!(cfg.container_packages.is_empty());
    }

    /// Demo spec with `invalid_token` removed (requirement AUTH-001-R2 and
    /// its scenario) and flow `svc_auth` renamed to `svc_auth2`.
    const DEMO_SPEC_V2: &str = r#"
decision AUTH-001 "Use OAuth2 client credentials for service-to-service auth" {
  status: accepted
  context: "Internal services need machine-to-machine auth without shared secrets"
  consequences: "+ no shared secrets in code; - token latency on cold start"
}

model {
  container api "Orders API"
  container auth "Auth Service"
  rel api -> auth: "requests token"
  flow svc_auth2 {
    api -> auth: "POST /token"
    alt valid {
      auth --> api: "200 + access token"
    } else {
      auth --> api: "401 unauthorized"
    }
  }
}

spec AUTH-001 {
  requirement AUTH-001-R1 (EARS) {
    text: "WHEN a service presents valid client credentials THEN the auth service SHALL return an access token"
    layers: [unit, contract, e2e]
    scenarios: [valid_token]
  }
  scenario valid_token {
    given: "valid client credentials"
    when: "a token request is made"
    then: "the response status is 200 and a token is returned"
  }
  invariant token_ttl {
    expr: "access_token.ttl_seconds <= 3600"
    layers: [unit]
  }
  infra_policy no_hardcoded_secrets {
    description: "No secrets committed in source or config"
    layers: [infra]
  }
}
"#;

    fn temp_project() -> PathBuf {
        // The counter is what guarantees uniqueness: SystemTime nanos has
        // microsecond resolution on some platforms, so tests running in
        // parallel can land on the same directory.
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "decispec-gen-sweep-test-{}-{n}",
            std::process::id(),
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("specs")).unwrap();
        std::fs::write(root.join("decispec.toml"), "[project]\nname = \"t\"\n").unwrap();
        root
    }

    #[test]
    fn gen_removes_stale_artifacts_and_keeps_foreign_files() {
        let root = temp_project();
        std::fs::write(
            root.join("specs/0001-auth.spec"),
            DEMO_SPEC.trim_start_matches('\n'),
        )
        .unwrap();

        // gen #1: everything from the full demo spec; nothing is stale yet.
        let ws = load_workspace(&root).expect("demo spec should parse");
        let cfg = load_config(&root).unwrap();
        let removed = write_artifacts(&root, &ws, &cfg, None).unwrap();
        assert!(removed.is_empty(), "first gen must remove nothing");

        let stale_py = root.join("tests/generated/unit/test_AUTH_001_invalid_token.py");
        let stale_mmd = root.join(".decispec/diagrams/svc_auth.mmd");
        let stale_puml = root.join(".decispec/diagrams/svc_auth.puml");
        let kept_py = root.join("tests/generated/unit/test_AUTH_001_valid_token.py");
        let kept_ts = root.join("tests/generated/e2e/valid_token.spec.ts");
        assert!(stale_py.is_file() && stale_mmd.is_file() && stale_puml.is_file());
        assert!(kept_py.is_file() && kept_ts.is_file());

        // Foreign files that every sweep must leave alone.
        std::fs::create_dir_all(root.join("tests/glue")).unwrap();
        let glue = root.join("tests/glue/__init__.py");
        std::fs::write(&glue, "# hand-written glue\n").unwrap();
        std::fs::create_dir_all(root.join("tests/generated/results")).unwrap();
        let manifest = root.join("tests/generated/results/manifest.json");
        // Header-carrying lookalike in results/: still never removed.
        std::fs::write(
            &manifest,
            format!(
                "{}\n{{}}\n",
                decispec_codegen::header("tests/generated/results/manifest.json")
            ),
        )
        .unwrap();
        let hand_py = root.join("tests/generated/unit/hand_written.py");
        std::fs::write(&hand_py, "# hand-written, no header\n").unwrap();
        let hand_mmd = root.join(".decispec/diagrams/hand.mmd");
        std::fs::write(&hand_mmd, "sequenceDiagram\n    a->>b: x\n").unwrap();

        // gen #2: scenario removed, flow renamed.
        std::fs::write(
            root.join("specs/0001-auth.spec"),
            DEMO_SPEC_V2.trim_start_matches('\n'),
        )
        .unwrap();
        let ws = load_workspace(&root).expect("spec v2 should parse");
        let removed = write_artifacts(&root, &ws, &load_config(&root).unwrap(), None).unwrap();

        assert!(!stale_py.exists(), "stale unit test for removed scenario");
        assert!(!stale_mmd.exists(), "stale mermaid for renamed flow");
        assert!(!stale_puml.exists(), "stale plantuml for renamed flow");
        assert_eq!(
            removed,
            vec![
                ".decispec/diagrams/svc_auth.mmd".to_string(),
                ".decispec/diagrams/svc_auth.puml".to_string(),
                "tests/generated/unit/test_AUTH_001_invalid_token.py".to_string(),
            ]
        );
        assert!(root.join(".decispec/diagrams/svc_auth2.mmd").is_file());
        assert!(root.join(".decispec/diagrams/model.mmd").is_file());
        assert!(kept_py.is_file(), "rendered file must survive");
        assert!(kept_ts.is_file(), "rendered file must survive");
        assert!(glue.is_file(), "glue must survive");
        assert!(manifest.is_file(), "results/ must survive");
        assert!(hand_py.is_file(), "headerless file must survive");
        assert!(hand_mmd.is_file(), "headerless diagram must survive");

        // gen #3 identical: idempotent, bytes unchanged, nothing removed.
        let before = std::fs::read(&kept_ts).unwrap();
        let removed = write_artifacts(&root, &ws, &load_config(&root).unwrap(), None).unwrap();
        assert!(removed.is_empty(), "re-gen must remove nothing");
        assert_eq!(std::fs::read(&kept_ts).unwrap(), before);

        std::fs::remove_dir_all(&root).unwrap();
    }

    // -- F46: GEN-008 — diagrams under .decispec/diagrams ----------------------

    #[test]
    fn gen_writes_diagrams_under_decispec_dir_not_docs() {
        let root = temp_project();
        std::fs::write(
            root.join("specs/0001-auth.spec"),
            DEMO_SPEC.trim_start_matches('\n'),
        )
        .unwrap();
        let ws = load_workspace(&root).expect("demo spec should parse");
        write_artifacts(&root, &ws, &load_config(&root).unwrap(), None).unwrap();

        assert!(root.join(".decispec/diagrams/svc_auth.mmd").is_file());
        assert!(root.join(".decispec/diagrams/svc_auth.puml").is_file());
        assert!(root.join(".decispec/diagrams/model.mmd").is_file());
        assert!(
            !root.join("docs/diagrams").exists(),
            "gen must not create the legacy docs/diagrams tree"
        );

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn sweep_migrates_header_diagrams_out_of_docs_diagrams() {
        let root = temp_project();
        std::fs::write(
            root.join("specs/0001-auth.spec"),
            DEMO_SPEC.trim_start_matches('\n'),
        )
        .unwrap();
        let ws = load_workspace(&root).expect("demo spec should parse");
        let cfg = load_config(&root).unwrap();
        write_artifacts(&root, &ws, &cfg, None).unwrap();

        // A repo that committed the old tree: one generated file, one edited.
        std::fs::create_dir_all(root.join("docs/diagrams")).unwrap();
        let legacy = root.join("docs/diagrams/committed.mmd");
        std::fs::write(
            &legacy,
            format!(
                "{}\nsequenceDiagram\n    a->>b: x\n",
                decispec_codegen::header("docs/diagrams/committed.mmd")
            ),
        )
        .unwrap();
        let hand = root.join("docs/diagrams/hand.mmd");
        std::fs::write(&hand, "sequenceDiagram\n    a->>b: hand\n").unwrap();

        // An identical re-gen triggers only the migration sweep.
        let removed = write_artifacts(&root, &ws, &cfg, None).unwrap();
        assert!(!legacy.exists(), "legacy header-bearing diagram must migrate away");
        assert!(hand.is_file(), "hand-edited diagram must survive");
        assert!(
            removed.contains(&"docs/diagrams/committed.mmd".to_string()),
            "migration must be reported in the removed list: {removed:?}"
        );

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn gen_twice_keeps_diagram_bytes_identical() {
        let root = temp_project();
        std::fs::write(
            root.join("specs/0001-auth.spec"),
            DEMO_SPEC.trim_start_matches('\n'),
        )
        .unwrap();
        let ws = load_workspace(&root).expect("demo spec should parse");
        let cfg = load_config(&root).unwrap();
        write_artifacts(&root, &ws, &cfg, None).unwrap();
        let diagram = root.join(".decispec/diagrams/svc_auth.mmd");
        let first = std::fs::read(&diagram).unwrap();

        let removed = write_artifacts(&root, &ws, &cfg, None).unwrap();
        assert!(removed.is_empty(), "identical re-gen must remove nothing");
        assert_eq!(std::fs::read(&diagram).unwrap(), first, "GEN-001 at the new location");

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn sweep_removes_stale_diagrams_from_decispec_dir_only() {
        let root = temp_project();
        std::fs::write(
            root.join("specs/0001-auth.spec"),
            DEMO_SPEC.trim_start_matches('\n'),
        )
        .unwrap();
        let ws = load_workspace(&root).expect("demo spec should parse");
        let cfg = load_config(&root).unwrap();
        write_artifacts(&root, &ws, &cfg, None).unwrap();

        // Stale diagram inside the managed tree, and a header lookalike outside
        // every managed tree — only the first may ever be swept.
        let stale = root.join(".decispec/diagrams/ghost.mmd");
        std::fs::write(
            &stale,
            format!(
                "{}\nsequenceDiagram\n",
                decispec_codegen::header(".decispec/diagrams/ghost.mmd")
            ),
        )
        .unwrap();
        std::fs::create_dir_all(root.join("docs")).unwrap();
        let foreign = root.join("docs/foreign.mmd");
        std::fs::write(
            &foreign,
            format!("{}\n", decispec_codegen::header("docs/foreign.mmd")),
        )
        .unwrap();

        let removed = write_artifacts(&root, &ws, &cfg, None).unwrap();
        assert!(!stale.exists(), "stale managed diagram must be swept");
        assert!(
            foreign.is_file(),
            "a header lookalike outside the managed trees must survive"
        );
        assert!(removed.contains(&".decispec/diagrams/ghost.mmd".to_string()));

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn sweep_does_not_create_managed_dirs_when_absent() {
        // A project where gen has never run: no docs/ or tests/ trees at all.
        let root = temp_project();
        let removed = sweep_stale_artifacts(&root, &[]).unwrap();
        assert!(removed.is_empty());
        assert!(!root.join("docs").exists(), "sweep must not create docs/");
        assert!(!root.join("tests").exists(), "sweep must not create tests/");
        assert!(
            !root.join(".decispec").exists(),
            "sweep must not create .decispec/ either"
        );

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn gate_exits_2_when_specs_are_broken() {
        let root = temp_project();
        std::fs::write(
            root.join("specs/0001-auth.spec"),
            DEMO_SPEC.trim_start_matches('\n').replace(
                "scenarios: [valid_token]",
                "scenarios: [valid_token, nope_missing]",
            ),
        )
        .unwrap();
        let results = root.join("tests/generated/results");
        std::fs::create_dir_all(&results).unwrap();

        // gate cannot build a matrix without a valid workspace: it could not
        // complete, so it must NOT exit 1 (that code is reserved for a FAIL
        // verdict).
        assert_eq!(
            exit_code(cmd_gate(&root, false, Some(results), false, None, &Filter::default())),
            ExitCode::from(EXIT_CANNOT_COMPLETE)
        );

        // check reports malformed specs by design (that IS its job), so 1.
        assert_eq!(cmd_check(&root, false).unwrap(), ExitCode::from(EXIT_FINDINGS));

        std::fs::remove_dir_all(&root).unwrap();
    }

    // -- F26: check --strict (style lint) -------------------------------------

    /// A workspace that parses and validates clean but breaks the
    /// authoring-style rules: consequences with no minus entry.
    const STYLE_BAD: &str = "decision U-9 \"t\" {\n  status: accepted\n  context: \"ev\"\n  consequences: \"+ only plus\"\n}\n";

    #[test]
    fn check_strict_reports_style_findings_and_exits_1() {
        let root = temp_project();
        std::fs::write(root.join("specs/0001-style.spec"), STYLE_BAD).unwrap();
        assert_eq!(
            exit_code(cmd_check(&root, true)),
            ExitCode::from(EXIT_FINDINGS)
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn check_without_strict_ignores_style_findings() {
        let root = temp_project();
        std::fs::write(root.join("specs/0001-style.spec"), STYLE_BAD).unwrap();
        assert_eq!(
            exit_code(cmd_check(&root, false)),
            ExitCode::from(EXIT_OK)
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn gate_exits_1_on_fail_verdict() {
        let root = temp_project();
        std::fs::write(
            root.join("specs/0001-auth.spec"),
            DEMO_SPEC.trim_start_matches('\n'),
        )
        .unwrap();
        // Existing but empty: rows are uncovered / have no artifacts.
        let results = root.join("tests/generated/results");
        std::fs::create_dir_all(&results).unwrap();

        assert_eq!(
            exit_code(cmd_gate(&root, false, Some(results), false, None, &Filter::default())),
            ExitCode::from(EXIT_FINDINGS)
        );
        assert!(root.join(".decispec/report.json").is_file());

        std::fs::remove_dir_all(&root).unwrap();
    }

    // -- F39: gate console filters -------------------------------------------

    #[test]
    fn gate_with_filters_prints_a_narrowed_matrix_but_exit_follows_verdict() {
        let root = temp_project();
        std::fs::write(
            root.join("specs/0001-auth.spec"),
            DEMO_SPEC.trim_start_matches('\n'),
        )
        .unwrap();
        let results = root.join("tests/generated/results");
        std::fs::create_dir_all(&results).unwrap();

        // empty results -> FAIL verdict whatever the filter shows…
        let none_match = Filter {
            prefixes: vec!["ZZZ-".to_string()],
            ..Default::default()
        };
        assert_eq!(
            exit_code(cmd_gate(&root, false, Some(results.clone()), false, None, &none_match)),
            ExitCode::from(EXIT_FINDINGS)
        );
        // …and a filter that matches does not soften it either.
        let some_match = Filter {
            prefixes: vec!["AUTH".to_string()],
            ..Default::default()
        };
        assert_eq!(
            exit_code(cmd_gate(&root, false, Some(results), false, None, &some_match)),
            ExitCode::from(EXIT_FINDINGS)
        );

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn gate_json_with_prefix_filter_is_narrowed_while_report_json_stays_full() {
        let root = temp_project();
        std::fs::write(
            root.join("specs/0001-auth.spec"),
            DEMO_SPEC.trim_start_matches('\n'),
        )
        .unwrap();
        let results = root.join("tests/generated/results");
        std::fs::create_dir_all(&results).unwrap();

        let none_match = Filter {
            prefixes: vec!["ZZZ-".to_string()],
            ..Default::default()
        };
        assert_eq!(
            exit_code(cmd_gate(&root, true, Some(results), false, None, &none_match)),
            ExitCode::from(EXIT_FINDINGS)
        );

        // the persisted report is always the FULL one — filters are a
        // console concern, never a record concern (stdout narrowing is
        // covered by gate::filtered_report's unit tests)
        let on_disk = std::fs::read_to_string(root.join(".decispec/report.json")).unwrap();
        assert!(on_disk.contains("AUTH-001"), "report.json lost rows: {on_disk}");

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn filter_flags_parse_prefix_csv_and_reject_unknown_status() {
        let f = parse_filter(Some("CMD, DSL"), None, false, false).unwrap();
        assert_eq!(f.prefixes, vec!["CMD".to_string(), "DSL".to_string()]);
        assert!(f.status.is_none());

        let f = parse_filter(None, Some("accepted"), true, true).unwrap();
        assert_eq!(f.status, Some(decispec_ir::Status::Accepted));
        assert!(f.only_failing && f.only_uncovered);

        assert!(parse_filter(None, Some("bogus"), false, false).is_err());
    }

    // -- F40: query glob patterns -------------------------------------------

    const QUERY_WS: &str = r#"
decision CMD-001 "One" {
  status: accepted
  context: "ev"
  consequences: "+ a. - b"
}
decision CMD-002 "Two" {
  status: proposed
  context: "ev"
  consequences: "+ a. - b"
}
decision TUI-001 "Three" {
  status: accepted
  context: "ev"
  consequences: "+ a. - b"
}
"#;

    fn query_ws() -> Workspace {
        let unit = decispec_parse::parse_file("specs/q.spec", QUERY_WS).unwrap();
        decispec_parse::merge(vec![unit])
    }

    #[test]
    fn query_glob_match_follows_wildcard_semantics() {
        assert!(id_glob_match("CMD-*", "CMD-001"));
        assert!(!id_glob_match("CMD-*", "TUI-001"));
        assert!(!id_glob_match("CMD-*", "XCMD-001"));
        assert!(id_glob_match("*001", "CMD-001"));
        assert!(id_glob_match("*", "ANY-ID"));
        assert!(id_glob_match("C*D-001", "CMD-001"));
        // no star: exact equality, the pre-F40 behavior
        assert!(id_glob_match("CMD-001", "CMD-001"));
        assert!(!id_glob_match("CMD-00", "CMD-001"));
    }

    #[test]
    fn query_pattern_selects_decisions_and_status_filters() {
        let ws = query_ws();
        let hits = matching_decisions(&ws, "CMD-*", None);
        assert_eq!(
            hits.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(),
            vec!["CMD-001", "CMD-002"]
        );
        let hits = matching_decisions(&ws, "CMD-*", Some(Status::Proposed));
        assert_eq!(
            hits.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(),
            vec!["CMD-002"]
        );
        assert!(matching_decisions(&ws, "ZZZ-*", None).is_empty());
    }

    #[test]
    fn query_pattern_with_no_match_exits_1() {
        let root = temp_project();
        std::fs::write(root.join("specs/0001-auth.spec"), QUERY_WS.trim_start()).unwrap();
        assert_eq!(
            exit_code(cmd_query(&root, "ZZZ-*", None)),
            ExitCode::from(EXIT_FINDINGS)
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn query_exact_id_and_star_pattern_both_exit_0() {
        let root = temp_project();
        std::fs::write(root.join("specs/0001-auth.spec"), QUERY_WS.trim_start()).unwrap();
        // exact id keeps the full-subtree path
        assert_eq!(
            exit_code(cmd_query(&root, "CMD-001", None)),
            ExitCode::from(EXIT_OK)
        );
        // star pattern lists compactly
        assert_eq!(
            exit_code(cmd_query(&root, "CMD-*", None)),
            ExitCode::from(EXIT_OK)
        );
        // --status narrows the exact path too: mismatch = no match
        assert_eq!(
            exit_code(cmd_query(&root, "CMD-001", Some(&Status::Rejected))),
            ExitCode::from(EXIT_FINDINGS)
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    // -- F41: gen --only ----------------------------------------------------

    const TUI_DEMO_SPEC: &str = r#"
decision TUI-001 "Picker narrows as the user types" {
  status: accepted
  context: "ev"
  consequences: "+ a. - b"
}
spec TUI-001 {
  requirement TUI-001-R1 (EARS) {
    text: "WHEN the user types in the picker THEN the list SHALL narrow"
    layers: [unit]
    scenarios: [picker_filters]
  }
  scenario picker_filters {
    given: "an open picker"
    when: "the user types"
    then: "only matching entries remain"
  }
}
"#;

    #[test]
    fn gen_only_writes_matching_artifacts_and_skips_the_sweep() {
        let root = temp_project();
        std::fs::write(
            root.join("specs/0001-auth.spec"),
            DEMO_SPEC.trim_start_matches('\n'),
        )
        .unwrap();
        std::fs::write(
            root.join("specs/0002-tui.spec"),
            TUI_DEMO_SPEC.trim_start_matches('\n'),
        )
        .unwrap();
        // A stale generated diagram from an earlier full run: the --only
        // sweep must leave it alone.
        let stale = root.join("docs/diagrams/old_flow.mmd");
        std::fs::create_dir_all(stale.parent().unwrap()).unwrap();
        std::fs::write(
            &stale,
            format!(
                "{}\nsequenceDiagram\n",
                decispec_codegen::header("docs/diagrams/old_flow.mmd")
            ),
        )
        .unwrap();

        assert_eq!(
            exit_code(cmd_gen(&root, Some(&["AUTH".to_string()]))),
            ExitCode::from(EXIT_OK)
        );

        assert!(
            root.join("tests/generated/unit/test_AUTH_001_valid_token.py")
                .is_file(),
            "matching spec's artifact must be written"
        );
        assert!(
            !root.join("tests/generated/unit/test_TUI_001_picker_filters.py")
                .exists(),
            "non-matching spec's artifact must not be written"
        );
        assert!(
            !root.join("docs/diagrams/svc_auth.mmd").exists(),
            "workspace diagrams must be skipped"
        );
        assert!(
            !root.join("tests/generated/infra/conftest.toml").exists(),
            "shared conftest must be skipped"
        );
        assert!(stale.is_file(), "stale sweep must be skipped under --only");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn gen_only_with_unmatched_prefix_exits_1_and_writes_nothing() {
        let root = temp_project();
        std::fs::write(
            root.join("specs/0001-auth.spec"),
            DEMO_SPEC.trim_start_matches('\n'),
        )
        .unwrap();
        assert_eq!(
            exit_code(cmd_gen(&root, Some(&["ZZZ".to_string()]))),
            ExitCode::from(EXIT_FINDINGS)
        );
        assert!(
            !root.join("tests/generated").exists(),
            "nothing may be written for an unmatched prefix"
        );
        assert!(!root.join("docs/diagrams").exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn gate_strict_skipped_turns_skipped_rows_into_findings() {
        let root = temp_project();
        std::fs::write(
            root.join("specs/0001-auth.spec"),
            DEMO_SPEC.trim_start_matches('\n'),
        )
        .unwrap();
        let ws = load_workspace(&root).expect("demo spec should parse");
        write_artifacts(&root, &ws, &load_config(&root).unwrap(), None).unwrap();
        // Empty results dir: artifacts exist, but no toolchain ran.
        let results = root.join("tests/generated/results");
        std::fs::create_dir_all(&results).unwrap();

        // Default: skipped rows do not affect the verdict (exit 0).
        assert_eq!(
            exit_code(cmd_gate(
                &root,
                false,
                Some(results.clone()),
                false,
                None,
                &Filter::default()
            )),
            ExitCode::from(EXIT_OK)
        );
        // Strict: the same rows are findings, so exit 1 — no new exit code.
        assert_eq!(
            exit_code(cmd_gate(
                &root,
                false,
                Some(results),
                true,
                None,
                &Filter::default()
            )),
            ExitCode::from(EXIT_FINDINGS)
        );

        let report: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(root.join(".decispec/report.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(report["strict_skipped"], serde_json::json!(true));
        assert_eq!(report["verdict"], serde_json::json!("fail"));

        std::fs::remove_dir_all(&root).unwrap();
    }

    // -- F47: skip honesty (GATE-008/009) --------------------------------------

    /// One decision, one unit scenario: the whole matrix is a single row, so
    /// an all-skipped run is easy to reason about.
    const SKIP_SPEC: &str = r#"
decision SKIP-001 "Known skipped work" {
  status: accepted
}
spec SKIP-001 {
  requirement SKIP-001-R1 (EARS) {
    text: "WHEN the pending work lands THEN the row SHALL pass"
    layers: [unit]
    scenarios: [only_scenario]
  }
  scenario only_scenario {
    given: "an unimplemented check"
    when: "the unit toolchain runs"
    then: "the case is reported skipped"
  }
}
"#;

    const SKIP_JUNIT: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<testsuites>
  <testsuite name="unit" tests="1" failures="0" skipped="1">
    <testcase classname="test_SKIP_001_only_scenario" name="test_scenario_SKIP_001_only_scenario_unit[unit]" time="0.001">
      <skipped message="gap tracked in TASH-515"/>
    </testcase>
  </testsuite>
</testsuites>"#;

    fn skip_project() -> PathBuf {
        let root = temp_project();
        std::fs::write(root.join("specs/0001-skip.spec"), SKIP_SPEC.trim_start_matches('\n'))
            .unwrap();
        let ws = load_workspace(&root).expect("skip spec should parse");
        write_artifacts(&root, &ws, &load_config(&root).unwrap(), None).unwrap();
        let results = root.join("tests/generated/results");
        std::fs::create_dir_all(&results).unwrap();
        std::fs::write(
            results.join("manifest.json"),
            r#"{"adapters":[{"name":"pytest","layer":"unit","status":"ran","result_files":["pytest.xml"]}]}"#,
        )
        .unwrap();
        std::fs::write(results.join("pytest.xml"), SKIP_JUNIT).unwrap();
        root
    }

    #[test]
    fn gate_allow_skips_relieves_listed_skips_end_to_end() {
        let root = skip_project();
        std::fs::write(
            root.join("specs/skips.toml"),
            "[[skips]]\ndecision = \"SKIP-001\"\nitem = \"only_scenario\"\nreason = \"ratchet: TASH-515\"\n",
        )
        .unwrap();
        let allow = Some(root.join("specs/skips.toml"));

        // Strict without the ratchet: the evidence skip is a finding (exit 1).
        assert_eq!(
            exit_code(cmd_gate(
                &root,
                false,
                Some(root.join("tests/generated/results")),
                true,
                None,
                &Filter::default()
            )),
            ExitCode::from(EXIT_FINDINGS)
        );
        // Strict with the ratchet: the listed row stays SKIPPED, exit 0.
        assert_eq!(
            exit_code(cmd_gate(
                &root,
                false,
                Some(root.join("tests/generated/results")),
                true,
                allow,
                &Filter::default()
            )),
            ExitCode::from(EXIT_OK)
        );

        let report: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(root.join(".decispec/report.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(report["counts"]["skipped"], serde_json::json!(1));
        let detail = report["decisions"][0]["rows"][0]["detail"].as_str().unwrap();
        assert!(detail.contains("allowlisted"), "{detail}");
        assert!(detail.contains("TASH-515"), "{detail}");

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn gate_allow_skips_missing_file_exits_2() {
        let root = skip_project();
        let missing = Some(root.join("specs/skips.toml"));
        assert_eq!(
            exit_code(cmd_gate(
                &root,
                false,
                Some(root.join("tests/generated/results")),
                true,
                missing,
                &Filter::default()
            )),
            ExitCode::from(EXIT_CANNOT_COMPLETE)
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    // -- F48: portable specs (PORT-001/002) ------------------------------------

    /// A bare directory: no decispec.toml, no specs/ — what `init --from`
    /// targets and what an external store looks like.
    fn bare_dir(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "decispec-port-test-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    /// One decision, one requirement, one scenario: a valid stored spec.
    const STORED_SPEC_A: &str = "decision TVGO-001 \"Sample A\" {\n  status: accepted\n}\nspec TVGO-001 {\n  requirement TVGO-001-R1 (EARS) {\n    text: \"WHEN a THEN b\"\n    layers: [unit]\n    scenarios: [one]\n  }\n  scenario one {\n    given: \"g\"\n    when: \"w\"\n    then: \"t\"\n  }\n}\n";
    const STORED_SPEC_B: &str = "decision VERY-001 \"Sample B\" {\n  status: accepted\n}\nspec VERY-001 {\n  requirement VERY-001-R1 (EARS) {\n    text: \"WHEN c THEN d\"\n    layers: [unit]\n    scenarios: [two]\n  }\n  scenario two {\n    given: \"g\"\n    when: \"w\"\n    then: \"t\"\n  }\n}\n";

    #[test]
    fn extract_to_writes_named_spec_outside_the_repo() {
        let root = temp_project();
        std::fs::create_dir_all(root.join("pkg")).unwrap();
        std::fs::write(root.join("pkg/mod.py"), "def f():\n    pass\n").unwrap();
        let store = bare_dir("store");
        let dirname = root.file_name().unwrap().to_string_lossy().to_string();

        assert_eq!(
            exit_code(cmd_extract(&root, false, Some(store.clone()))),
            ExitCode::from(EXIT_OK)
        );
        let written = store.join(format!("{dirname}.spec"));
        assert!(written.is_file(), "store must hold {dirname}.spec");
        assert!(
            !root.join("specs/extracted.spec").exists(),
            "the analyzed repo's own specs/ tree must stay untouched"
        );
        assert!(
            std::fs::read_dir(root.join("specs"))
                .unwrap()
                .flatten()
                .next()
                .is_none(),
            "no file may be written inside the analyzed project"
        );

        std::fs::remove_dir_all(&root).unwrap();
        std::fs::remove_dir_all(&store).unwrap();
    }

    #[test]
    fn extract_to_refuses_to_overwrite_existing_file() {
        let root = temp_project();
        std::fs::write(root.join("mod.py"), "def f():\n    pass\n").unwrap();
        let store = bare_dir("store-clash");
        let dirname = root.file_name().unwrap().to_string_lossy().to_string();
        let existing = store.join(format!("{dirname}.spec"));
        std::fs::write(&existing, "sentinel bytes").unwrap();

        assert_eq!(
            exit_code(cmd_extract(&root, false, Some(store.clone()))),
            ExitCode::from(EXIT_CANNOT_COMPLETE)
        );
        assert_eq!(
            std::fs::read_to_string(&existing).unwrap(),
            "sentinel bytes",
            "an existing store file is never overwritten"
        );

        std::fs::remove_dir_all(&root).unwrap();
        std::fs::remove_dir_all(&store).unwrap();
    }

    #[test]
    fn extract_to_and_stdout_are_mutually_exclusive() {
        let root = temp_project();
        std::fs::write(root.join("mod.py"), "def f():\n    pass\n").unwrap();
        let store = bare_dir("store-conflict");

        let code = exit_code(cmd_extract(&root, true, Some(store.clone())));
        assert_eq!(code, ExitCode::from(EXIT_CANNOT_COMPLETE));
        assert!(
            std::fs::read_dir(&store)
                .unwrap()
                .flatten()
                .next()
                .is_none(),
            "the mutual exclusion must write nothing"
        );
        assert!(!root.join("specs/extracted.spec").exists());

        std::fs::remove_dir_all(&root).unwrap();
        std::fs::remove_dir_all(&store).unwrap();
    }

    #[test]
    fn init_from_files_and_dirs_scaffolds_a_project() {
        let store = bare_dir("store-full");
        std::fs::write(store.join("tvgo.spec"), STORED_SPEC_A).unwrap();
        std::fs::write(store.join("verydian.spec"), STORED_SPEC_B).unwrap();
        let target = bare_dir("target");

        assert_eq!(
            exit_code(cmd_init(&target, false, vec![store.clone()])),
            ExitCode::from(EXIT_OK)
        );
        assert_eq!(
            std::fs::read_to_string(target.join("specs/tvgo.spec")).unwrap(),
            STORED_SPEC_A,
            "copied under the original name, byte-identical"
        );
        assert!(target.join("specs/verydian.spec").is_file());
        assert!(target.join("decispec.toml").is_file());
        assert!(target.join("tests/glue/__init__.py").is_file());
        // The new project is check-clean: the copied specs form a valid
        // workspace (this is the `decispec check exits 0` half of the spec).
        load_workspace(&target).expect("copied specs must form a valid workspace");

        std::fs::remove_dir_all(&store).unwrap();
        std::fs::remove_dir_all(&target).unwrap();
    }

    #[test]
    fn init_from_refuses_to_clobber_existing_specs() {
        let store = bare_dir("store-clobber");
        std::fs::write(store.join("tvgo.spec"), STORED_SPEC_A).unwrap();
        let target = bare_dir("target-clobber");
        std::fs::create_dir_all(target.join("specs")).unwrap();
        std::fs::write(target.join("specs/keep.spec"), STORED_SPEC_B).unwrap();

        assert_eq!(
            exit_code(cmd_init(&target, false, vec![store.clone()])),
            ExitCode::from(EXIT_CANNOT_COMPLETE)
        );
        assert_eq!(
            std::fs::read_to_string(target.join("specs/keep.spec")).unwrap(),
            STORED_SPEC_B,
            "the existing spec file is untouched"
        );
        assert!(
            !target.join("decispec.toml").exists(),
            "no scaffold may be written on the refusal path"
        );

        std::fs::remove_dir_all(&store).unwrap();
        std::fs::remove_dir_all(&target).unwrap();
    }

    #[test]
    fn init_from_broken_specs_exit_2_naming_diagnostics() {
        let store = bare_dir("store-broken");
        std::fs::write(
            store.join("broken.spec"),
            "decision BROKEN-001 \"never terminated {\n  context: \"unterminated\n}\n",
        )
        .unwrap();
        let target = bare_dir("target-broken");

        // The diagnostics themselves are file:line strings naming the store
        // file — pinned directly, not through a captured stderr.
        let files = select_store_specs(std::slice::from_ref(&store)).expect("selection should succeed");
        let diags = validate_store_specs(&files).expect_err("unterminated string must not parse");
        assert!(
            diags.iter().any(|d| d.contains("broken.spec") && d.contains(':')),
            "file:line diagnostics naming the store file: {diags:?}"
        );

        assert_eq!(
            exit_code(cmd_init(&target, false, vec![store.clone()])),
            ExitCode::from(EXIT_CANNOT_COMPLETE)
        );
        assert!(!target.join("decispec.toml").exists(), "no scaffold");
        assert!(!target.join("specs").exists(), "no specs dir");
        assert!(!target.join("tests").exists(), "no glue");

        std::fs::remove_dir_all(&store).unwrap();
        std::fs::remove_dir_all(&target).unwrap();
    }

    // -- F38: proof coverage (PROOF-001..005) ---------------------------------

    /// A minimal but shape-faithful report.json: D-1 all-pass, D-2 mixed,
    /// D-3 all-skipped, D-4 superseded. 1/3 proven -> 33%.
    const COVERAGE_REPORT: &str = r#"{
      "project": "demo",
      "verdict": "fail",
      "strict_skipped": false,
      "counts": {"rows": 5, "passed": 3, "failed": 1, "uncovered": 0, "skipped": 1, "no_artifacts": 0},
      "failures": [],
      "warnings": [],
      "decisions": [
        {"id": "D-1", "status": "accepted", "outcome": "pass",
         "rows": [
           {"decision": "D-1", "requirement": "D-1-R1", "item": "a", "kind": "scenario", "layer": "unit", "artifacts": [], "result": "pass", "detail": ""},
           {"decision": "D-1", "requirement": "D-1-R1", "item": "b", "kind": "scenario", "layer": "unit", "artifacts": [], "result": "pass", "detail": ""}
         ]},
        {"id": "D-2", "status": "proposed", "outcome": "fail",
         "rows": [
           {"decision": "D-2", "requirement": "D-2-R1", "item": "c", "kind": "scenario", "layer": "unit", "artifacts": [], "result": "pass", "detail": ""},
           {"decision": "D-2", "requirement": "D-2-R1", "item": "d", "kind": "scenario", "layer": "unit", "artifacts": [], "result": "fail", "detail": ""}
         ]},
        {"id": "D-3", "status": "accepted", "outcome": "fail",
         "rows": [
           {"decision": "D-3", "requirement": "D-3-R1", "item": "e", "kind": "scenario", "layer": "unit", "artifacts": [], "result": "skipped", "detail": "toolchain not installed"}
         ]},
        {"id": "D-4", "status": "superseded", "outcome": "excluded-superseded", "rows": []}
      ]
    }"#;

    #[test]
    fn coverage_prints_marked_lines_and_headline() {
        let (project, cov) = coverage_from_report(COVERAGE_REPORT).unwrap();
        assert_eq!(project, "demo");
        assert_eq!(cov.gatable, 3, "superseded D-4 excluded");
        assert_eq!(cov.proven, 1);
        assert_eq!(cov.percent, 33);

        let out = render_coverage(&project, &cov, "2026-10-20 12:00:00 UTC");
        // One marked line per gatable decision, symbols paired with labels.
        assert!(out.contains("✓ D-1 proven"), "{out}");
        assert!(out.contains("◐ D-2 partial"), "{out}");
        assert!(out.contains("○ D-3 unproven"), "{out}");
        assert!(!out.contains("D-4"), "excluded decisions must not appear: {out}");
        assert!(
            out.contains("coverage: 1/3 decisions proven (33%)"),
            "{out}"
        );
        // PROOF-001-R1: the output names the run it reflects.
        assert!(out.contains("from gate run 2026-10-20 12:00:00 UTC"), "{out}");
    }

    #[test]
    fn gate_summary_prints_the_proven_percentage() {
        // The gate summary line and the coverage command share coverage_line,
        // so the two surfaces cannot drift (PROOF-005).
        let (_, cov) = coverage_from_report(COVERAGE_REPORT).unwrap();
        assert_eq!(coverage_line(&cov), "coverage: 1/3 decisions proven (33%)");
    }

    #[test]
    fn coverage_below_the_min_floor_exits_1() {
        let root = temp_project();
        std::fs::create_dir_all(root.join(".decispec")).unwrap();
        std::fs::write(root.join(".decispec/report.json"), COVERAGE_REPORT).unwrap();

        // 33% < 50%: a finding, naming both numbers (asserted via the
        // render/parse pins above; here the exit contract).
        assert_eq!(
            exit_code(cmd_coverage(&root, Some(50))),
            ExitCode::from(EXIT_FINDINGS)
        );
        // At the floor, not below it: clean.
        assert_eq!(
            exit_code(cmd_coverage(&root, Some(33))),
            ExitCode::from(EXIT_OK)
        );

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn coverage_without_a_gate_report_exits_2() {
        let root = temp_project();
        // No .decispec/report.json at all: could-not-complete, never a
        // fabricated zero percent.
        assert_eq!(
            exit_code(cmd_coverage(&root, None)),
            ExitCode::from(EXIT_CANNOT_COMPLETE)
        );

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn coverage_with_corrupt_report_exits_2() {
        let root = temp_project();
        std::fs::create_dir_all(root.join(".decispec")).unwrap();
        std::fs::write(root.join(".decispec/report.json"), "not json").unwrap();
        assert_eq!(
            exit_code(cmd_coverage(&root, None)),
            ExitCode::from(EXIT_CANNOT_COMPLETE)
        );

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn format_utc_converts_known_epochs() {
        assert_eq!(format_utc(0), "1970-01-01 00:00:00 UTC");
        assert_eq!(format_utc(86_400), "1970-01-02 00:00:00 UTC");
        assert_eq!(format_utc(1_792_454_400), "2026-10-20 00:00:00 UTC");
    }

    #[test]
    fn gen_exits_2_when_specs_are_broken() {
        let root = temp_project();
        std::fs::write(
            root.join("specs/0001-auth.spec"),
            DEMO_SPEC.trim_start_matches('\n').replace(
                "scenarios: [valid_token]",
                "scenarios: [valid_token, nope_missing]",
            ),
        )
        .unwrap();

        // gen cannot codegen without a workspace: could not complete.
        assert_eq!(
            exit_code(cmd_gen(&root, None)),
            ExitCode::from(EXIT_CANNOT_COMPLETE)
        );

        std::fs::remove_dir_all(&root).unwrap();
    }

    // -----------------------------------------------------------------------
    // extract
    // -----------------------------------------------------------------------

    /// Temp project with a small Python layout (no specs yet).
    fn extract_fixture() -> PathBuf {
        let root = temp_project();
        std::fs::create_dir_all(root.join("api")).unwrap();
        std::fs::write(root.join("api/__init__.py"), "").unwrap();
        std::fs::write(root.join("api/views.py"), "import auth\n").unwrap();
        std::fs::create_dir_all(root.join("auth")).unwrap();
        std::fs::write(root.join("auth/__init__.py"), "").unwrap();
        root
    }

    #[test]
    fn extract_writes_draft_and_refuses_to_overwrite() {
        let root = extract_fixture();

        // First run writes specs/extracted.spec and succeeds.
        assert_eq!(cmd_extract(&root, false, None).unwrap(), ExitCode::from(EXIT_OK));
        let draft = root.join("specs/extracted.spec");
        let text = std::fs::read_to_string(&draft).unwrap();
        assert!(text.starts_with("# DRAFT from `decispec extract`"));
        assert!(text.contains("container api \"api\""));
        assert!(text.contains("rel api -> auth: \"api imports auth\""));

        // Second run must NOT overwrite: could not complete, file untouched.
        assert_eq!(
            exit_code(cmd_extract(&root, false, None)),
            ExitCode::from(EXIT_CANNOT_COMPLETE)
        );
        assert_eq!(std::fs::read_to_string(&draft).unwrap(), text);

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn extract_stdout_writes_no_file() {
        let root = extract_fixture();
        // --stdout succeeds and never touches specs/extracted.spec.
        assert_eq!(cmd_extract(&root, true, None).unwrap(), ExitCode::from(EXIT_OK));
        assert!(!root.join("specs/extracted.spec").exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn extract_with_no_source_files_is_cannot_complete() {
        // specs/ + decispec.toml but no source files at all.
        let root = temp_project();
        assert_eq!(
            exit_code(cmd_extract(&root, false, None)),
            ExitCode::from(EXIT_CANNOT_COMPLETE)
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn extract_with_missing_path_arg_is_cannot_complete() {
        let root = extract_fixture();
        let missing = root.join("no-such-dir");
        assert_eq!(
            exit_code(run(Cli {
                command: CommandKind::Extract {
                    path: Some(missing),
                    stdout: false,
                    to: None,
                },
            })),
            ExitCode::from(EXIT_CANNOT_COMPLETE)
        );
        // An existing file (not a directory) is equally unusable.
        let file = root.join("api/__init__.py");
        assert_eq!(
            exit_code(run(Cli {
                command: CommandKind::Extract {
                    path: Some(file),
                    stdout: false,
                    to: None,
                },
            })),
            ExitCode::from(EXIT_CANNOT_COMPLETE)
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Temp project with a small TypeScript layout plus the scaffold that
    /// `decispec init` writes (tests/glue glue package).
    fn extract_ts_fixture() -> PathBuf {
        let root = temp_project();
        std::fs::create_dir_all(root.join("pages")).unwrap();
        std::fs::write(
            root.join("pages/index.ts"),
            "import { h } from '../lib/h';\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("lib")).unwrap();
        std::fs::write(root.join("lib/h.ts"), "").unwrap();
        std::fs::create_dir_all(root.join("tests/glue")).unwrap();
        std::fs::write(root.join("tests/glue/__init__.py"), "").unwrap();
        std::fs::write(root.join("tests/glue/conftest.py"), "").unwrap();
        root
    }

    #[test]
    fn extract_ignores_decispec_scaffold_when_detecting_language() {
        let root = extract_ts_fixture();

        assert_eq!(cmd_extract(&root, false, None).unwrap(), ExitCode::from(EXIT_OK));
        let text = std::fs::read_to_string(root.join("specs/extracted.spec")).unwrap();
        // Without scaffold exclusions the 2 glue .py files tie the 2 .ts
        // files and the draft flips to Python with a lone `tests` container.
        assert!(
            text.contains("container lib \"lib\""),
            "real containers missing from draft:\n{text}"
        );
        assert!(
            text.contains("container pages \"pages\""),
            "real containers missing from draft:\n{text}"
        );
        assert!(
            !text.contains("container tests"),
            "DecisionSpec scaffold leaked into the draft:\n{text}"
        );

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn extract_ignores_glue_scaffold_in_python_projects() {
        let root = extract_fixture();
        // What `decispec init` writes into a Python project.
        std::fs::create_dir_all(root.join("tests/glue")).unwrap();
        std::fs::write(root.join("tests/glue/__init__.py"), "def f():\n    pass\n").unwrap();
        std::fs::write(root.join("tests/glue/conftest.py"), "").unwrap();

        assert_eq!(cmd_extract(&root, false, None).unwrap(), ExitCode::from(EXIT_OK));
        let text = std::fs::read_to_string(root.join("specs/extracted.spec")).unwrap();
        assert!(
            text.contains("container api \"api\""),
            "real containers missing from draft:\n{text}"
        );
        assert!(
            text.contains("container auth \"auth\""),
            "real containers missing from draft:\n{text}"
        );
        assert!(
            !text.contains("container tests"),
            "DecisionSpec scaffold leaked into the draft:\n{text}"
        );

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn extract_writes_draft_with_decisions_from_adrs() {
        let root = extract_fixture();
        std::fs::create_dir_all(root.join("docs/adr")).unwrap();
        std::fs::write(
            root.join("docs/adr/0001-use-postgres.md"),
            "# Use Postgres\n\n## Status\nAccepted\n\n## Context\nWe need a store.\n",
        )
        .unwrap();

        assert_eq!(cmd_extract(&root, false, None).unwrap(), ExitCode::from(EXIT_OK));
        let text = std::fs::read_to_string(root.join("specs/extracted.spec")).unwrap();
        assert!(
            text.contains("# Source: docs/adr/0001-use-postgres.md"),
            "ADR source comment missing:\n{text}"
        );
        assert!(
            text.contains("decision ADR-0001 \"Use Postgres\" {"),
            "decision block missing:\n{text}"
        );
        assert!(text.contains("  status: accepted\n"), "{text}");
        assert!(text.contains("  context: \"We need a store.\"\n"), "{text}");
        assert!(text.contains("model {\n"), "{text}");
        // decisions render before the model block
        assert!(
            text.find("decision ADR-0001").unwrap() < text.find("model {").unwrap(),
            "{text}"
        );
        // the draft is a valid workspace
        assert_eq!(cmd_check(&root, false).unwrap(), ExitCode::from(EXIT_OK));

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn mcp_subcommand_parses_and_dispatches() {
        // The stdio loop itself is covered by decispec-mcp's Cursor tests;
        // running cmd_mcp here would block on the test harness's stdin.
        let cli = Cli::try_parse_from(["decispec", "mcp"]).unwrap();
        assert!(matches!(cli.command, CommandKind::Mcp));
    }

    #[test]
    fn index_writes_decisions_md_and_exits_0() {
        let root = temp_project();
        std::fs::write(
            root.join("specs/0001-auth.spec"),
            DEMO_SPEC.trim_start_matches('\n'),
        )
        .unwrap();

        assert_eq!(cmd_index(&root, None).unwrap(), ExitCode::from(EXIT_OK));
        let text = std::fs::read_to_string(root.join(".decispec/docs/decisions.md")).unwrap();
        assert!(text.contains("# Decision Index"));
        assert!(text.contains("```mermaid"));
        assert!(text.contains("## AUTH-001 — Use OAuth2"));

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn index_honors_out_override() {
        let root = temp_project();
        std::fs::write(
            root.join("specs/0001-auth.spec"),
            DEMO_SPEC.trim_start_matches('\n'),
        )
        .unwrap();

        assert_eq!(
            cmd_index(&root, Some(PathBuf::from("out/index.md"))).unwrap(),
            ExitCode::from(EXIT_OK)
        );
        let text = std::fs::read_to_string(root.join("out/index.md")).unwrap();
        assert!(text.contains("# Decision Index"));

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn index_exits_2_when_specs_are_broken() {
        let root = temp_project();
        std::fs::write(
            root.join("specs/0001-auth.spec"),
            DEMO_SPEC.trim_start_matches('\n').replace(
                "scenarios: [valid_token]",
                "scenarios: [valid_token, nope_missing]",
            ),
        )
        .unwrap();

        // index needs a valid workspace like gen/gate: could not complete,
        // and it must not write a half-built index.
        assert_eq!(
            exit_code(cmd_index(&root, None)),
            ExitCode::from(EXIT_CANNOT_COMPLETE)
        );
        assert!(!root.join(".decispec/docs/decisions.md").exists());
        assert!(!root.join("docs/decisions.md").exists());

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn extract_without_project_root_is_cannot_complete() {
        // Serialize against any other cwd-sensitive test.
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = LOCK.lock().unwrap();
        let root = temp_project();
        let bare = root.join("bare");
        std::fs::create_dir_all(&bare).unwrap();
        let original = std::env::current_dir().unwrap();

        std::env::set_current_dir(&bare).unwrap();
        let code = exit_code(run(Cli {
            command: CommandKind::Extract {
                path: None,
                stdout: false,
                to: None,
            },
        }));
        std::env::set_current_dir(&original).unwrap();

        assert_eq!(code, ExitCode::from(EXIT_CANNOT_COMPLETE));
        std::fs::remove_dir_all(&root).unwrap();
    }

    // -----------------------------------------------------------------------
    // tui (F31)
    // -----------------------------------------------------------------------

    #[test]
    fn tui_register_adds_project_and_persists_registry() {
        let root = temp_project(); // temp_project writes decispec.toml name="t"
        let reg = root.join("config/registry.toml");

        let entry = register_project(&reg, &root).unwrap();
        assert_eq!(entry.name, "t");
        let canonical = root.canonicalize().unwrap();
        assert_eq!(entry.path, canonical.to_string_lossy());

        // re-registering updates in place, never duplicates
        let entry2 = register_project(&reg, &root).unwrap();
        let loaded = read_registry(&reg).unwrap();
        assert_eq!(loaded.projects.len(), 1);
        assert_eq!(loaded.projects[0], entry2);

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn tui_register_rejects_a_non_project_dir() {
        let root = temp_project();
        let bare = root.join("bare");
        std::fs::create_dir_all(&bare).unwrap();
        let reg = root.join("config/registry.toml");
        assert!(register_project(&reg, &bare).is_err());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn tui_registry_seeding_suggests_only_existing_dirs() {
        let home = temp_project(); // reused as a fake $HOME
        std::fs::create_dir_all(home.join("Projects/FPF/tvgo")).unwrap();
        let seeds = default_seed_entries(&home);
        assert_eq!(seeds.len(), 1);
        assert_eq!(seeds[0].name, "tvgo");
        // verydian-app does not exist under this home -> not suggested
        assert!(!seeds.iter().any(|p| p.name == "verydian"));
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn tui_load_or_seed_writes_registry_on_first_run() {
        let root = temp_project();
        let reg = root.join("config/registry.toml");
        // no seeds on this machine -> first run persists an empty registry
        let loaded = load_or_seed_registry(&reg, vec![]).unwrap();
        assert!(loaded.projects.is_empty());
        assert!(reg.is_file(), "first run persists the registry file");
        assert_eq!(load_or_seed_registry(&reg, vec![]).unwrap(), loaded);

        // with seeds, first run persists them
        let reg2 = root.join("config2/registry.toml");
        let seeds = vec![decispec_tui::registry::ProjectEntry {
            name: "x".to_string(),
            path: "/tmp/x".to_string(),
        }];
        let loaded2 = load_or_seed_registry(&reg2, seeds.clone()).unwrap();
        assert_eq!(loaded2.projects, seeds);
        assert!(reg2.is_file());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn tui_load_project_keeps_broken_specs_as_error_state() {
        let root = temp_project();
        std::fs::write(
            root.join("specs/0001-auth.spec"),
            DEMO_SPEC.trim_start_matches('\n').replace(
                "scenarios: [valid_token]",
                "scenarios: [valid_token, nope_missing]",
            ),
        )
        .unwrap();
        let entry = decispec_tui::registry::ProjectEntry {
            name: "broken".to_string(),
            path: root.to_string_lossy().to_string(),
        };
        let data = load_tui_project(&entry);
        assert!(data.workspace.is_none());
        let err = data.load_error.expect("error state");
        assert!(err.contains("nope_missing"), "error: {err}");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn tui_load_project_reads_gate_report_when_present() {
        let root = temp_project();
        std::fs::write(
            root.join("specs/0001-auth.spec"),
            DEMO_SPEC.trim_start_matches('\n'),
        )
        .unwrap();
        std::fs::create_dir_all(root.join(".decispec")).unwrap();
        std::fs::write(
            root.join(".decispec/report.json"),
            r#"{"project":"t","verdict":"pass","counts":{"rows":1}}"#,
        )
        .unwrap();
        let entry = decispec_tui::registry::ProjectEntry {
            name: "ok".to_string(),
            path: root.to_string_lossy().to_string(),
        };
        let data = load_tui_project(&entry);
        assert!(data.workspace.is_some());
        let gate = data.gate.expect("gate snapshot");
        assert_eq!(gate.verdict, decispec_tui::snapshot::Verdict::Pass);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn tui_translate_event_maps_presses_only() {
        use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
        let press = |code: KeyCode| {
            Event::Key(KeyEvent {
                code,
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: crossterm::event::KeyEventState::NONE,
            })
        };
        assert_eq!(
            translate_event(press(KeyCode::Char('q'))),
            Some(Input::Char('q'))
        );
        assert_eq!(translate_event(press(KeyCode::Down)), Some(Input::Down));
        assert_eq!(translate_event(press(KeyCode::Tab)), Some(Input::Tab));
        // release events are ignored (no double-firing on some terminals)
        let release = Event::Key(KeyEvent {
            code: KeyCode::Char('q'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Release,
            state: crossterm::event::KeyEventState::NONE,
        });
        assert_eq!(translate_event(release), None);
    }

    #[test]
    fn tui_subcommand_parses() {
        let cli = Cli::try_parse_from(["decispec", "tui"]).unwrap();
        assert!(matches!(
            cli.command,
            CommandKind::Tui { register: None }
        ));
        let cli = Cli::try_parse_from(["decispec", "tui", "--register", "/tmp/x"]).unwrap();
        assert!(matches!(
            cli.command,
            CommandKind::Tui {
                register: Some(_)
            }
        ));
    }

    #[test]
    fn tui_expand_home_leaves_absolute_and_plain_paths_alone() {
        assert_eq!(expand_home("/abs/path"), PathBuf::from("/abs/path"));
        assert_eq!(expand_home("relative/path"), PathBuf::from("relative/path"));
        // "~/" expands against $HOME when set, and never panics when it is not
        let expanded = expand_home("~/somewhere");
        if std::env::var_os("HOME").is_some() {
            assert!(expanded.ends_with("somewhere"));
            assert!(!expanded.starts_with("~"));
        } else {
            assert_eq!(expanded, PathBuf::from("~/somewhere"));
        }
    }

    // -- F32: editor save + PR ------------------------------------------------

    #[test]
    fn translate_event_maps_editor_keys_and_ctrl() {
        use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
        let key = |code: KeyCode, mods: KeyModifiers| {
            Event::Key(KeyEvent {
                code,
                modifiers: mods,
                kind: KeyEventKind::Press,
                state: crossterm::event::KeyEventState::NONE,
            })
        };
        assert_eq!(
            translate_event(key(KeyCode::Char('s'), KeyModifiers::CONTROL)),
            Some(Input::Ctrl('s'))
        );
        assert_eq!(
            translate_event(key(KeyCode::Char('S'), KeyModifiers::CONTROL)),
            Some(Input::Ctrl('s'))
        );
        assert_eq!(
            translate_event(key(KeyCode::Backspace, KeyModifiers::NONE)),
            Some(Input::Backspace)
        );
        assert_eq!(
            translate_event(key(KeyCode::Delete, KeyModifiers::NONE)),
            Some(Input::Delete)
        );
        assert_eq!(
            translate_event(key(KeyCode::Home, KeyModifiers::NONE)),
            Some(Input::Home)
        );
        assert_eq!(
            translate_event(key(KeyCode::End, KeyModifiers::NONE)),
            Some(Input::End)
        );
        assert_eq!(
            translate_event(key(KeyCode::PageUp, KeyModifiers::NONE)),
            Some(Input::PageUp)
        );
        assert_eq!(
            translate_event(key(KeyCode::PageDown, KeyModifiers::NONE)),
            Some(Input::PageDown)
        );
        // plain chars stay plain even with SHIFT
        assert_eq!(
            translate_event(key(KeyCode::Char('A'), KeyModifiers::SHIFT)),
            Some(Input::Char('A'))
        );
    }

    #[test]
    fn save_spec_file_writes_and_reports_workspace_health() {
        let root = temp_project();
        std::fs::write(
            root.join("specs/0001-auth.spec"),
            DEMO_SPEC.trim_start_matches('\n'),
        )
        .unwrap();

        // valid edit: writes + summary
        let text = format!("{}\n# cockpit note\n", DEMO_SPEC.trim_start_matches('\n'));
        let msg = save_spec_file(&root, "specs/0001-auth.spec", &text).unwrap();
        assert!(msg.contains("saved specs/0001-auth.spec"), "msg: {msg}");
        assert!(msg.contains("1 decision(s)"), "msg: {msg}");
        assert_eq!(std::fs::read_to_string(root.join("specs/0001-auth.spec")).unwrap(), text);

        // broken edit: still writes, reported as a loud warning (not hidden,
        // not a double-barrelled "save failed")
        let broken = text.replace("scenarios: [valid_token]", "scenarios: [valid_token, nope]");
        let msg = save_spec_file(&root, "specs/0001-auth.spec", &broken).unwrap();
        assert!(msg.contains("WARNING"), "msg: {msg}");
        assert!(msg.contains("nope"), "msg: {msg}");
        assert_eq!(
            std::fs::read_to_string(root.join("specs/0001-auth.spec")).unwrap(),
            broken
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn save_spec_file_creates_new_file_and_parent_dirs() {
        let root = temp_project();
        // brand-new file in a subdir that does not exist yet
        let msg = save_spec_file(&root, "specs/payments/010-new.spec", "# new\n").unwrap();
        assert!(msg.contains("saved specs/payments/010-new.spec"), "msg: {msg}");
        assert_eq!(
            std::fs::read_to_string(root.join("specs/payments/010-new.spec")).unwrap(),
            "# new\n"
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn save_spec_file_refuses_paths_outside_specs() {
        let root = temp_project();
        // clean any leftover from a pre-hardening red run
        let _ = std::fs::remove_file("/tmp/decispec-evil.spec");
        let _ = std::fs::remove_file(root.parent().unwrap().join("evil.spec"));
        for evil in ["../evil.spec", "specs/../evil.spec", "/tmp/decispec-evil.spec"] {
            let err = save_spec_file(&root, evil, "nope\n").unwrap_err();
            assert!(err.contains("outside specs"), "err for {evil}: {err}");
        }
        // nothing escaped
        assert!(!root.parent().unwrap().join("evil.spec").exists());
        assert!(!Path::new("/tmp/decispec-evil.spec").exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn pr_readiness_flags_non_git_clean_tree_and_missing_gh() {
        let root = temp_project();
        std::fs::write(
            root.join("specs/0001-auth.spec"),
            DEMO_SPEC.trim_start_matches('\n'),
        )
        .unwrap();
        // not a git repo
        let err = pr_readiness(&root, true).unwrap_err();
        assert!(err.contains("not a git repository"), "err: {err}");

        let work = git_fixture(&root);
        // clean tree: nothing to PR
        let err = pr_readiness(&work, true).unwrap_err();
        assert!(err.contains("no uncommitted changes"), "err: {err}");
        // dirty tree but no gh
        std::fs::write(work.join("specs/0001-auth.spec"), "# touched\n").unwrap();
        let err = pr_readiness(&work, false).unwrap_err();
        assert!(err.contains("gh"), "err: {err}");
        // dirty tree + gh -> ready
        assert!(pr_readiness(&work, true).is_ok());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn pr_branch_name_is_utc_timestamped() {
        // 2000-01-01 00:00:00 UTC = 10957 days after the epoch
        let y2k = std::time::UNIX_EPOCH + std::time::Duration::from_secs(10_957 * 86_400);
        assert_eq!(pr_branch_name(y2k), "spec/tui-20000101-000000");
        // 2026-10-08 07:38:07 UTC (verified with `date -u -r 1791445087`)
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_791_445_087);
        assert_eq!(pr_branch_name(t), "spec/tui-20261008-073807");
    }

    #[test]
    fn create_spec_pr_commits_pushes_then_reports_gh_failure() {
        let root = temp_project();
        std::fs::write(
            root.join("specs/0001-auth.spec"),
            DEMO_SPEC.trim_start_matches('\n'),
        )
        .unwrap();
        let work = git_fixture(&root);
        // a spec edit (the fixture copied the pristine spec into work) plus a
        // non-spec file: `git add specs/` must stage ONLY the spec, so an
        // accidental widening to `git add -A` fails this assertion.
        let spec = work.join("specs/0001-auth.spec");
        let text = format!(
            "{}\n# cockpit edit\n",
            DEMO_SPEC.trim_start_matches('\n')
        );
        std::fs::write(&spec, &text).unwrap();
        std::fs::write(work.join("notes.txt"), "not for the PR\n").unwrap();

        // gh_ok injected true: the git flow runs everywhere; the gh step
        // itself fails in every environment (missing binary, or a non-GitHub
        // remote) and must be named in the error.
        let err = create_spec_pr_with(&work, "demo", true).unwrap_err();
        assert!(err.contains("gh"), "err: {err}");

        let branch = git_output(&work, &["branch", "--show-current"]).unwrap();
        assert!(branch.starts_with("spec/tui-"), "branch: {branch}");
        // the branch (and only specs/) reached the origin
        let log = std::process::Command::new("git")
            .args(["log", "--oneline", "-1", branch.trim()])
            .current_dir(root.join("origin.git"))
            .output()
            .unwrap();
        assert!(log.status.success(), "origin log: {:?}", log);
        let msg = String::from_utf8_lossy(&log.stdout);
        assert!(msg.contains("spec: demo edits"), "msg: {msg}");
        // the commit contains exactly one file: the spec (never notes.txt)
        let names = git_output(&work, &["diff-tree", "--no-commit-id", "--name-only", "-r", "HEAD"]).unwrap();
        assert_eq!(names.trim(), "specs/0001-auth.spec", "names: {names}");
        // the non-spec change stayed uncommitted in the worktree
        let status = git_output(&work, &["status", "--porcelain"]).unwrap();
        assert!(status.contains("notes.txt"), "status: {status}");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn create_spec_pr_refuses_changes_outside_specs_and_restores() {
        let root = temp_project();
        std::fs::write(
            root.join("specs/0001-auth.spec"),
            DEMO_SPEC.trim_start_matches('\n'),
        )
        .unwrap();
        let work = git_fixture(&root);
        // only a non-spec change: nothing to PR -> refusal, worktree restored
        std::fs::write(work.join("notes.txt"), "nothing spec here\n").unwrap();

        let err = create_spec_pr_with(&work, "demo", true).unwrap_err();
        assert!(err.contains("outside specs/"), "err: {err}");

        let branch = git_output(&work, &["branch", "--show-current"]).unwrap();
        assert!(!branch.starts_with("spec/tui-"), "still on {branch}");
        let leftover = git_output(&work, &["branch", "--list", "spec/tui-*"]).unwrap();
        assert!(leftover.trim().is_empty(), "leftover: {leftover}");
        // the change survives, uncommitted, ready for a human
        let status = git_output(&work, &["status", "--porcelain"]).unwrap();
        assert!(status.contains("notes.txt"), "status: {status}");
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// A git repo with a local bare `origin`, one commit, and local identity —
    /// enough to exercise branch/commit/push without a network.
    fn git_fixture(root: &Path) -> PathBuf {
        let origin = root.join("origin.git");
        let work = root.join("work");
        std::fs::create_dir_all(&work).unwrap();
        std::fs::create_dir_all(work.join("specs")).unwrap();
        std::fs::copy(root.join("decispec.toml"), work.join("decispec.toml")).unwrap();
        std::fs::copy(
            root.join("specs/0001-auth.spec"),
            work.join("specs/0001-auth.spec"),
        )
        .unwrap();
        let git_in = |dir: &Path, args: &[&str]| {
            let st = std::process::Command::new("git")
                .args(args)
                .current_dir(dir)
                .status()
                .unwrap();
            assert!(st.success(), "git {} failed", args.join(" "));
        };
        git_in(root, &["init", "--bare", "origin.git"]);
        git_in(&work, &["init", "."]);
        git_in(&work, &["config", "user.email", "t@example.com"]);
        git_in(&work, &["config", "user.name", "t"]);
        git_in(&work, &["add", "-A"]);
        git_in(&work, &["commit", "-m", "init"]);
        git_in(
            &work,
            &["remote", "add", "origin", &origin.to_string_lossy()],
        );
        work
    }
}
