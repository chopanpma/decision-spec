//! Proof coverage: roll a gate report up into per-decision proof states and a
//! headline percentage — the "% of specs proven by test" surface (PROOF-001
//! through PROOF-005, specs/070-proof.spec).
//!
//! Pure arithmetic over evidence extracted from a report; both cli (from
//! `GateReport` / report.json) and tui (from its `snapshot` mirror) feed the
//! same [`rollup`] so every surface computes the identical number.

use decispec_ir::Status;

use crate::{DecisionOutcome, GateReport, RowResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProofState {
    /// Every gatable row mapped to the decision passed.
    Proven,
    /// At least one row passed and at least one did not.
    Partial,
    /// No row passed — including decisions with no rows at all.
    Unproven,
}

impl ProofState {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProofState::Proven => "proven",
            ProofState::Partial => "partial",
            ProofState::Unproven => "unproven",
        }
    }

    /// Glyph paired with the label everywhere a state renders, so no-color
    /// terminals and piped logs read the same three states as the TUI colors.
    pub fn symbol(&self) -> &'static str {
        match self {
            ProofState::Proven => "✓",
            ProofState::Partial => "◐",
            ProofState::Unproven => "○",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionCoverage {
    pub id: String,
    pub state: ProofState,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Coverage {
    /// Decisions with every row passed.
    pub proven: usize,
    /// Accepted + proposed decisions — the denominator.
    pub gatable: usize,
    /// `proven * 100 / gatable`, floored; 0 when `gatable` is 0.
    pub percent: usize,
    /// Gatable decisions in report order, each with its computed state.
    pub decisions: Vec<DecisionCoverage>,
}

/// One decision's evidence: its declared status plus the counts needed by the
/// proof rule. Carrying counts (not row references) lets cli and tui each map
/// their own report representation into evidence without sharing row types.
#[derive(Debug, Clone, Copy)]
pub struct DecisionEvidence<'a> {
    pub id: &'a str,
    pub status: Status,
    pub rows_total: usize,
    pub rows_passed: usize,
}

impl<'a> DecisionEvidence<'a> {
    pub fn from_outcome(d: &'a DecisionOutcome) -> Self {
        DecisionEvidence {
            id: &d.id,
            status: d.status,
            rows_total: d.rows.len(),
            rows_passed: d.rows.iter().filter(|r| r.result == RowResult::Pass).count(),
        }
    }
}

/// Roll report evidence up into per-decision proof states + the headline.
/// Superseded and rejected decisions are excluded from every count; a SKIPPED
/// (or failed/uncovered/no-artifacts) row is never proof.
pub fn rollup(evidence: &[DecisionEvidence]) -> Coverage {
    let mut cov = Coverage::default();
    for e in evidence {
        if !matches!(e.status, Status::Accepted | Status::Proposed) {
            continue;
        }
        cov.gatable += 1;
        let state = if e.rows_total > 0 && e.rows_passed == e.rows_total {
            ProofState::Proven
        } else if e.rows_passed > 0 {
            ProofState::Partial
        } else {
            ProofState::Unproven
        };
        if state == ProofState::Proven {
            cov.proven += 1;
        }
        cov.decisions.push(DecisionCoverage {
            id: e.id.to_string(),
            state,
        });
    }
    cov.percent = cov.proven.checked_mul(100).and_then(|x| x.checked_div(cov.gatable)).unwrap_or(0);
    cov
}

impl GateReport {
    /// Proof coverage of this report: the same rollup `decispec coverage`
    /// computes from report.json, so the gate summary line and the command
    /// can never disagree (PROOF-005).
    pub fn coverage(&self) -> Coverage {
        rollup(
            &self
                .decisions
                .iter()
                .map(DecisionEvidence::from_outcome)
                .collect::<Vec<_>>(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GateCounts, Verdict};

    fn ev<'a>(id: &'a str, status: Status, rows: &'a [RowResult]) -> DecisionEvidence<'a> {
        DecisionEvidence {
            id,
            status,
            rows_total: rows.len(),
            rows_passed: rows.iter().filter(|r| **r == RowResult::Pass).count(),
        }
    }

    fn report_with(decisions: Vec<DecisionOutcome>) -> GateReport {
        GateReport {
            project: "p".to_string(),
            verdict: Verdict::Pass,
            strict_skipped: false,
            counts: GateCounts::default(),
            failures: vec![],
            warnings: vec![],
            decisions,
        }
    }

    fn outcome(id: &str, status: Status, rows: Vec<RowResult>) -> DecisionOutcome {
        DecisionOutcome {
            id: id.to_string(),
            status,
            outcome: String::new(),
            rows: rows
                .into_iter()
                .map(|result| crate::MatrixRow {
                    decision: id.to_string(),
                    requirement: "R".to_string(),
                    item: "i".to_string(),
                    kind: "scenario".to_string(),
                    layer: decispec_ir::Layer::Unit,
                    artifacts: vec![],
                    result,
                    detail: String::new(),
                })
                .collect(),
        }
    }

    #[test]
    fn every_passing_row_marks_the_decision_proven() {
        let cov = rollup(&[ev(
            "D-1",
            Status::Accepted,
            &[RowResult::Pass, RowResult::Pass],
        )]);
        assert_eq!(cov.decisions[0].state, ProofState::Proven);
        assert_eq!(cov.proven, 1);
        assert_eq!(cov.gatable, 1);
        assert_eq!(cov.percent, 100);
    }

    #[test]
    fn one_failing_row_marks_the_decision_partial() {
        let cov = rollup(&[ev(
            "D-1",
            Status::Accepted,
            &[RowResult::Pass, RowResult::Fail],
        )]);
        assert_eq!(cov.decisions[0].state, ProofState::Partial);
        assert_eq!(cov.proven, 0);
        assert_eq!(cov.percent, 0);
    }

    #[test]
    fn a_skipped_row_is_not_proof() {
        // All-skipped: the toolchain never ran, so there is no evidence —
        // the decision is unproven, never vacuously proven.
        let cov = rollup(&[ev("D-1", Status::Accepted, &[RowResult::Skipped])]);
        assert_eq!(cov.decisions[0].state, ProofState::Unproven);
        // Mixed pass + skipped: partial, and still not proven.
        let cov = rollup(&[ev(
            "D-1",
            Status::Accepted,
            &[RowResult::Pass, RowResult::Skipped],
        )]);
        assert_eq!(cov.decisions[0].state, ProofState::Partial);
    }

    #[test]
    fn a_decision_with_no_rows_is_unproven() {
        let cov = rollup(&[ev("D-1", Status::Proposed, &[])]);
        assert_eq!(cov.decisions[0].state, ProofState::Unproven);
        assert_eq!(cov.gatable, 1, "proposed decisions count in the denominator");
        assert_eq!(cov.percent, 0);
    }

    #[test]
    fn superseded_and_rejected_decisions_are_excluded() {
        let cov = rollup(&[
            ev("D-1", Status::Accepted, &[RowResult::Pass]),
            ev("D-2", Status::Proposed, &[RowResult::Pass]),
            ev("D-3", Status::Superseded, &[RowResult::Pass]),
            ev("D-4", Status::Rejected, &[]),
        ]);
        assert_eq!(cov.gatable, 2, "only accepted + proposed count");
        assert_eq!(cov.proven, 2);
        assert_eq!(
            cov.decisions.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(),
            vec!["D-1", "D-2"],
            "excluded decisions must not appear in the per-decision list either"
        );
    }

    #[test]
    fn headline_counts_decisions_not_scenarios() {
        // One decision with a single row, one with five: each counts once.
        let cov = rollup(&[
            ev("D-1", Status::Accepted, &[RowResult::Pass]),
            ev(
                "D-2",
                Status::Accepted,
                &[
                    RowResult::Pass,
                    RowResult::Pass,
                    RowResult::Pass,
                    RowResult::Pass,
                    RowResult::Fail,
                ],
            ),
        ]);
        assert_eq!(cov.gatable, 2);
        assert_eq!(cov.proven, 1);
        assert_eq!(cov.percent, 50);
    }

    #[test]
    fn empty_workspace_reports_zero_not_nan() {
        let cov = rollup(&[ev("D-1", Status::Superseded, &[RowResult::Pass])]);
        assert_eq!(cov.gatable, 0);
        assert_eq!(cov.percent, 0);
        assert!(cov.decisions.is_empty());
    }

    #[test]
    fn report_coverage_rolls_up_its_own_decisions() {
        // GateReport::coverage is the gate-summary path (PROOF-005): it must
        // see the same states the command computes from report.json.
        let report = report_with(vec![
            outcome("D-1", Status::Accepted, vec![RowResult::Pass]),
            outcome("D-2", Status::Proposed, vec![RowResult::NoArtifacts]),
            outcome("D-3", Status::Superseded, vec![RowResult::Pass]),
        ]);
        let cov = report.coverage();
        assert_eq!(
            cov.decisions
                .iter()
                .map(|d| (d.id.as_str(), d.state))
                .collect::<Vec<_>>(),
            vec![
                ("D-1", ProofState::Proven),
                ("D-2", ProofState::Unproven),
            ]
        );
        assert_eq!((cov.proven, cov.gatable, cov.percent), (1, 2, 50));
    }
}
