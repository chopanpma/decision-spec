//! Compliance gate: builds the traceability matrix
//! (decision -> requirement -> item -> layer -> artifact -> result), consumes
//! JUnit XML produced by the test adapters, and renders a verdict.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
pub use decispec_ir::Layer;
use decispec_ir::{Status, Workspace, names};

#[derive(Debug, thiserror::Error)]
pub enum GateError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("junit parse error in {file}: {message}")]
    Junit { file: String, message: String },
}

// ---------------------------------------------------------------------------
// JUnit XML
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct JunitCase {
    pub classname: String,
    pub name: String,
    pub failed: bool,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct JunitSuite {
    pub name: String,
    pub cases: Vec<JunitCase>,
}

pub fn parse_junit(file: &str, xml: &str) -> Result<Vec<JunitSuite>, GateError> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| GateError::Junit {
        file: file.to_string(),
        message: e.to_string(),
    })?;
    let root = doc.root_element();
    let root_name = root.tag_name().name();
    if root_name != "testsuite" && root_name != "testsuites" {
        return Err(GateError::Junit {
            file: file.to_string(),
            message: format!("expected <testsuite(s)>, found <{root_name}>"),
        });
    }
    let mut suites = Vec::new();
    let suite_nodes: Vec<_> = if root_name == "testsuite" {
        vec![root]
    } else {
        root.children()
            .filter(|n| n.is_element() && n.tag_name().name() == "testsuite")
            .collect()
    };
    for sn in suite_nodes {
        let mut suite = JunitSuite {
            name: sn.attribute("name").unwrap_or("").to_string(),
            cases: Vec::new(),
        };
        for case in sn
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "testcase")
        {
            let failure = case
                .children()
                .find(|n| n.is_element() && matches!(n.tag_name().name(), "failure" | "error"));
            suite.cases.push(JunitCase {
                classname: case.attribute("classname").unwrap_or("").to_string(),
                name: case.attribute("name").unwrap_or("").to_string(),
                failed: failure.is_some(),
                message: failure.and_then(|f| f.attribute("message").map(|m| m.to_string())),
            });
        }
        suites.push(suite);
    }
    Ok(suites)
}

// ---------------------------------------------------------------------------
// Run manifest (written by `decispec test`)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AdapterStatus {
    Ran,
    Skipped,
    Failed,
}

impl AdapterStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            AdapterStatus::Ran => "ran",
            AdapterStatus::Skipped => "skipped",
            AdapterStatus::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterRecord {
    pub name: String,
    pub layer: Layer,
    pub status: AdapterStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default)]
    pub result_files: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Manifest {
    pub adapters: Vec<AdapterRecord>,
}

// ---------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    Pass,
    Fail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RowResult {
    Pass,
    Fail,
    Uncovered,
    Skipped,
    NoArtifacts,
}

impl RowResult {
    pub fn as_str(&self) -> &'static str {
        match self {
            RowResult::Pass => "PASS",
            RowResult::Fail => "FAIL",
            RowResult::Uncovered => "UNCOVERED",
            RowResult::Skipped => "SKIPPED",
            RowResult::NoArtifacts => "NO-ARTIFACTS",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MatrixRow {
    pub decision: String,
    /// Requirement id, or "-" for items attached directly to the spec.
    pub requirement: String,
    pub item: String,
    pub kind: String,
    pub layer: Layer,
    pub artifacts: Vec<String>,
    pub result: RowResult,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DecisionOutcome {
    pub id: String,
    pub status: Status,
    /// pass | fail | excluded-rejected | excluded-superseded
    pub outcome: String,
    pub rows: Vec<MatrixRow>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct GateCounts {
    pub rows: usize,
    pub passed: usize,
    pub failed: usize,
    pub uncovered: usize,
    pub skipped: usize,
    pub no_artifacts: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct GateReport {
    pub project: String,
    pub verdict: Verdict,
    /// True when SKIPPED rows were escalated to FAIL; a reader of report.json
    /// can tell a strict-escalated FAIL from a genuine test failure.
    pub strict_skipped: bool,
    pub counts: GateCounts,
    pub failures: Vec<String>,
    pub warnings: Vec<String>,
    pub decisions: Vec<DecisionOutcome>,
}

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

pub struct GateInput<'a> {
    pub ws: &'a Workspace,
    /// `tests/generated` — used for structural coverage (artifacts per layer).
    pub generated_dir: &'a Path,
    /// Directory with JUnit XML + manifest.json (from `decispec test`).
    pub results_dir: &'a Path,
    pub project: String,
    /// Opt-in strict mode: escalate SKIPPED rows (toolchain never installed or
    /// absent from the manifest) to FAIL, so a CI job cannot pass silently.
    /// Off by default: a skipped toolchain is not a failure. `NoArtifacts`
    /// keeps its precedence and is never re-classified.
    pub strict_skipped: bool,
}

/// Per-layer adapter view: worst status across adapters of that layer, and the
/// JUnit cases collected from that layer's result files.
struct LayerRun {
    status: AdapterStatus,
    adapters: Vec<String>,
    cases: Vec<JunitCase>,
}

fn read_manifest(results_dir: &Path) -> Option<Manifest> {
    let path = results_dir.join("manifest.json");
    let data = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

/// Map a result filename to a layer when no manifest is present.
fn layer_for_result_file(name: &str) -> Option<Layer> {
    let n = name.to_ascii_lowercase();
    if n.contains("pytest") {
        Some(Layer::Unit)
    } else if n.contains("cucumber") || n.contains("karate") || n.contains("contract") {
        Some(Layer::Contract)
    } else if n.contains("playwright") || n.contains("e2e") {
        Some(Layer::E2e)
    } else {
        None
    }
}

fn status_rank(s: AdapterStatus) -> u8 {
    match s {
        AdapterStatus::Failed => 2,
        AdapterStatus::Ran => 1,
        AdapterStatus::Skipped => 0,
    }
}

fn layer_entry(
    runs: &mut std::collections::BTreeMap<Layer, LayerRun>,
    layer: Layer,
    status: AdapterStatus,
    adapter: String,
) -> &mut LayerRun {
    let run = runs.entry(layer).or_insert_with(|| LayerRun {
        status,
        adapters: Vec::new(),
        cases: Vec::new(),
    });
    // Worst status wins: failed > ran > skipped.
    if status_rank(status) > status_rank(run.status) {
        run.status = status;
    }
    run.adapters.push(adapter);
    run
}

fn load_layer_runs(
    results_dir: &Path,
) -> Result<std::collections::BTreeMap<Layer, LayerRun>, GateError> {
    let mut runs: std::collections::BTreeMap<Layer, LayerRun> = std::collections::BTreeMap::new();

    let manifest = read_manifest(results_dir);
    match manifest {
        Some(m) => {
            for a in m.adapters {
                layer_entry(&mut runs, a.layer, a.status, a.name.clone());
                if a.status != AdapterStatus::Skipped {
                    for rel in &a.result_files {
                        let path = PathBuf::from(rel);
                        let file_name = path
                            .file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_default();
                        if !file_name.ends_with(".xml") {
                            continue;
                        }
                        let full = if path.is_absolute() {
                            path.clone()
                        } else {
                            results_dir.join(&file_name)
                        };
                        let xml = match std::fs::read_to_string(&full) {
                            Ok(x) => x,
                            Err(_) => continue,
                        };
                        let suites = parse_junit(&file_name, &xml)?;
                        let run = runs.get_mut(&a.layer).expect("entry exists");
                        for s in suites {
                            run.cases.extend(s.cases);
                        }
                    }
                }
            }
        }
        None => {
            // No manifest: infer from *.xml files in the results dir.
            let entries = match std::fs::read_dir(results_dir) {
                Ok(e) => e,
                Err(_) => return Ok(runs),
            };
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if !name.ends_with(".xml") {
                    continue;
                }
                let Some(layer) = layer_for_result_file(&name) else {
                    continue;
                };
                let xml = std::fs::read_to_string(e.path())?;
                let suites = parse_junit(&name, &xml)?;
                layer_entry(&mut runs, layer, AdapterStatus::Ran, name.clone());
                let run = runs.get_mut(&layer).expect("entry exists");
                for s in suites {
                    run.cases.extend(s.cases);
                }
            }
        }
    }
    Ok(runs)
}

fn dir_has_files(dir: &Path) -> bool {
    fn walk(dir: &Path, found: &mut bool) {
        if *found {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, found);
            } else {
                *found = true;
                return;
            }
        }
    }
    let mut found = false;
    walk(dir, &mut found);
    found
}

/// A layer counts as structurally covered if `tests/generated/<layer>`
/// contains at least one file.
fn layer_has_artifacts(generated_dir: &Path, layer: Layer) -> bool {
    dir_has_files(&generated_dir.join(layer.dir()))
}

fn norm(s: &str) -> String {
    s.replace('-', "_")
}

/// Exact pytest test ids generated for a unit-layer scenario row
/// (parametrized id included, since generated tests use pytest.mark.parametrize).
fn unit_test_ids(item_kind: &str, spec_id: &str, item_id: &str) -> Vec<String> {
    let base = match item_kind {
        "scenario" => names::pytest_scenario_test_name(spec_id, item_id),
        "invariant" => names::pytest_invariant_test_name(spec_id, item_id),
        _ => return Vec::new(),
    };
    vec![base.clone(), format!("{base}[unit]")]
}

/// Expected JUnit ids generated for a contract/e2e-layer scenario row.
fn contract_e2e_test_ids(
    layer: Layer,
    item_kind: &str,
    spec_id: &str,
    item_id: &str,
) -> Vec<String> {
    match (layer, item_kind) {
        (Layer::Contract, "scenario") => vec![names::contract_scenario_test_name(spec_id, item_id)],
        (Layer::E2e, "scenario") => vec![names::e2e_test_name(spec_id, item_id)],
        _ => Vec::new(),
    }
}

/// Artifacts generated for a spec item on a layer (for the matrix + coverage).
fn item_artifacts(
    ws: &Workspace,
    spec_id: &str,
    item_kind: &str,
    item_id: &str,
    layer: Layer,
) -> Vec<String> {
    let _ = ws;
    match (layer, item_kind) {
        (Layer::Unit, "scenario") => vec![format!(
            "tests/generated/unit/{}",
            names::unit_test_file(spec_id, item_id)
        )],
        (Layer::Unit, "invariant") => vec![format!(
            "tests/generated/unit/{}",
            names::invariant_test_file(spec_id, item_id)
        )],
        (Layer::Contract, "scenario") => vec![format!(
            "tests/generated/contract/{}",
            names::contract_feature_file(spec_id)
        )],
        (Layer::E2e, "scenario") => vec![format!(
            "tests/generated/e2e/{}",
            names::e2e_spec_file(item_id)
        )],
        (Layer::Infra, "infra_policy") => vec![format!(
            "tests/generated/infra/{}",
            names::infra_policy_file(item_id)
        )],
        _ => Vec::new(),
    }
}

fn junit_matches<'a>(cases: &'a [JunitCase], ids: &[String]) -> Vec<&'a JunitCase> {
    cases
        .iter()
        .filter(|c| {
            let name = norm(&c.name);
            let classname = norm(&c.classname);
            ids.iter().any(|id| {
                let want = norm(id);
                name == want || classname == want
            })
        })
        .collect()
}

/// A row whose toolchain did not run. Strict mode escalates it to FAIL so a
/// gate run cannot pass because a toolchain was never installed; the detail
/// says so, to keep it distinguishable from a genuine test failure.
fn skipped_outcome(detail: String, strict_skipped: bool) -> (RowResult, String) {
    if strict_skipped {
        (
            RowResult::Fail,
            format!("{detail}; treated as FAIL in strict mode"),
        )
    } else {
        (RowResult::Skipped, detail)
    }
}

/// Whether the layer's JUnit output uses the naming convention codegen emits
/// for this spec: any case name mentions the sanitized spec id. When it does, a
/// row with no matching case is genuinely uncovered; when it does not, the
/// toolchain's naming differs from our assumption and the row falls back to
/// suite-level matching.
///
/// Only `name` is probed, never `classname`. The classname is chosen by the
/// toolchain rather than by codegen: cucumber reports the feature file path or
/// feature name there, and both contain the spec id even when the scenario
/// titles predate the generated naming. Probing it would suppress the fallback
/// and turn an old, green project red.
fn suite_uses_generated_naming(run: &LayerRun, probe: &str) -> bool {
    let probe = norm(probe);
    run.cases.iter().any(|c| norm(&c.name).contains(&probe))
}

/// Coarse, whole-layer judgment used only when a row's own generated test id
/// has no JUnit result to match: one failing case in the layer fails every row
/// on it, and any result at all passes every row.
fn suite_level_outcome(run: Option<&LayerRun>) -> (RowResult, String) {
    let Some(run) = run else {
        return (RowResult::Skipped, "toolchain did not run".to_string());
    };
    if run.cases.is_empty() {
        return (
            RowResult::Uncovered,
            "toolchain ran but produced no JUnit results".to_string(),
        );
    }
    if let Some(f) = run.cases.iter().find(|c| c.failed) {
        (
            RowResult::Fail,
            format!(
                "test '{}' failed: {}",
                f.name,
                f.message.as_deref().unwrap_or("no message")
            ),
        )
    } else {
        (RowResult::Pass, format!("{} passed", run.cases.len()))
    }
}

/// `Ok((result, detail))` is the row outcome. `Err(detail)` means the row could
/// not be matched per item and the caller should apply the documented
/// suite-level fallback plus emit a warning naming `detail`.
fn evaluate_row(
    layer: Layer,
    run: Option<&LayerRun>,
    generated_dir: &Path,
    expected_ids: &[String],
    naming_probe: &str,
    strict_skipped: bool,
) -> Result<(RowResult, String), String> {
    if !layer_has_artifacts(generated_dir, layer) {
        return Ok((
            RowResult::NoArtifacts,
            format!("no test artifacts generated for layer '{layer}' (run `decispec gen`)"),
        ));
    }
    let Some(run) = run else {
        return Ok(skipped_outcome(
            "toolchain did not run".to_string(),
            strict_skipped,
        ));
    };
    match run.status {
        AdapterStatus::Skipped => Ok(skipped_outcome(
            format!(
                "toolchain not run ({}); skipped is not a failure",
                run.adapters.join(", ")
            ),
            strict_skipped,
        )),
        AdapterStatus::Ran | AdapterStatus::Failed => match layer {
            Layer::Unit => {
                let matched = junit_matches(&run.cases, expected_ids);
                if matched.is_empty() {
                    Ok((
                        RowResult::Uncovered,
                        "generated tests have no JUnit result in this run (toolchain ran)"
                            .to_string(),
                    ))
                } else if let Some(f) = matched.iter().find(|c| c.failed) {
                    Ok((
                        RowResult::Fail,
                        format!(
                            "test '{}' failed: {}",
                            f.name,
                            f.message.as_deref().unwrap_or("no message")
                        ),
                    ))
                } else {
                    Ok((RowResult::Pass, format!("{} passed", matched.len())))
                }
            }
            Layer::Contract | Layer::E2e => {
                let matched = if expected_ids.is_empty() {
                    Vec::new()
                } else {
                    junit_matches(&run.cases, expected_ids)
                };
                if matched.is_empty() {
                    if !expected_ids.is_empty() && !suite_uses_generated_naming(run, naming_probe) {
                        return Err("no JUnit result matched the generated test id".to_string());
                    }
                    if run.cases.is_empty() {
                        return Ok((
                            RowResult::Uncovered,
                            "toolchain ran but produced no JUnit results".to_string(),
                        ));
                    }
                    Ok((
                        RowResult::Uncovered,
                        "generated tests have no JUnit result in this run (toolchain ran)"
                            .to_string(),
                    ))
                } else if let Some(f) = matched.iter().find(|c| c.failed) {
                    Ok((
                        RowResult::Fail,
                        format!(
                            "test '{}' failed: {}",
                            f.name,
                            f.message.as_deref().unwrap_or("no message")
                        ),
                    ))
                } else {
                    Ok((RowResult::Pass, format!("{} passed", matched.len())))
                }
            }
            // infra/fitness toolchains don't emit JUnit: gate on adapter status.
            Layer::Infra | Layer::Fitness => {
                if run.status == AdapterStatus::Failed {
                    Ok((
                        RowResult::Fail,
                        format!(
                            "adapter '{}' reported violations or crashed",
                            run.adapters.join(", ")
                        ),
                    ))
                } else {
                    Ok((
                        RowResult::Pass,
                        format!("adapter '{}' completed", run.adapters.join(", ")),
                    ))
                }
            }
        },
    }
}

pub fn evaluate(input: &GateInput) -> Result<GateReport, GateError> {
    let ws = input.ws;
    let layer_runs = load_layer_runs(input.results_dir)?;

    let mut failures: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut decisions: Vec<DecisionOutcome> = Vec::new();
    let mut counts = GateCounts::default();

    for decision in &ws.decisions {
        match decision.status {
            Status::Rejected => {
                decisions.push(DecisionOutcome {
                    id: decision.id.clone(),
                    status: decision.status,
                    outcome: "excluded-rejected".to_string(),
                    rows: Vec::new(),
                });
                continue;
            }
            Status::Superseded => {
                warnings.push(format!(
                    "decision '{}' is superseded{}; excluded from hard failures",
                    decision.id,
                    decision
                        .superseded_by
                        .as_ref()
                        .map(|t| format!(" by {t}"))
                        .unwrap_or_default()
                ));
            }
            _ => {}
        }

        let spec = ws.spec_for_decision(&decision.id);
        let requirements = spec.map(|s| s.requirements.as_slice()).unwrap_or(&[]);

        if requirements.is_empty() {
            let msg = format!(
                "decision '{}' ({}) has no requirements",
                decision.id,
                decision.status.as_str()
            );
            if decision.status == Status::Superseded {
                warnings.push(format!(
                    "{msg}; not a hard failure because the decision is superseded"
                ));
            } else {
                failures.push(msg.clone());
                counts.failed += 1;
            }
            decisions.push(DecisionOutcome {
                id: decision.id.clone(),
                status: decision.status,
                outcome: if decision.status == Status::Superseded {
                    "excluded-superseded".to_string()
                } else {
                    "fail".to_string()
                },
                rows: Vec::new(),
            });
            continue;
        }

        let mut rows: Vec<MatrixRow> = Vec::new();
        let mut decision_failed = false;
        let mut any_tested = false;

        let mut add_row = |req_id: &str,
                           item_id: &str,
                           kind: &str,
                           layer: Layer,
                           decision_failed: &mut bool,
                           any_tested: &mut bool,
                           counts: &mut GateCounts,
                           failures: &mut Vec<String>,
                           warnings: &mut Vec<String>| {
            let artifacts = item_artifacts(ws, &decision.id, kind, item_id, layer);
            let expected_ids = match layer {
                Layer::Unit => unit_test_ids(kind, &decision.id, item_id),
                Layer::Contract | Layer::E2e => {
                    contract_e2e_test_ids(layer, kind, &decision.id, item_id)
                }
                _ => Vec::new(),
            };
            let (result, detail) = match evaluate_row(
                layer,
                layer_runs.get(&layer),
                input.generated_dir,
                &expected_ids,
                &names::sanitize(&decision.id),
                input.strict_skipped,
            ) {
                Ok(outcome) => outcome,
                Err(reason) => {
                    warnings.push(format!(
                        "{} -> {} [{}]: {reason}; falling back to suite-level matching",
                        decision.id,
                        item_id,
                        layer.as_str()
                    ));
                    suite_level_outcome(layer_runs.get(&layer))
                }
            };
            match result {
                RowResult::Pass => counts.passed += 1,
                RowResult::Fail => {
                    counts.failed += 1;
                    *decision_failed = true;
                    failures.push(format!(
                        "{} -> {} -> {} [{}]: {detail}",
                        decision.id,
                        req_id,
                        item_id,
                        layer.as_str()
                    ));
                }
                RowResult::Uncovered => {
                    counts.uncovered += 1;
                    *decision_failed = true;
                    failures.push(format!(
                        "{} -> {} -> {} [{}]: {detail}",
                        decision.id,
                        req_id,
                        item_id,
                        layer.as_str()
                    ));
                }
                RowResult::Skipped => counts.skipped += 1,
                RowResult::NoArtifacts => {
                    counts.no_artifacts += 1;
                    *decision_failed = true;
                    failures.push(format!(
                        "{} -> {} -> {} [{}]: {detail}",
                        decision.id,
                        req_id,
                        item_id,
                        layer.as_str()
                    ));
                }
            }
            if matches!(result, RowResult::Pass | RowResult::Fail) {
                *any_tested = true;
            }
            counts.rows += 1;
            rows.push(MatrixRow {
                decision: decision.id.clone(),
                requirement: req_id.to_string(),
                item: item_id.to_string(),
                kind: kind.to_string(),
                layer,
                artifacts,
                result,
                detail,
            });
        };

        for req in requirements {
            if req.scenarios.is_empty() {
                let msg = format!(
                    "decision '{}' requirement '{}' has no scenarios",
                    decision.id, req.id
                );
                if decision.status == Status::Superseded {
                    warnings.push(format!(
                        "{msg}; not a hard failure because the decision is superseded"
                    ));
                } else {
                    failures.push(msg.clone());
                    decision_failed = true;
                }
            }
            for scen_id in &req.scenarios {
                let Some(scen) = spec.and_then(|s| s.scenario(scen_id)) else {
                    continue; // unknown refs are reported by `decispec check`
                };
                for layer in &req.layers {
                    add_row(
                        &req.id,
                        &scen.id,
                        "scenario",
                        *layer,
                        &mut decision_failed,
                        &mut any_tested,
                        &mut counts,
                        &mut failures,
                        &mut warnings,
                    );
                }
            }
        }

        if let Some(sp) = spec {
            // Scenarios not referenced by any requirement: informational rows
            // on the layers they are exercised on (usually none).
            let referenced: std::collections::BTreeSet<&str> = requirements
                .iter()
                .flat_map(|r| r.scenarios.iter().map(|s| s.as_str()))
                .collect();
            for scen in &sp.scenarios {
                if referenced.contains(scen.id.as_str()) {
                    continue;
                }
                for layer in sp.layers_for_scenario(&scen.id) {
                    add_row(
                        "-",
                        &scen.id,
                        "scenario",
                        layer,
                        &mut decision_failed,
                        &mut any_tested,
                        &mut counts,
                        &mut failures,
                        &mut warnings,
                    );
                }
            }
            for inv in &sp.invariants {
                for layer in &inv.layers {
                    add_row(
                        "-",
                        &inv.id,
                        "invariant",
                        *layer,
                        &mut decision_failed,
                        &mut any_tested,
                        &mut counts,
                        &mut failures,
                        &mut warnings,
                    );
                }
            }
            for policy in &sp.infra_policies {
                for layer in &policy.layers {
                    add_row(
                        "-",
                        &policy.id,
                        "infra_policy",
                        *layer,
                        &mut decision_failed,
                        &mut any_tested,
                        &mut counts,
                        &mut failures,
                        &mut warnings,
                    );
                }
            }
        }

        if decision.status == Status::Superseded {
            if any_tested {
                warnings.push(format!(
                    "coverage drift: tests still map to superseded decision '{}'",
                    decision.id
                ));
            }
            decisions.push(DecisionOutcome {
                id: decision.id.clone(),
                status: decision.status,
                outcome: "excluded-superseded".to_string(),
                rows,
            });
        } else {
            decisions.push(DecisionOutcome {
                id: decision.id.clone(),
                status: decision.status,
                outcome: if decision_failed { "fail" } else { "pass" }.to_string(),
                rows,
            });
        }
    }

    let verdict = if failures.is_empty() {
        Verdict::Pass
    } else {
        Verdict::Fail
    };

    Ok(GateReport {
        project: input.project.clone(),
        verdict,
        strict_skipped: input.strict_skipped,
        counts,
        failures,
        warnings,
        decisions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use decispec_ir::*;

    const JUNIT_PASS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<testsuites>
  <testsuite name="unit" tests="2" failures="0">
    <testcase classname="test_AUTH_001_valid_token" name="test_scenario_AUTH_001_valid_token_unit[unit]" time="0.001"/>
    <testcase classname="test_AUTH_001_invalid_token" name="test_scenario_AUTH_001_invalid_token_unit[unit]" time="0.001"/>
  </testsuite>
</testsuites>"#;

    const JUNIT_FAIL: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<testsuites>
  <testsuite name="unit" tests="2" failures="1">
    <testcase classname="test_AUTH_001_valid_token" name="test_scenario_AUTH_001_valid_token_unit[unit]" time="0.001"/>
    <testcase classname="test_AUTH_001_invalid_token" name="test_scenario_AUTH_001_invalid_token_unit[unit]" time="0.001">
      <failure message="NotImplementedError: implement per spec AUTH-001">traceback</failure>
    </testcase>
  </testsuite>
</testsuites>"#;

    fn test_workspace() -> Workspace {
        let mut ws = Workspace::default();
        ws.decisions.push(Decision {
            id: "AUTH-001".to_string(),
            title: "Use OAuth2 client credentials for service-to-service auth".to_string(),
            status: Status::Accepted,
            context: None,
            consequences: None,
            superseded_by: None,
            src: Src {
                file: "specs/0001-auth.spec".to_string(),
                line: 2,
            },
        });
        ws.specs.push(Spec {
            id: "AUTH-001".to_string(),
            requirements: vec![Requirement {
                id: "AUTH-001-R1".to_string(),
                style: Some("EARS".to_string()),
                text: "WHEN ... THEN ...".to_string(),
                layers: vec![Layer::Unit],
                scenarios: vec!["valid_token".to_string(), "invalid_token".to_string()],
                src: Src {
                    file: "specs/0001-auth.spec".to_string(),
                    line: 20,
                },
            }],
            scenarios: vec![
                Scenario {
                    id: "valid_token".to_string(),
                    given: "g".to_string(),
                    when: "w".to_string(),
                    then: "t".to_string(),
                    src: Src {
                        file: "specs/0001-auth.spec".to_string(),
                        line: 26,
                    },
                },
                Scenario {
                    id: "invalid_token".to_string(),
                    given: "g".to_string(),
                    when: "w".to_string(),
                    then: "t".to_string(),
                    src: Src {
                        file: "specs/0001-auth.spec".to_string(),
                        line: 31,
                    },
                },
            ],
            invariants: vec![],
            infra_policies: vec![],
            src: Src {
                file: "specs/0001-auth.spec".to_string(),
                line: 17,
            },
        });
        ws
    }

    struct TestDirs {
        root: PathBuf,
    }

    impl TestDirs {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "decispec-gate-test-{name}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(root.join("tests/generated/unit")).unwrap();
            std::fs::create_dir_all(root.join("tests/generated/results")).unwrap();
            std::fs::write(
                root.join("tests/generated/unit/dummy.py"),
                "# placeholder so the unit layer counts as generated\n",
            )
            .unwrap();
            TestDirs { root }
        }

        fn write_manifest(&self, adapters: Vec<AdapterRecord>) {
            let m = Manifest { adapters };
            std::fs::write(
                self.root.join("tests/generated/results/manifest.json"),
                serde_json::to_string_pretty(&m).unwrap(),
            )
            .unwrap();
        }

        fn write_pytest_xml(&self, xml: &str) {
            std::fs::write(self.root.join("tests/generated/results/pytest.xml"), xml).unwrap();
        }
    }

    impl Drop for TestDirs {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn run_gate(dirs: &TestDirs, ws: &Workspace) -> GateReport {
        run_gate_strict(dirs, ws, false)
    }

    fn run_gate_strict(dirs: &TestDirs, ws: &Workspace, strict_skipped: bool) -> GateReport {
        let input = GateInput {
            ws,
            generated_dir: &dirs.root.join("tests/generated"),
            results_dir: &dirs.root.join("tests/generated/results"),
            project: "test".to_string(),
            strict_skipped,
        };
        evaluate(&input).expect("gate evaluation failed")
    }

    #[test]
    fn passing_junit_yields_pass() {
        let dirs = TestDirs::new("pass");
        dirs.write_manifest(vec![AdapterRecord {
            name: "pytest".to_string(),
            layer: Layer::Unit,
            status: AdapterStatus::Ran,
            reason: None,
            result_files: vec!["pytest.xml".to_string()],
        }]);
        dirs.write_pytest_xml(JUNIT_PASS);
        let ws = test_workspace();
        let report = run_gate(&dirs, &ws);
        assert_eq!(report.verdict, Verdict::Pass);
        assert_eq!(report.counts.passed, 2);
        assert!(report.failures.is_empty());
    }

    #[test]
    fn failing_junit_yields_fail() {
        let dirs = TestDirs::new("fail");
        dirs.write_manifest(vec![AdapterRecord {
            name: "pytest".to_string(),
            layer: Layer::Unit,
            status: AdapterStatus::Failed,
            reason: None,
            result_files: vec!["pytest.xml".to_string()],
        }]);
        dirs.write_pytest_xml(JUNIT_FAIL);
        let ws = test_workspace();
        let report = run_gate(&dirs, &ws);
        assert_eq!(report.verdict, Verdict::Fail);
        assert_eq!(report.counts.failed, 1);
        assert_eq!(report.failures.len(), 1);
        assert!(
            report.failures[0].contains("invalid_token"),
            "{}",
            report.failures[0]
        );
        assert!(
            report.failures[0].contains("NotImplementedError"),
            "{}",
            report.failures[0]
        );
        let row = report.decisions[0]
            .rows
            .iter()
            .find(|r| r.item == "invalid_token")
            .unwrap();
        assert_eq!(row.result, RowResult::Fail);
    }

    #[test]
    fn missing_result_for_executed_toolchain_is_uncovered() {
        let dirs = TestDirs::new("uncovered");
        // pytest ran according to the manifest, but produced no JUnit results.
        dirs.write_manifest(vec![AdapterRecord {
            name: "pytest".to_string(),
            layer: Layer::Unit,
            status: AdapterStatus::Ran,
            reason: None,
            result_files: vec![],
        }]);
        let ws = test_workspace();
        let report = run_gate(&dirs, &ws);
        assert_eq!(report.verdict, Verdict::Fail);
        assert_eq!(report.counts.uncovered, 2);
        assert!(
            report
                .failures
                .iter()
                .all(|f| f.contains("no JUnit result"))
        );
    }

    #[test]
    fn skipped_toolchain_is_not_a_failure() {
        let dirs = TestDirs::new("skipped");
        dirs.write_manifest(vec![AdapterRecord {
            name: "pytest".to_string(),
            layer: Layer::Unit,
            status: AdapterStatus::Skipped,
            reason: Some("toolchain not installed".to_string()),
            result_files: vec![],
        }]);
        let ws = test_workspace();
        let report = run_gate(&dirs, &ws);
        assert_eq!(report.verdict, Verdict::Pass);
        assert_eq!(report.counts.skipped, 2);
        assert_eq!(report.counts.passed, 0);
    }

    #[test]
    fn strict_skipped_escalates_to_fail() {
        let dirs = TestDirs::new("strictskip");
        dirs.write_manifest(vec![AdapterRecord {
            name: "pytest".to_string(),
            layer: Layer::Unit,
            status: AdapterStatus::Skipped,
            reason: Some("toolchain not installed".to_string()),
            result_files: vec![],
        }]);
        let ws = test_workspace();
        let report = run_gate_strict(&dirs, &ws, true);
        assert_eq!(report.verdict, Verdict::Fail);
        assert!(report.strict_skipped);
        assert_eq!(report.counts.failed, 2);
        assert_eq!(report.counts.skipped, 0);
        assert_eq!(report.counts.passed, 0);
        assert_eq!(report.decisions[0].outcome, "fail");
        assert!(
            report.failures.iter().all(|f| f.contains("strict mode")),
            "{:?}",
            report.failures
        );
        for r in &report.decisions[0].rows {
            assert_eq!(r.result, RowResult::Fail);
            assert!(r.detail.contains("strict mode"), "{}", r.detail);
        }
    }

    #[test]
    fn strict_skipped_escalates_when_manifest_absent() {
        let dirs = TestDirs::new("strictnomanifest");
        // No manifest.json at all: the layer is absent from the run, which is
        // the other Skipped path and must escalate too.
        let ws = test_workspace();
        let report = run_gate_strict(&dirs, &ws, true);
        assert_eq!(report.verdict, Verdict::Fail);
        assert_eq!(report.counts.failed, 2);
        assert_eq!(report.counts.skipped, 0);
        assert!(
            report
                .failures
                .iter()
                .all(|f| f.contains("toolchain did not run") && f.contains("strict mode")),
            "{:?}",
            report.failures
        );
        assert_eq!(
            report.decisions[0].rows[0].detail,
            "toolchain did not run; treated as FAIL in strict mode"
        );
    }

    #[test]
    fn strict_skipped_does_not_touch_no_artifacts() {
        let dirs = TestDirs::new("strictnoart");
        // Skipped toolchain *and* no generated artifacts: NoArtifacts wins and
        // is counted once, never re-classified as a strict failure.
        std::fs::remove_dir_all(dirs.root.join("tests/generated/unit")).unwrap();
        dirs.write_manifest(vec![AdapterRecord {
            name: "pytest".to_string(),
            layer: Layer::Unit,
            status: AdapterStatus::Skipped,
            reason: Some("toolchain not installed".to_string()),
            result_files: vec![],
        }]);
        let ws = test_workspace();
        let report = run_gate_strict(&dirs, &ws, true);
        assert_eq!(report.counts.no_artifacts, 2);
        assert_eq!(report.counts.failed, 0);
        assert_eq!(report.counts.skipped, 0);
        assert!(
            report
                .failures
                .iter()
                .all(|f| f.contains("no test artifacts generated") && !f.contains("strict mode")),
            "{:?}",
            report.failures
        );
    }

    #[test]
    fn strict_skipped_leaves_passing_project_passing() {
        let dirs = TestDirs::new("strictpass");
        dirs.write_manifest(vec![AdapterRecord {
            name: "pytest".to_string(),
            layer: Layer::Unit,
            status: AdapterStatus::Ran,
            reason: None,
            result_files: vec!["pytest.xml".to_string()],
        }]);
        dirs.write_pytest_xml(JUNIT_PASS);
        let ws = test_workspace();
        let report = run_gate_strict(&dirs, &ws, true);
        assert_eq!(report.verdict, Verdict::Pass);
        assert_eq!(report.counts.passed, 2);
        assert_eq!(report.counts.failed, 0);
        assert_eq!(report.counts.skipped, 0);
        assert!(report.failures.is_empty());
    }

    #[test]
    fn no_artifacts_is_structural_failure() {
        let dirs = TestDirs::new("noart");
        // Remove the generated unit dir entirely.
        std::fs::remove_dir_all(dirs.root.join("tests/generated/unit")).unwrap();
        dirs.write_manifest(vec![AdapterRecord {
            name: "pytest".to_string(),
            layer: Layer::Unit,
            status: AdapterStatus::Ran,
            reason: None,
            result_files: vec!["pytest.xml".to_string()],
        }]);
        dirs.write_pytest_xml(JUNIT_PASS);
        let ws = test_workspace();
        let report = run_gate(&dirs, &ws);
        assert_eq!(report.verdict, Verdict::Fail);
        assert_eq!(report.counts.no_artifacts, 2);
    }

    #[test]
    fn decision_with_no_requirements_fails() {
        let dirs = TestDirs::new("noreq");
        let mut ws = test_workspace();
        ws.specs.clear();
        let report = run_gate(&dirs, &ws);
        assert_eq!(report.verdict, Verdict::Fail);
        assert!(
            report.failures[0].contains("has no requirements"),
            "{}",
            report.failures[0]
        );
    }

    #[test]
    fn requirement_with_no_scenarios_fails() {
        let dirs = TestDirs::new("noscen");
        let mut ws = test_workspace();
        ws.specs[0].requirements[0].scenarios.clear();
        let report = run_gate(&dirs, &ws);
        assert_eq!(report.verdict, Verdict::Fail);
        assert!(
            report
                .failures
                .iter()
                .any(|f| f.contains("has no scenarios")),
            "{:?}",
            report.failures
        );
    }

    #[test]
    fn superseded_decision_is_warning_not_failure() {
        let dirs = TestDirs::new("sup");
        let mut ws = test_workspace();
        ws.decisions[0].status = Status::Superseded;
        ws.decisions[0].superseded_by = Some("AUTH-007".to_string());
        let report = run_gate(&dirs, &ws);
        assert_eq!(report.verdict, Verdict::Pass);
        assert!(
            report
                .warnings
                .iter()
                .any(|w| w.contains("superseded by AUTH-007")),
            "{:?}",
            report.warnings
        );
        assert_eq!(report.decisions[0].outcome, "excluded-superseded");
    }

    const CONTRACT_JUNIT_ONE_FAIL: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<testsuites>
  <testsuite name="contract" tests="2" failures="1">
    <testcase classname="AUTH_001_valid_token" name="AUTH_001_valid_token" time="0.001">
      <failure message="contract mismatch">boom</failure>
    </testcase>
    <testcase classname="AUTH_001_invalid_token" name="AUTH_001_invalid_token" time="0.001"/>
  </testsuite>
</testsuites>"#;

    const E2E_JUNIT_ONE_FAIL: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<testsuites>
  <testsuite name="e2e" tests="2" failures="1">
    <testcase classname="valid_token.spec.ts" name="AUTH_001 valid_token" time="0.001">
      <failure message="e2e timeout">boom</failure>
    </testcase>
    <testcase classname="invalid_token.spec.ts" name="AUTH_001 invalid_token" time="0.001"/>
  </testsuite>
</testsuites>"#;

    fn multi_layer_workspace() -> Workspace {
        let mut ws = test_workspace();
        ws.specs[0].requirements[0].layers = vec![Layer::Unit, Layer::Contract, Layer::E2e];
        ws.specs[0].requirements[0].scenarios = vec!["valid_token".to_string()];
        ws.specs[0].requirements.push(Requirement {
            id: "AUTH-001-R2".to_string(),
            style: Some("EARS".to_string()),
            text: "WHEN credentials are invalid THEN the auth service SHALL reject with 401"
                .to_string(),
            layers: vec![Layer::Unit, Layer::Contract, Layer::E2e],
            scenarios: vec!["invalid_token".to_string()],
            src: Src {
                file: "specs/0001-auth.spec".to_string(),
                line: 24,
            },
        });
        ws
    }

    fn write_contract_artifacts(dirs: &TestDirs) {
        std::fs::create_dir_all(dirs.root.join("tests/generated/contract")).unwrap();
        std::fs::create_dir_all(dirs.root.join("tests/generated/e2e")).unwrap();
        std::fs::write(
            dirs.root.join("tests/generated/contract/AUTH-001.feature"),
            "Feature: x\n",
        )
        .unwrap();
        std::fs::write(
            dirs.root.join("tests/generated/e2e/valid_token.spec.ts"),
            "//\n",
        )
        .unwrap();
    }

    fn write_xml(dirs: &TestDirs, file: &str, xml: &str) {
        std::fs::write(dirs.root.join("tests/generated/results").join(file), xml).unwrap();
    }

    fn adapter(layer: Layer, name: &str, file: &str, status: AdapterStatus) -> AdapterRecord {
        AdapterRecord {
            name: name.to_string(),
            layer,
            status,
            reason: None,
            result_files: vec![file.to_string()],
        }
    }

    fn row_for<'a>(report: &'a GateReport, item: &str, layer: Layer) -> &'a MatrixRow {
        report.decisions[0]
            .rows
            .iter()
            .find(|r| r.item == item && r.layer == layer)
            .expect("row missing")
    }

    #[test]
    fn contract_row_fails_only_for_its_own_failing_scenario() {
        let dirs = TestDirs::new("contractperitem");
        write_contract_artifacts(&dirs);
        dirs.write_manifest(vec![adapter(
            Layer::Contract,
            "cucumber",
            "contract.xml",
            AdapterStatus::Ran,
        )]);
        write_xml(&dirs, "contract.xml", CONTRACT_JUNIT_ONE_FAIL);
        let report = run_gate(&dirs, &multi_layer_workspace());
        assert_eq!(
            row_for(&report, "valid_token", Layer::Contract).result,
            RowResult::Fail
        );
        assert_eq!(
            row_for(&report, "invalid_token", Layer::Contract).result,
            RowResult::Pass
        );
        assert_eq!(report.counts.uncovered, 0);
        assert!(
            !report
                .warnings
                .iter()
                .any(|w| w.contains("falling back to suite-level matching"))
        );
    }

    #[test]
    fn e2e_row_fails_only_for_its_own_failing_scenario() {
        let dirs = TestDirs::new("e2eperitem");
        write_contract_artifacts(&dirs);
        dirs.write_manifest(vec![adapter(
            Layer::E2e,
            "playwright",
            "playwright.xml",
            AdapterStatus::Ran,
        )]);
        write_xml(&dirs, "playwright.xml", E2E_JUNIT_ONE_FAIL);
        let report = run_gate(&dirs, &multi_layer_workspace());
        assert_eq!(
            row_for(&report, "valid_token", Layer::E2e).result,
            RowResult::Fail
        );
        assert_eq!(
            row_for(&report, "invalid_token", Layer::E2e).result,
            RowResult::Pass
        );
        assert!(
            !report
                .warnings
                .iter()
                .any(|w| w.contains("falling back to suite-level matching"))
        );
    }

    #[test]
    fn contract_row_uncovered_when_no_result_for_that_scenario() {
        let dirs = TestDirs::new("contractuncovered");
        write_contract_artifacts(&dirs);
        dirs.write_manifest(vec![adapter(
            Layer::Contract,
            "cucumber",
            "contract.xml",
            AdapterStatus::Ran,
        )]);
        write_xml(
            &dirs,
            "contract.xml",
            r#"<?xml version="1.0" encoding="utf-8"?>
<testsuites><testsuite name="contract" tests="1" failures="0">
  <testcase classname="AUTH_001_valid_token" name="AUTH_001_valid_token"/>
</testsuite></testsuites>"#,
        );
        let report = run_gate(&dirs, &multi_layer_workspace());
        assert_eq!(
            row_for(&report, "valid_token", Layer::Contract).result,
            RowResult::Pass
        );
        assert_eq!(
            row_for(&report, "invalid_token", Layer::Contract).result,
            RowResult::Uncovered
        );
        assert_eq!(report.counts.uncovered, 1);
        assert_eq!(report.verdict, Verdict::Fail);
    }

    #[test]
    fn contract_falls_back_to_suite_level_when_ids_do_not_match() {
        let dirs = TestDirs::new("contractfallback");
        write_contract_artifacts(&dirs);
        dirs.write_manifest(vec![adapter(
            Layer::Contract,
            "cucumber",
            "contract.xml",
            AdapterStatus::Ran,
        )]);
        // Old-style feature: JUnit names carry the bare scenario id only.
        write_xml(
            &dirs,
            "contract.xml",
            r#"<?xml version="1.0" encoding="utf-8"?>
<testsuites><testsuite name="contract" tests="2" failures="1">
  <testcase classname="c" name="valid_token"><failure message="boom"/></testcase>
  <testcase classname="c" name="invalid_token"/>
</testsuite></testsuites>"#,
        );
        let report = run_gate(&dirs, &multi_layer_workspace());
        let warning = report
            .warnings
            .iter()
            .find(|w| w.contains("no JUnit result matched the generated test id"))
            .expect("fallback warning missing");
        assert!(
            warning.contains("AUTH-001") && warning.contains("[contract]"),
            "{warning}"
        );
        assert!(
            warning.contains("falling back to suite-level matching"),
            "{warning}"
        );
        assert_eq!(report.counts.uncovered, 0);
        // Coarse fallback: both rows follow the suite, so both fail.
        assert_eq!(
            row_for(&report, "valid_token", Layer::Contract).result,
            RowResult::Fail
        );
        assert_eq!(
            row_for(&report, "invalid_token", Layer::Contract).result,
            RowResult::Fail
        );
    }

    #[test]
    fn contract_falls_back_when_only_the_classname_carries_the_spec_id() {
        // Cucumber reports the feature file path or feature name as the
        // classname, so an old-style feature still yields a classname
        // containing the spec id. That must not suppress the fallback and turn
        // a green project red.
        let dirs = TestDirs::new("contractclassname");
        write_contract_artifacts(&dirs);
        dirs.write_manifest(vec![adapter(
            Layer::Contract,
            "cucumber",
            "contract.xml",
            AdapterStatus::Ran,
        )]);
        write_xml(
            &dirs,
            "contract.xml",
            r#"<?xml version="1.0" encoding="utf-8"?>
<testsuites><testsuite name="contract" tests="2" failures="1">
  <testcase classname="tests/generated/contract/AUTH-001.feature" name="valid_token"><failure message="boom"/></testcase>
  <testcase classname="tests/generated/contract/AUTH-001.feature" name="invalid_token"/>
</testsuite></testsuites>"#,
        );
        let report = run_gate(&dirs, &multi_layer_workspace());
        assert_eq!(report.counts.uncovered, 0, "{:?}", report.counts);
        assert!(
            report
                .warnings
                .iter()
                .any(|w| w.contains("no JUnit result matched the generated test id"))
        );
        assert_eq!(
            row_for(&report, "valid_token", Layer::Contract).result,
            RowResult::Fail
        );
        assert_eq!(
            row_for(&report, "invalid_token", Layer::Contract).result,
            RowResult::Fail
        );
    }

    #[test]
    fn junit_parsing_handles_testsuite_root() {
        let xml = r#"<testsuite name="unit" tests="1" failures="1">
  <testcase classname="c" name="n"><error message="boom">x</error></testcase>
</testsuite>"#;
        let suites = parse_junit("inline", xml).unwrap();
        assert_eq!(suites.len(), 1);
        assert_eq!(suites[0].cases.len(), 1);
        assert!(suites[0].cases[0].failed);
        assert_eq!(suites[0].cases[0].message.as_deref(), Some("boom"));
    }

    #[test]
    fn junit_parsing_rejects_non_junit() {
        assert!(parse_junit("bad", "<html></html>").is_err());
    }
}
