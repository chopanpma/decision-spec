//! `decispec` binary: init / check / gen / test / gate / query.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use clap::Parser;
use decispec_codegen::{CodegenConfig, render_all};
use decispec_gate::{
    AdapterRecord, AdapterStatus, GateInput, Layer as GateLayer, Manifest, Verdict,
};
use decispec_ir::{Layer, Workspace};
use decispec_store::{DecisionRun, RunRecord, Store};

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
    },
    /// Parse specs/**/*.spec and report errors (dup IDs, unknown refs, ...)
    Check,
    /// Generate diagrams and test artifacts from the IR
    Gen,
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
        /// Treat SKIPPED rows (toolchain never installed) as FAIL
        #[arg(long)]
        strict_skipped: bool,
    },
    /// Print the decision (or requirement/scenario) subtree: the agent view
    Query { id: String },
    /// Bootstrap a draft spec from an existing Python/JS/TS project
    /// (containers from the directory layout, rels from the import graph)
    Extract {
        /// Project directory to analyze (default: the DecisionSpec project root
        /// found by walking up from cwd)
        path: Option<PathBuf>,
        /// Print the draft spec to stdout instead of writing specs/extracted.spec
        #[arg(long)]
        stdout: bool,
    },
    /// Serve MCP over stdio (NDJSON JSON-RPC 2.0) for AI agents
    Mcp,
    /// Write a single-file, GitHub-renderable decision index (markdown with
    /// embedded mermaid diagrams and decision tables)
    Index {
        /// Output path relative to the project root (default: docs/decisions.md)
        #[arg(long)]
        out: Option<PathBuf>,
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

fn cmd_init(demo: bool) -> CliResult<ExitCode> {
    let root = std::env::current_dir()?;
    if root.join("decispec.toml").exists() {
        return Err(CliError::Msg(format!(
            "{} already contains a decispec.toml",
            root.display()
        )));
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

    if demo {
        std::fs::write(
            root.join("specs/0001-auth.spec"),
            DEMO_SPEC.trim_start_matches('\n'),
        )?;
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
    println!("next: decispec check && decispec gen");
    Ok(ExitCode::from(EXIT_OK))
}

// ---------------------------------------------------------------------------
// check
// ---------------------------------------------------------------------------

fn cmd_check(root: &Path) -> CliResult<ExitCode> {
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

fn cmd_gen(root: &Path) -> CliResult<ExitCode> {
    let ws = load_workspace_or_bail(root)?;
    let cfg = load_config(root)?;
    let removed = write_artifacts(root, &ws, &cfg)?;
    for r in &removed {
        println!("removed stale {r}");
    }
    Ok(ExitCode::from(EXIT_OK))
}

/// Render `ws`, write every artifact under `root`, then (only after all
/// writes succeeded) sweep stale generated files. Returns the sorted
/// project-relative paths of removed stale artifacts.
fn write_artifacts(root: &Path, ws: &Workspace, cfg: &Config) -> CliResult<Vec<String>> {
    let files = render_all(
        ws,
        &CodegenConfig {
            lang: cfg.lang.clone(),
            glue_module: cfg.glue_module.clone(),
            rust_glue: cfg.rust_glue.clone(),
            root_package: cfg.root_package.clone(),
            container_packages: cfg.container_packages.clone(),
        },
    );
    let mut diagrams = 0usize;
    let mut tests = 0usize;
    for f in &files {
        let path = root.join(&f.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, &f.content)?;
        if f.path.starts_with("docs/diagrams/") {
            diagrams += 1;
        } else {
            tests += 1;
        }
        println!("wrote {}", f.path);
    }
    let removed = sweep_stale_artifacts(root, &files)?;
    println!(
        "done: {diagrams} diagram file(s) under docs/diagrams/, {tests} test artifact(s) under tests/generated/ (tests/glue/ untouched)"
    );
    Ok(removed)
}

/// Delete generated artifacts whose spec item disappeared: files under the
/// managed trees (`docs/diagrams/`, `tests/generated/<layer>/`) that carry
/// the GENERATED header but are not in the current render set. Only sweep
/// directories gen manages; never create them. Returns the sorted removed
/// paths (project-relative, forward slashes).
fn sweep_stale_artifacts(
    root: &Path,
    files: &[decispec_codegen::GeneratedFile],
) -> CliResult<Vec<String>> {
    let rendered: std::collections::HashSet<&str> = files.iter().map(|f| f.path.as_str()).collect();
    let mut removed = Vec::new();
    // gen writes flat files under docs/diagrams/.
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

fn run_cmd(root: &Path, program: &str, args: &[&str]) -> CliResult<CmdOutput> {
    let out = Command::new(program)
        .args(args)
        .current_dir(root)
        .output()?;
    Ok(CmdOutput {
        ok: out.status.success(),
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
    })
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
fn run_adapters(root: &Path) -> CliResult<Manifest> {
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
        let probe = run_cmd(root, "python3", &["-m", "pytest", "--version"])?;
        if !probe.ok {
            adapters.push(skipped(
                "pytest",
                GateLayer::Unit,
                "toolchain not installed (python3 -m pytest)",
            ));
        } else {
            let xml_rel = "tests/generated/results/pytest.xml";
            let out = run_cmd(
                root,
                "python3",
                &[
                    "-m",
                    "pytest",
                    "tests/generated/unit",
                    "-q",
                    "--junitxml",
                    xml_rel,
                ],
            )?;
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
    if !dir_has_files_with_ext(&contract_dir, "feature") {
        adapters.push(skipped(
            "cucumber",
            GateLayer::Contract,
            "no generated contract tests",
        ));
    } else if on_path("cucumber") {
        let xml_rel = "tests/generated/results/contract.xml";
        let out = run_cmd(
            root,
            "cucumber",
            &[
                "tests/generated/contract",
                "--format",
                "junit",
                "--out",
                xml_rel,
            ],
        )?;
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
    } else if on_path("karate") {
        let out = run_cmd(root, "karate", &["tests/generated/contract"])?;
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
                Some(format!("karate exited non-zero: {}", tail(&out.stderr, 3)))
            },
            result_files,
        });
    } else {
        adapters.push(skipped(
            "cucumber",
            GateLayer::Contract,
            "toolchain not installed (cucumber or karate)",
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
        let probe = run_cmd(root, "npx", &["--no-install", "playwright", "--version"])?;
        if !probe.ok {
            adapters.push(skipped(
                "playwright",
                GateLayer::E2e,
                "toolchain not installed (npx playwright)",
            ));
        } else {
            let out = run_cmd(
                root,
                "npx",
                &[
                    "--no-install",
                    "playwright",
                    "test",
                    "tests/generated/e2e",
                    "--reporter=junit",
                ],
            )?;
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
    if !dir_has_files_with_ext(&infra_dir, "rego") {
        adapters.push(skipped(
            "conftest",
            GateLayer::Infra,
            "no generated infra policies",
        ));
    } else if on_path("conftest") {
        let out = run_cmd(root, "conftest", &["test", "tests/generated/infra"])?;
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
            "toolchain not installed (conftest)",
        ));
    }

    // fitness / import-linter + dependency-cruiser
    let fitness_dir = root.join("tests/generated/fitness");
    let has_fitness = fitness_dir.join("dependency-cruiser.cjs").is_file();
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
        if on_path("lint-imports") {
            let out = run_cmd(
                root,
                "lint-imports",
                &["--config", "tests/generated/fitness/.importlinter"],
            )?;
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
                "toolchain not installed (lint-imports)",
            ));
        }
        if on_path("depcruise") {
            if root.join("src").is_dir() {
                let out = run_cmd(
                    root,
                    "depcruise",
                    &[
                        "--config",
                        "tests/generated/fitness/dependency-cruiser.cjs",
                        "src",
                    ],
                )?;
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
                "toolchain not installed (depcruise)",
            ));
        }
    }

    Ok(Manifest { adapters })
}

fn cmd_test(root: &Path) -> CliResult<ExitCode> {
    let manifest = run_adapters(root)?;
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

fn print_matrix(report: &decispec_gate::GateReport) {
    println!("═══ DECISIONSPEC GATE ═══ project: {}", report.project);
    let rows: Vec<&decispec_gate::MatrixRow> = report
        .decisions
        .iter()
        .flat_map(|d| d.rows.iter())
        .collect();
    if rows.is_empty() {
        println!("(no traceability rows)");
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
    for d in &report.decisions {
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
}

fn cmd_gate(
    root: &Path,
    json: bool,
    results: Option<PathBuf>,
    strict_skipped: bool,
) -> CliResult<ExitCode> {
    let cfg = load_config(root)?;
    let ws = load_workspace_or_bail(root)?;

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
            let manifest = run_adapters(root)?;
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

    print_matrix(&report);
    println!(".decispec/report.json written; run recorded in .decispec/decispec.db");
    if json {
        println!("{}", report_json);
    }
    Ok(if report.verdict == Verdict::Pass {
        ExitCode::from(EXIT_OK)
    } else {
        ExitCode::from(EXIT_FINDINGS)
    })
}

// ---------------------------------------------------------------------------
// query
// ---------------------------------------------------------------------------

fn cmd_query(root: &Path, id: &str) -> CliResult<ExitCode> {
    let ws = match load_workspace(root) {
        Ok(ws) => ws,
        Err(errors) => {
            print_errors(&errors);
            return Ok(ExitCode::from(EXIT_FINDINGS));
        }
    };

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
                "    {} -> docs/diagrams/{}.mmd, docs/diagrams/{}.puml",
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

fn cmd_extract(root: &Path, stdout: bool) -> CliResult<ExitCode> {
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
// main
// ---------------------------------------------------------------------------

fn run(cli: Cli) -> CliResult<ExitCode> {
    match cli.command {
        CommandKind::Init { demo } => cmd_init(demo),
        CommandKind::Check => {
            let root = find_project_root()?;
            cmd_check(&root)
        }
        CommandKind::Gen => {
            let root = find_project_root()?;
            cmd_gen(&root)
        }
        CommandKind::Test => {
            let root = find_project_root()?;
            cmd_test(&root)
        }
        CommandKind::Gate {
            json,
            results,
            strict_skipped,
        } => {
            let root = find_project_root()?;
            cmd_gate(&root, json, results, strict_skipped)
        }
        CommandKind::Query { id } => {
            let root = find_project_root()?;
            cmd_query(&root, &id)
        }
        CommandKind::Extract { path, stdout } => {
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
            cmd_extract(&root, stdout)
        }
        CommandKind::Mcp => cmd_mcp(),
        CommandKind::Index { out } => {
            let root = find_project_root()?;
            cmd_index(&root, out)
        }
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
        let removed = write_artifacts(&root, &ws, &cfg).unwrap();
        assert!(removed.is_empty(), "first gen must remove nothing");

        let stale_py = root.join("tests/generated/unit/test_AUTH_001_invalid_token.py");
        let stale_mmd = root.join("docs/diagrams/svc_auth.mmd");
        let stale_puml = root.join("docs/diagrams/svc_auth.puml");
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
        let hand_mmd = root.join("docs/diagrams/hand.mmd");
        std::fs::write(&hand_mmd, "sequenceDiagram\n    a->>b: x\n").unwrap();

        // gen #2: scenario removed, flow renamed.
        std::fs::write(
            root.join("specs/0001-auth.spec"),
            DEMO_SPEC_V2.trim_start_matches('\n'),
        )
        .unwrap();
        let ws = load_workspace(&root).expect("spec v2 should parse");
        let removed = write_artifacts(&root, &ws, &load_config(&root).unwrap()).unwrap();

        assert!(!stale_py.exists(), "stale unit test for removed scenario");
        assert!(!stale_mmd.exists(), "stale mermaid for renamed flow");
        assert!(!stale_puml.exists(), "stale plantuml for renamed flow");
        assert_eq!(
            removed,
            vec![
                "docs/diagrams/svc_auth.mmd".to_string(),
                "docs/diagrams/svc_auth.puml".to_string(),
                "tests/generated/unit/test_AUTH_001_invalid_token.py".to_string(),
            ]
        );
        assert!(root.join("docs/diagrams/svc_auth2.mmd").is_file());
        assert!(root.join("docs/diagrams/model.mmd").is_file());
        assert!(kept_py.is_file(), "rendered file must survive");
        assert!(kept_ts.is_file(), "rendered file must survive");
        assert!(glue.is_file(), "glue must survive");
        assert!(manifest.is_file(), "results/ must survive");
        assert!(hand_py.is_file(), "headerless file must survive");
        assert!(hand_mmd.is_file(), "headerless diagram must survive");

        // gen #3 identical: idempotent, bytes unchanged, nothing removed.
        let before = std::fs::read(&kept_ts).unwrap();
        let removed = write_artifacts(&root, &ws, &load_config(&root).unwrap()).unwrap();
        assert!(removed.is_empty(), "re-gen must remove nothing");
        assert_eq!(std::fs::read(&kept_ts).unwrap(), before);

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
            exit_code(cmd_gate(&root, false, Some(results), false)),
            ExitCode::from(EXIT_CANNOT_COMPLETE)
        );

        // check reports malformed specs by design (that IS its job), so 1.
        assert_eq!(cmd_check(&root).unwrap(), ExitCode::from(EXIT_FINDINGS));

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
            exit_code(cmd_gate(&root, false, Some(results), false)),
            ExitCode::from(EXIT_FINDINGS)
        );
        assert!(root.join(".decispec/report.json").is_file());

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
        write_artifacts(&root, &ws, &load_config(&root).unwrap()).unwrap();
        // Empty results dir: artifacts exist, but no toolchain ran.
        let results = root.join("tests/generated/results");
        std::fs::create_dir_all(&results).unwrap();

        // Default: skipped rows do not affect the verdict (exit 0).
        assert_eq!(
            exit_code(cmd_gate(&root, false, Some(results.clone()), false)),
            ExitCode::from(EXIT_OK)
        );
        // Strict: the same rows are findings, so exit 1 — no new exit code.
        assert_eq!(
            exit_code(cmd_gate(&root, false, Some(results), true)),
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
            exit_code(cmd_gen(&root)),
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
        assert_eq!(cmd_extract(&root, false).unwrap(), ExitCode::from(EXIT_OK));
        let draft = root.join("specs/extracted.spec");
        let text = std::fs::read_to_string(&draft).unwrap();
        assert!(text.starts_with("# DRAFT from `decispec extract`"));
        assert!(text.contains("container api \"api\""));
        assert!(text.contains("rel api -> auth: \"api imports auth\""));

        // Second run must NOT overwrite: could not complete, file untouched.
        assert_eq!(
            exit_code(cmd_extract(&root, false)),
            ExitCode::from(EXIT_CANNOT_COMPLETE)
        );
        assert_eq!(std::fs::read_to_string(&draft).unwrap(), text);

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn extract_stdout_writes_no_file() {
        let root = extract_fixture();
        // --stdout succeeds and never touches specs/extracted.spec.
        assert_eq!(cmd_extract(&root, true).unwrap(), ExitCode::from(EXIT_OK));
        assert!(!root.join("specs/extracted.spec").exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn extract_with_no_source_files_is_cannot_complete() {
        // specs/ + decispec.toml but no source files at all.
        let root = temp_project();
        assert_eq!(
            exit_code(cmd_extract(&root, false)),
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

        assert_eq!(cmd_extract(&root, false).unwrap(), ExitCode::from(EXIT_OK));
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

        assert_eq!(cmd_extract(&root, false).unwrap(), ExitCode::from(EXIT_OK));
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

        assert_eq!(cmd_extract(&root, false).unwrap(), ExitCode::from(EXIT_OK));
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
        assert_eq!(cmd_check(&root).unwrap(), ExitCode::from(EXIT_OK));

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
        let text = std::fs::read_to_string(root.join("docs/decisions.md")).unwrap();
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
            },
        }));
        std::env::set_current_dir(&original).unwrap();

        assert_eq!(code, ExitCode::from(EXIT_CANNOT_COMPLETE));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
