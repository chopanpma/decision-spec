//! MCP server over stdio (newline-delimited JSON-RPC 2.0) exposing
//! DecisionSpec tools to AI agents: validate spec edits with file:line
//! diagnostics, bootstrap drafts from existing projects, and read the
//! porting work queue — without parsing CLI text.
//!
//! The pure dispatcher [`handle_message`] holds all the logic and is
//! unit-testable without processes; [`serve`] is the thin stdio loop that
//! cli's `decispec mcp` subcommand runs.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{json, Value};

/// Per-connection state: the agent's cwd (set once) and the project root
/// resolved from it at `initialize` time.
pub struct Session {
    cwd: PathBuf,
    root: Option<PathBuf>,
}

impl Session {
    pub fn new(cwd: PathBuf) -> Self {
        Session { cwd, root: None }
    }
}

#[derive(Deserialize)]
struct Request {
    id: Option<Value>,
    method: String,
    #[allow(dead_code)]
    params: Option<Value>,
}

fn result_reply(id: Value, result: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string()
}

fn error_reply(id: Value, code: i64, message: &str) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
        .to_string()
}

/// Dispatch one NDJSON line. Returns `None` for notifications (a valid
/// JSON-RPC object without an `id`) — the caller writes nothing back.
pub fn handle_message(session: &mut Session, line: &str) -> Option<String> {
    let req: Request = match serde_json::from_str(line) {
        Ok(r) => r,
        Err(_) => return Some(error_reply(Value::Null, -32700, "parse error")),
    };
    let id = req.id.clone().unwrap_or(Value::Null);
    // Notification (`initialized` etc.): never replied to.
    req.id.as_ref()?;
    match req.method.as_str() {
        "initialize" => {
            session.root = find_project_root(&session.cwd);
            Some(result_reply(
                id,
                json!({
                    "protocolVersion": "2025-03-26",
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "decispec", "version": env!("CARGO_PKG_VERSION") },
                }),
            ))
        }
        "ping" => Some(result_reply(id, json!({}))),
        "tools/list" => Some(result_reply(id, json!({ "tools": tool_catalog() }))),
        "tools/call" => {
            let params = req.params.clone().unwrap_or(json!({}));
            let name = params["name"].as_str().unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            match call_tool(session, name, &args) {
                Ok(payload) => Some(result_reply(
                    id,
                    json!({ "content": [{ "type": "text", "text": payload.to_string() }] }),
                )),
                Err(message) => Some(result_reply(
                    id,
                    json!({
                        "content": [{ "type": "text", "text": message }],
                        "isError": true,
                    }),
                )),
            }
        }
        _ => Some(error_reply(id, -32601, "method not found")),
    }
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

/// The root a tool operates on: per-call `path` arg wins, else the root
/// resolved at initialize.
fn resolve_root(session: &Session, args: &Value) -> Result<PathBuf, String> {
    if let Some(p) = args.get("path").and_then(Value::as_str) {
        return find_project_root(Path::new(p))
            .ok_or_else(|| format!("no decispec.toml found walking up from {p}"));
    }
    session.root.clone().ok_or_else(|| {
        "no project root: no decispec.toml found from cwd at initialize; pass `path`".to_string()
    })
}

fn call_tool(session: &Session, name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "project_info" => tool_project_info(&resolve_root(session, args)?),
        "extract_project" => tool_extract_project(&resolve_root(session, args)?),
        "validate_spec" => tool_validate_spec(args),
        "check_workspace" => tool_check_workspace(&resolve_root(session, args)?),
        "workspace_overview" => tool_workspace_overview(&resolve_root(session, args)?),
        "gate_status" => tool_gate_status(&resolve_root(session, args)?),
        _ => Err(format!("unknown tool: {name}")),
    }
}

/// The six tools, with real JSON Schema input schemas.
fn tool_catalog() -> Value {
    let with_path = || {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Project directory (default: root resolved at initialize)" }
            },
            "required": [],
        })
    };
    json!([
        {
            "name": "project_info",
            "description": "Describe the DecisionSpec project: root, config (project name, stack lang, glue module, fitness packages) and spec/generated file counts. Use first to orient in an unfamiliar repo.",
            "inputSchema": with_path(),
        },
        {
            "name": "extract_project",
            "description": "Bootstrap a draft spec from an existing Python/JS/TS project: containers, import-graph relationships, ADR decisions, plus the rendered draft and fitness TOML. Use when porting a project that has no specs yet.",
            "inputSchema": with_path(),
        },
        {
            "name": "validate_spec",
            "description": "Parse and validate one .spec text and return file:line diagnostics as JSON without touching the filesystem. Use to iterate on a spec edit before writing it.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "text": { "type": "string", "description": "The .spec text to validate" },
                    "filename": { "type": "string", "description": "File name used in diagnostics (default: proposed.spec)" }
                },
                "required": ["text"],
            },
        },
        {
            "name": "check_workspace",
            "description": "Parse and validate every specs/**/*.spec file and return structured diagnostics. Use to check the whole workspace after edits.",
            "inputSchema": with_path(),
        },
        {
            "name": "workspace_overview",
            "description": "Summarize the workspace as a porting work queue: decisions, requirements with layers/scenarios, and gaps (accepted/proposed without requirements, requirements without scenarios, superseded self-references). Use to plan remaining spec work.",
            "inputSchema": with_path(),
        },
        {
            "name": "gate_status",
            "description": "Read the last .decispec/report.json gate verdict, counts, and failing/uncovered/skipped row summaries. Use to see compliance status without re-running the gate.",
            "inputSchema": with_path(),
        },
    ])
}

// ---------------------------------------------------------------------------
// Tool implementations
// ---------------------------------------------------------------------------

/// Minimal tolerant read of the flat decispec.toml shape (project name,
/// stack lang/glue_module, fitness packages). Duplicates cli's private
/// config parser on purpose — see the F16 claim.
#[derive(Default)]
struct McpConfig {
    name: String,
    lang: String,
    glue_module: String,
    packages: std::collections::BTreeMap<String, String>,
}

fn read_config(root: &Path) -> McpConfig {
    let mut cfg = McpConfig {
        name: "my-project".to_string(),
        lang: "python".to_string(),
        glue_module: "tests.glue".to_string(),
        ..Default::default()
    };
    let Ok(src) = std::fs::read_to_string(root.join("decispec.toml")) else {
        return cfg;
    };
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
            ("project", "name") => cfg.name = v,
            ("stack", "lang") => cfg.lang = v,
            ("stack", "glue_module") => cfg.glue_module = v,
            ("stack.fitness.packages", container) => {
                cfg.packages.insert(container.to_string(), v);
            }
            _ => {}
        }
    }
    cfg
}

/// All `specs/**/*.spec` files, sorted, as root-relative paths.
fn spec_files(root: &Path) -> Vec<PathBuf> {
    fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect(&path, out);
            } else if path.extension().is_some_and(|e| e == "spec") {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    collect(&root.join("specs"), &mut out);
    out.sort();
    out
}

fn count_files(dir: &Path) -> usize {
    fn collect(dir: &Path, out: &mut usize) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect(&path, out);
            } else {
                *out += 1;
            }
        }
    }
    let mut n = 0;
    collect(dir, &mut n);
    n
}

fn diag_json(d: &decispec_parse::Diagnostic) -> Value {
    json!({ "file": d.file, "line": d.line, "message": d.message })
}

/// Scaffold exclusions, mirroring `decispec extract`: tests/generated +
/// the glue package dir from config.
fn scaffold_exclusions(root: &Path) -> Vec<PathBuf> {
    let cfg = read_config(root);
    let mut exclude = vec![PathBuf::from("tests/generated")];
    let glue_dir = cfg.glue_module.replace('.', "/");
    if !glue_dir.is_empty() {
        exclude.push(PathBuf::from(glue_dir));
    }
    exclude
}

fn tool_project_info(root: &Path) -> Result<Value, String> {
    let cfg = read_config(root);
    Ok(json!({
        "root": root.to_string_lossy(),
        "project": cfg.name,
        "lang": cfg.lang,
        "glue_module": cfg.glue_module,
        "fitness_packages": cfg.packages,
        "files": {
            "specs": spec_files(root).len(),
            "generated": count_files(&root.join("tests/generated")),
        },
    }))
}

fn tool_extract_project(root: &Path) -> Result<Value, String> {
    let exclude = scaffold_exclusions(root);
    let ex = decispec_extract::analyze(root, &exclude).map_err(|e| e.to_string())?;
    let lang = match ex.lang {
        decispec_extract::Lang::Python => "python",
        decispec_extract::Lang::TypeScript => "typescript",
        decispec_extract::Lang::JavaScript => "javascript",
    };
    Ok(json!({
        "lang": lang,
        "project_name": ex.project_name,
        "containers": ex
            .containers
            .iter()
            .map(|c| json!({ "id": c.id, "title": c.title, "module": c.module }))
            .collect::<Vec<_>>(),
        "relationships": ex
            .relationships
            .iter()
            .map(|(from, to)| json!([from, to]))
            .collect::<Vec<_>>(),
        "decisions": ex
            .decisions
            .iter()
            .map(|d| json!({
                "id": d.id,
                "title": d.title,
                "status": d.status,
                "context": d.context,
                "consequences": d.consequences,
                "source": d.source,
            }))
            .collect::<Vec<_>>(),
        "warnings": ex.warnings,
        "spec": decispec_extract::render_spec(&ex),
        "fitness_toml": decispec_extract::render_fitness_toml(&ex),
    }))
}

fn tool_validate_spec(args: &Value) -> Result<Value, String> {
    let text = args["text"]
        .as_str()
        .ok_or_else(|| "validate_spec requires a `text` string argument".to_string())?;
    let filename = args
        .get("filename")
        .and_then(Value::as_str)
        .unwrap_or("proposed.spec");
    let mut diags = Vec::new();
    match decispec_parse::parse_file(filename, text) {
        Ok(unit) => {
            let ws = decispec_parse::merge(vec![unit]);
            for d in decispec_parse::validate(&ws) {
                diags.push(diag_json(&d));
            }
        }
        Err(e) => diags.push(json!({ "file": e.file, "line": e.line, "message": e.message })),
    }
    Ok(json!({
        "filename": filename,
        "valid": diags.is_empty(),
        "diagnostics": diags,
        "error_count": diags.len(),
    }))
}

/// Parse every spec file; returns (units, diagnostics-for-parse-failures).
fn load_specs(root: &Path) -> (Vec<decispec_parse::FileUnit>, Vec<Value>) {
    let mut units = Vec::new();
    let mut diags = Vec::new();
    for f in spec_files(root) {
        let rel = f
            .strip_prefix(root)
            .unwrap_or(&f)
            .to_string_lossy()
            .replace('\\', "/");
        let Ok(text) = std::fs::read_to_string(&f) else {
            diags.push(json!({ "file": rel, "line": 0, "message": "cannot read file" }));
            continue;
        };
        match decispec_parse::parse_file(&rel, &text) {
            Ok(u) => units.push(u),
            Err(e) => diags.push(json!({ "file": e.file, "line": e.line, "message": e.message })),
        }
    }
    (units, diags)
}

fn tool_check_workspace(root: &Path) -> Result<Value, String> {
    let files = spec_files(root);
    let (units, mut diags) = load_specs(root);
    if diags.is_empty() {
        let ws = decispec_parse::merge(units);
        for d in decispec_parse::validate(&ws) {
            diags.push(diag_json(&d));
        }
    }
    Ok(json!({
        "spec_files": files.len(),
        "valid": diags.is_empty(),
        "diagnostics": diags,
        "error_count": diags.len(),
    }))
}

fn tool_workspace_overview(root: &Path) -> Result<Value, String> {
    let (units, mut diags) = load_specs(root);
    let ws = decispec_parse::merge(units);
    for d in decispec_parse::validate(&ws) {
        diags.push(diag_json(&d));
    }
    let mut decisions = Vec::new();
    let mut no_requirements = Vec::new();
    let mut no_scenarios = Vec::new();
    let mut self_references = Vec::new();
    for d in &ws.decisions {
        let reqs: Vec<Value> = ws
            .specs
            .iter()
            .find(|s| s.id == d.id)
            .map(|s| {
                s.requirements
                    .iter()
                    .map(|r| {
                        if r.scenarios.is_empty() {
                            no_scenarios.push(r.id.clone());
                        }
                        json!({
                            "id": r.id,
                            "layers": r.layers.iter().map(|l| l.as_str()).collect::<Vec<_>>(),
                            "scenarios": r.scenarios,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        if (d.status.as_str() == "accepted" || d.status.as_str() == "proposed")
            && reqs.is_empty()
        {
            no_requirements.push(d.id.clone());
        }
        if d.status.as_str() == "superseded"
            && d.superseded_by.as_deref() == Some(d.id.as_str())
        {
            self_references.push(d.id.clone());
        }
        decisions.push(json!({
            "id": d.id,
            "title": d.title,
            "status": d.status.as_str(),
            "requirements": reqs,
        }));
    }
    Ok(json!({
        "diagnostics": diags,
        "decisions": decisions,
        "gaps": {
            "decisions_without_requirements": no_requirements,
            "requirements_without_scenarios": no_scenarios,
            "superseded_self_references": self_references,
        },
    }))
}

fn tool_gate_status(root: &Path) -> Result<Value, String> {
    let path = root.join(".decispec/report.json");
    if !path.is_file() {
        return Ok(json!({
            "report": false,
            "message": "no report yet — run `decispec gate` first",
        }));
    }
    let text =
        std::fs::read_to_string(&path).map_err(|e| format!("cannot read report.json: {e}"))?;
    let v: Value =
        serde_json::from_str(&text).map_err(|e| format!("report.json is not valid JSON: {e}"))?;
    let rows: Vec<Value> = v["decisions"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .flat_map(|d| d["rows"].as_array().cloned().unwrap_or_default())
        .collect();
    let pick = |want: &str| {
        rows.iter()
            .filter(|r| r["result"] == want)
            .map(|r| {
                json!({
                    "decision": r["decision"],
                    "requirement": r["requirement"],
                    "item": r["item"],
                    "layer": r["layer"],
                    "detail": r["detail"],
                })
            })
            .collect::<Vec<_>>()
    };
    Ok(json!({
        "report": true,
        "project": v["project"],
        "verdict": v["verdict"],
        "strict_skipped": v["strict_skipped"],
        "counts": v["counts"],
        "failures": v["failures"],
        "warnings": v["warnings"],
        "failing_rows": pick("fail"),
        "uncovered_rows": pick("uncovered"),
        "skipped_rows": pick("skipped"),
    }))
}

/// Walk up from `start` looking for `decispec.toml`. Duplicates the helper
/// in cli's main.rs (a binary cannot be depended on) — see the F16 claim.
fn find_project_root(start: &Path) -> Option<PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        if dir.join("decispec.toml").is_file() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Serve MCP over real stdio until EOF (used by `decispec mcp`).
pub fn serve() -> std::io::Result<()> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    serve_io(stdin.lock(), stdout.lock())
}

/// The NDJSON loop, generic over streams so tests can drive it with buffers.
pub fn serve_io<R: std::io::BufRead, W: std::io::Write>(
    mut input: R,
    mut output: W,
) -> std::io::Result<()> {
    let mut session = Session::new(std::env::current_dir()?);
    let mut line = String::new();
    loop {
        line.clear();
        if input.read_line(&mut line)? == 0 {
            break;
        }
        if let Some(reply) = handle_message(&mut session, &line) {
            output.write_all(reply.as_bytes())?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_dir(name: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "decispec-mcp-test-{}-{n}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    /// A project fixture: decispec.toml + specs/ dir.
    fn fixture(name: &str) -> PathBuf {
        let root = temp_dir(name);
        std::fs::write(root.join("decispec.toml"), "[project]\nname = \"t\"\n").unwrap();
        std::fs::create_dir_all(root.join("specs")).unwrap();
        root
    }

    fn write(root: &Path, rel: &str, content: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    fn send(session: &mut Session, method: &str, id: i64, params: Value) -> Option<Value> {
        let line = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        })
        .to_string();
        handle_message(session, &line).map(|r| serde_json::from_str(&r).unwrap())
    }

    fn call(session: &mut Session, id: i64, name: &str, args: Value) -> Value {
        let reply = send(session, "tools/call", id, json!({ "name": name, "arguments": args }))
            .expect("tools/call always replies");
        assert!(
            reply["result"]["isError"].is_null(),
            "tool error: {reply}"
        );
        serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
    }

    // 1. initialize payload; notifications produce no reply; ping.
    #[test]
    fn initialize_result_and_notifications_silent() {
        let dir = temp_dir("init");
        let mut s = Session::new(dir.clone());
        let reply = send(
            &mut s,
            "initialize",
            1,
            json!({"protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}),
        )
        .unwrap();
        assert_eq!(reply["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(reply["result"]["capabilities"], json!({ "tools": {} }));
        assert_eq!(reply["result"]["serverInfo"]["name"], "decispec");
        assert!(
            !reply["result"]["serverInfo"]["version"]
                .as_str()
                .unwrap()
                .is_empty()
        );

        assert_eq!(
            handle_message(
                &mut s,
                r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#
            ),
            None
        );
        let pong = send(&mut s, "ping", 9, json!({})).unwrap();
        assert_eq!(pong["result"], json!({}));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // 2. tools/list: exactly the 6 tools, schemas with required fields.
    #[test]
    fn tools_list_has_six_tools_with_schemas() {
        let dir = temp_dir("list");
        let mut s = Session::new(dir.clone());
        let reply = send(&mut s, "tools/list", 2, json!({})).unwrap();
        let tools = reply["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 6);
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(
            names,
            [
                "project_info",
                "extract_project",
                "validate_spec",
                "check_workspace",
                "workspace_overview",
                "gate_status",
            ]
        );
        let vs = tools.iter().find(|t| t["name"] == "validate_spec").unwrap();
        assert_eq!(vs["inputSchema"]["required"], json!(["text"]));
        for t in tools {
            assert!(
                t["description"].as_str().is_some_and(|d| d.len() > 20),
                "every tool needs a real description: {t}"
            );
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // 3. validate_spec diagnostics (file:line) for a broken spec, empty for a
    //    valid one; unknown method -32601; malformed line -32700.
    #[test]
    fn validate_spec_reports_diagnostics_and_error_codes() {
        let dir = temp_dir("validate");
        let mut s = Session::new(dir.clone());

        let reply = send(
            &mut s,
            "tools/call",
            3,
            json!({
                "name": "validate_spec",
                "arguments": { "text": "model {\n  container a \"unterminated\n}\n" }
            }),
        )
        .unwrap();
        assert!(reply["result"]["isError"].is_null(), "{reply}");
        let payload: Value =
            serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(payload["valid"], false);
        let diags = payload["diagnostics"].as_array().unwrap();
        assert!(!diags.is_empty());
        assert_eq!(diags[0]["file"], "proposed.spec");
        assert!(diags[0]["line"].as_u64().unwrap() >= 2);
        assert!(!diags[0]["message"].as_str().unwrap().is_empty());

        let ok = call(
            &mut s,
            4,
            "validate_spec",
            json!({
                "text": "decision OK-1 \"Fine\" {\n  status: proposed\n}\n",
                "filename": "ok.spec"
            }),
        );
        assert_eq!(ok["filename"], "ok.spec");
        assert_eq!(ok["valid"], true);
        assert_eq!(ok["diagnostics"].as_array().unwrap().len(), 0);

        let unknown = send(&mut s, "no/such-method", 5, json!({})).unwrap();
        assert_eq!(unknown["error"]["code"], -32601);

        let malformed = handle_message(&mut s, "{this is not json").unwrap();
        let malformed: Value = serde_json::from_str(&malformed).unwrap();
        assert_eq!(malformed["error"]["code"], -32700);
        assert!(malformed["id"].is_null());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    // 4. extract_project on a fixture with packages + one MADR ADR.
    #[test]
    fn extract_project_returns_decisions_and_containers() {
        let root = fixture("extract");
        write(&root, "api/__init__.py", "");
        write(&root, "api/views.py", "import auth\n");
        write(&root, "auth/__init__.py", "");
        write(
            &root,
            "docs/adr/0001-use-postgres.md",
            "# Use Postgres\n\n## Status\nAccepted\n\n## Context\nWe need a store.\n",
        );
        // Session cwd is elsewhere; the per-call `path` arg overrides.
        let elsewhere = temp_dir("elsewhere");
        let mut s = Session::new(elsewhere.clone());
        send(&mut s, "initialize", 1, json!({})).unwrap();

        let p = call(&mut s, 6, "extract_project", json!({ "path": root }));
        assert_eq!(p["lang"], "python");
        let containers = p["containers"].as_array().unwrap();
        assert!(
            containers.iter().any(|c| c["id"] == "api" && c["module"] == "api"),
            "{containers:?}"
        );
        assert_eq!(p["decisions"].as_array().unwrap().len(), 1);
        assert_eq!(p["decisions"][0]["id"], "ADR-0001");
        assert_eq!(p["decisions"][0]["status"], "accepted");
        assert_eq!(p["decisions"][0]["source"], "docs/adr/0001-use-postgres.md");
        assert!(p["spec"].as_str().unwrap().contains("model {"));
        assert!(p["fitness_toml"].as_str().unwrap().contains("[stack.fitness]"));

        std::fs::remove_dir_all(&root).unwrap();
        std::fs::remove_dir_all(&elsewhere).unwrap();
    }

    // 5. workspace_overview gaps; check_workspace clean on a valid fixture.
    #[test]
    fn workspace_overview_lists_porting_gaps() {
        let root = fixture("overview");
        write(
            &root,
            "specs/0001.spec",
            r#"
decision AUTH-001 "Accepted without requirements" {
  status: accepted
}

decision AUTH-002 "Requirement without scenario" {
  status: accepted
}

decision AUTH-003 "Superseded self reference" {
  status: superseded
  superseded_by: AUTH-003
}

spec AUTH-002 {
  requirement AUTH-002-R1 (EARS) {
    text: "WHEN x THEN y"
    layers: [unit]
  }
}
"#,
        );
        let mut s = Session::new(root.clone());
        send(&mut s, "initialize", 1, json!({})).unwrap();

        let overview = call(&mut s, 7, "workspace_overview", json!({}));
        let gaps = &overview["gaps"];
        assert_eq!(
            gaps["decisions_without_requirements"],
            json!(["AUTH-001"])
        );
        assert_eq!(
            gaps["requirements_without_scenarios"],
            json!(["AUTH-002-R1"])
        );
        assert_eq!(
            gaps["superseded_self_references"],
            json!(["AUTH-003"])
        );
        let decisions = overview["decisions"].as_array().unwrap();
        assert_eq!(decisions.len(), 3);
        let d2 = decisions.iter().find(|d| d["id"] == "AUTH-002").unwrap();
        assert_eq!(d2["requirements"][0]["id"], "AUTH-002-R1");
        assert_eq!(d2["requirements"][0]["layers"], json!(["unit"]));

        let check = call(&mut s, 8, "check_workspace", json!({}));
        assert_eq!(check["valid"], true, "{check}");
        assert_eq!(check["diagnostics"].as_array().unwrap().len(), 0);
        assert_eq!(check["spec_files"], 1);

        std::fs::remove_dir_all(&root).unwrap();
    }

    // 6. gate_status: explicit no-report result, then a hand-written report.
    #[test]
    fn gate_status_reports_missing_and_parses_report() {
        let root = fixture("gate");
        let mut s = Session::new(root.clone());
        send(&mut s, "initialize", 1, json!({})).unwrap();

        let none = call(&mut s, 9, "gate_status", json!({}));
        assert_eq!(none["report"], false);
        assert!(
            none["message"].as_str().unwrap().contains("no report"),
            "{none}"
        );

        std::fs::create_dir_all(root.join(".decispec")).unwrap();
        std::fs::write(
            root.join(".decispec/report.json"),
            json!({
                "project": "t",
                "verdict": "pass",
                "strict_skipped": false,
                "counts": { "rows": 3, "passed": 2, "failed": 1, "uncovered": 0, "skipped": 0, "no_artifacts": 0 },
                "failures": ["AUTH-001-R1 valid_token [unit] FAIL"],
                "warnings": [],
                "decisions": [{
                    "id": "AUTH-001",
                    "status": "accepted",
                    "outcome": "pass",
                    "rows": [
                        { "decision": "AUTH-001", "requirement": "AUTH-001-R1", "item": "valid_token",
                          "kind": "scenario", "layer": "unit", "artifacts": [],
                          "result": "fail", "detail": "assert 200" },
                        { "decision": "AUTH-001", "requirement": "AUTH-001-R1", "item": "edge_case",
                          "kind": "scenario", "layer": "unit", "artifacts": [],
                          "result": "uncovered", "detail": "" }
                    ]
                }]
            })
            .to_string(),
        )
        .unwrap();

        let report = call(&mut s, 10, "gate_status", json!({}));
        assert_eq!(report["report"], true);
        assert_eq!(report["verdict"], "pass");
        assert_eq!(report["counts"]["failed"], 1);
        assert_eq!(report["failing_rows"][0]["item"], "valid_token");
        assert_eq!(report["uncovered_rows"][0]["item"], "edge_case");
        assert_eq!(report["skipped_rows"].as_array().unwrap().len(), 0);

        std::fs::remove_dir_all(&root).unwrap();
    }

    // 7. The stdio loop over buffers: one NDJSON reply per request, none for
    //    the notification.
    #[test]
    fn serve_io_dispatches_ndjson_over_buffers() {
        let input = Cursor::new(
            concat!(
                r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
                "\n",
                r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
                "\n",
                r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
                "\n",
            )
            .as_bytes()
            .to_vec(),
        );
        let mut output = Cursor::new(Vec::new());
        // Session cwd is the test process cwd (no decispec.toml there);
        // initialize resolves no root, which is fine for these two methods.
        serve_io(input, &mut output).unwrap();
        let text = String::from_utf8(output.into_inner()).unwrap();
        let lines: Vec<Value> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 2, "notification must produce no reply: {text}");
        assert_eq!(lines[0]["result"]["serverInfo"]["name"], "decispec");
        assert_eq!(lines[1]["result"]["tools"].as_array().unwrap().len(), 6);
    }
}
