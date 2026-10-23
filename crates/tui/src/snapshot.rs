//! Read-model of a project's `.decispec/report.json` for the dashboard view.
//!
//! `decispec-gate`'s report types are `Serialize`-only, and the dashboard
//! needs only a subset — so this mirror keeps `tui` decoupled from `gate`
//! while reusing `ir`'s `Layer`/`Status` (both `Deserialize`). serde defaults
//! keep a slightly older or newer report.json readable.

use decispec_gate::coverage::{Coverage, DecisionEvidence};
use decispec_ir::{Layer, Status};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    Pass,
    Fail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RowOutcome {
    Pass,
    Fail,
    Uncovered,
    Skipped,
    NoArtifacts,
}

impl RowOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            RowOutcome::Pass => "pass",
            RowOutcome::Fail => "fail",
            RowOutcome::Uncovered => "uncovered",
            RowOutcome::Skipped => "skipped",
            RowOutcome::NoArtifacts => "no-artifacts",
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Counts {
    #[serde(default)]
    pub rows: usize,
    #[serde(default)]
    pub passed: usize,
    #[serde(default)]
    pub failed: usize,
    #[serde(default)]
    pub uncovered: usize,
    #[serde(default)]
    pub skipped: usize,
    #[serde(default)]
    pub no_artifacts: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RowSnapshot {
    pub decision: String,
    #[serde(default)]
    pub requirement: String,
    #[serde(default)]
    pub item: String,
    #[serde(default)]
    pub layer: Option<Layer>,
    #[serde(default)]
    pub result: Option<RowOutcome>,
    #[serde(default)]
    pub detail: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DecisionSnapshot {
    pub id: String,
    pub status: Status,
    pub outcome: String,
    #[serde(default)]
    pub rows: Vec<RowSnapshot>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GateSnapshot {
    pub project: String,
    pub verdict: Verdict,
    #[serde(default)]
    pub strict_skipped: bool,
    pub counts: Counts,
    #[serde(default)]
    pub failures: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub decisions: Vec<DecisionSnapshot>,
}

impl GateSnapshot {
    /// Parse report.json content. I/O (missing file) is the caller's concern;
    /// unparseable content is a plain Err.
    pub fn parse(src: &str) -> Result<GateSnapshot, String> {
        serde_json::from_str(src).map_err(|e| format!("report.json: {e}"))
    }

    /// Matrix rows across all decisions, for the dashboard's failing-rows list.
    pub fn rows(&self) -> impl Iterator<Item = &RowSnapshot> {
        self.decisions.iter().flat_map(|d| d.rows.iter())
    }

    /// Proof rollup over this report (PROOF-004): the same `decispec_gate::
    /// coverage::rollup` the cli's `decispec coverage` feeds, so the cockpit
    /// and the console always agree on the number. A row proves its decision
    /// only on `RowOutcome::Pass`; a missing result is not evidence.
    pub fn coverage(&self) -> Coverage {
        let evidence: Vec<DecisionEvidence<'_>> = self
            .decisions
            .iter()
            .map(|d| DecisionEvidence {
                id: &d.id,
                status: d.status,
                rows_total: d.rows.len(),
                rows_passed: d
                    .rows
                    .iter()
                    .filter(|r| r.result == Some(RowOutcome::Pass))
                    .count(),
            })
            .collect();
        decispec_gate::coverage::rollup(&evidence)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use decispec_gate::coverage::ProofState;

    /// Shape mirror of what `decispec gate` writes today (see
    /// `crates/gate/src/lib.rs` GateReport/MatrixRow + the cli writer).
    const REPORT_JSON: &str = r#"{
      "project": "tvgo",
      "verdict": "pass",
      "strict_skipped": false,
      "counts": {"rows": 4, "passed": 2, "failed": 0, "uncovered": 1, "skipped": 1, "no_artifacts": 0},
      "failures": [],
      "warnings": ["coverage drift on OLD-1"],
      "decisions": [
        {
          "id": "AUTH-001",
          "status": "accepted",
          "outcome": "pass",
          "rows": [
            {"decision": "AUTH-001", "requirement": "AUTH-001-R1", "item": "valid_token",
             "kind": "scenario", "layer": "unit", "artifacts": ["tests/generated/unit/x.py"],
             "result": "pass", "detail": ""},
            {"decision": "AUTH-001", "requirement": "AUTH-001-R1", "item": "valid_token",
             "kind": "scenario", "layer": "contract", "artifacts": [],
             "result": "uncovered", "detail": ""},
            {"decision": "AUTH-001", "requirement": "-", "item": "imports",
             "kind": "fitness", "layer": "fitness", "artifacts": [],
             "result": "skipped", "detail": "toolchain not installed"},
            {"decision": "AUTH-001", "requirement": "AUTH-001-R2", "item": "old_scen",
             "kind": "scenario", "layer": "e2e", "artifacts": [],
             "result": "no-artifacts", "detail": "no generated artifacts for layer"}
          ]
        },
        {
          "id": "OLD-1",
          "status": "superseded",
          "outcome": "excluded-superseded",
          "rows": []
        }
      ]
    }"#;

    #[test]
    fn parses_a_full_report() {
        let snap = GateSnapshot::parse(REPORT_JSON).unwrap();
        assert_eq!(snap.project, "tvgo");
        assert_eq!(snap.verdict, Verdict::Pass);
        assert!(!snap.strict_skipped);
        assert_eq!(snap.counts.rows, 4);
        assert_eq!(snap.counts.passed, 2);
        assert_eq!(snap.counts.uncovered, 1);
        assert_eq!(snap.counts.skipped, 1);
        assert_eq!(snap.warnings, vec!["coverage drift on OLD-1".to_string()]);
        assert_eq!(snap.decisions.len(), 2);
        let auth = &snap.decisions[0];
        assert_eq!(auth.id, "AUTH-001");
        assert_eq!(auth.status, Status::Accepted);
        assert_eq!(auth.outcome, "pass");
        assert_eq!(auth.rows.len(), 4);
        assert_eq!(auth.rows[0].layer, Some(Layer::Unit));
        assert_eq!(auth.rows[0].result, Some(RowOutcome::Pass));
        assert_eq!(auth.rows[2].result, Some(RowOutcome::Skipped));
        assert_eq!(snap.decisions[1].status, Status::Superseded);
    }

    #[test]
    fn parses_kebab_case_row_results() {
        let snap = GateSnapshot::parse(REPORT_JSON).unwrap();
        let results: Vec<&str> = snap.rows().map(|r| r.result.unwrap().as_str()).collect();
        // "no-artifacts" only deserializes under the kebab-case rename — a
        // snake_case regression turns real reports into Err, pinned here.
        assert_eq!(results, vec!["pass", "uncovered", "skipped", "no-artifacts"]);
    }

    #[test]
    fn tolerates_missing_optional_fields() {
        let src = r#"{"project": "x", "verdict": "fail",
                      "counts": {"rows": 0}, "decisions": [{"id": "D", "status": "proposed", "outcome": "fail"}]}"#;
        let snap = GateSnapshot::parse(src).unwrap();
        assert_eq!(snap.verdict, Verdict::Fail);
        assert!(snap.failures.is_empty());
        assert!(snap.decisions[0].rows.is_empty());
        assert_eq!(snap.decisions[0].status, Status::Proposed);
    }

    #[test]
    fn ignores_unknown_fields() {
        let src = r#"{"project": "x", "verdict": "pass", "counts": {},
                      "future_field": {"nested": true}, "decisions": []}"#;
        assert!(GateSnapshot::parse(src).is_ok());
    }

    #[test]
    fn garbage_is_an_error() {
        assert!(GateSnapshot::parse("not json").is_err());
    }

    #[test]
    fn coverage_rolls_up_mirror_rows() {
        let snap = GateSnapshot::parse(REPORT_JSON).unwrap();
        let cov = snap.coverage();
        // RowOutcome::Pass is proof; uncovered/skipped/no-artifacts are not —
        // one pass among four rows is partial, not proven.
        assert_eq!(
            cov.decisions
                .iter()
                .map(|d| (d.id.as_str(), d.state))
                .collect::<Vec<_>>(),
            vec![("AUTH-001", ProofState::Partial)]
        );
        // OLD-1 is superseded: excluded from the list and every count.
        assert_eq!((cov.proven, cov.gatable, cov.percent), (0, 1, 0));
    }
}
