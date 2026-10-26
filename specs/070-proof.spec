# Proof coverage — prefix PROOF.
# Authored 2026-10-20 (F37, spec) and implemented same day (F38). Every
# scenario delegates (tests/glue/__init__.py, tier 1) to the exact workspace
# Rust test that pins it; the `# delegates:` comments name that test.

decision PROOF-001 "Proof coverage reads the last gate report instead of running tests" {
  status: accepted
  context: "The gate already writes .decispec/report.json on every run with every matrix row's result, and TUI-002 proved the file is re-readable through a Deserialize mirror. Running toolchains again just to count proven specs would duplicate the slowest part of the workflow and could disagree with the verdict the project already saw. Asked for on 2026-10-20: a per-project answer to what percent of the specs are proven by test, in the console and in the spec views."
  consequences: "+ coverage is fast and consistent with the verdict the gate actually reached. - it can be stale: specs edited after the last gate run are not reflected, so the output must name the run it reflects. - no report at all means no number: the command says so and exits 2 instead of guessing."
}

spec PROOF-001 {
  requirement PROOF-001-R1 (EARS) {
    text: "WHEN proof coverage is requested THEN it SHALL be computed from the most recent gate report without running test toolchains and SHALL name the run it reflects"
    layers: [unit]
    scenarios: [coverage_uses_last_report_without_running_toolchains, coverage_without_a_gate_report_exits_2, coverage_names_the_run_it_reflects]
  }
  scenario coverage_uses_last_report_without_running_toolchains {
    given: "a project whose .decispec/report.json holds a completed gate run"
    when: "decispec coverage computes the rollup"
    then: "no adapter or toolchain process runs and the printed states reflect that report's rows"
    # delegates: decispec-gate coverage::tests::report_coverage_rolls_up_its_own_decisions
  }
  scenario coverage_without_a_gate_report_exits_2 {
    given: "a project with no .decispec/report.json"
    when: "decispec coverage runs"
    then: "it exits 2 telling the user to run decispec gate first"
    # delegates: decispec (cli) tests::coverage_without_a_gate_report_exits_2
    # (same glue also pins tests::coverage_with_corrupt_report_exits_2: a
    # report that cannot be read is also exit 2, never a fabricated zero)
  }
  scenario coverage_names_the_run_it_reflects {
    given: "a report written by an earlier gate run"
    when: "decispec coverage prints its result"
    then: "the output names the run's identity or time so stale coverage is visible"
    # delegates: decispec (cli) tests::format_utc_converts_known_epochs — the
    # run-label formatter behind the "from gate run …" header line
  }
}

decision PROOF-002 "A decision is proven only when every gatable row mapped to it passed" {
  status: accepted
  context: "README's gate semantics define what a row verdict means (PASS, FAIL, SKIPPED, UNCOVERED, NO-ARTIFACTS), and README's own MVP-limitations section calls a skipped row a silent-pass hole — --strict-skipped exists precisely because skipped is the absence of evidence, not evidence. The rollup has to inherit that ontology rather than invent a new one, or the percentage would contradict the matrix that produced it."
  consequences: "+ the percentage can be traced row by row back to the matrix, and a decision with a skipped layer never shows proven. - partial progress is invisible in the headline number; only the per-decision mark shows it. - the denominator trusts declared status: rejecting a hard decision removes it from coverage, so the mark is gameable by status churn."
}

spec PROOF-002 {
  requirement PROOF-002-R1 (EARS) {
    text: "WHEN a decision's proof state is computed THEN it SHALL be proven only when every gatable row mapped to it passed, partial when at least one row passed and at least one did not, and unproven otherwise"
    layers: [unit]
    scenarios: [every_passing_row_marks_the_decision_proven, one_failing_row_marks_the_decision_partial, a_skipped_row_is_not_proof, a_decision_with_no_rows_is_unproven]
  }
  requirement PROOF-002-R2 (EARS) {
    text: "WHEN the headline percentage is computed THEN it SHALL count proven gatable decisions over all gatable decisions, excluding superseded and rejected ones, and SHALL not weight by scenario or row count"
    layers: [unit]
    scenarios: [superseded_and_rejected_decisions_are_excluded, headline_counts_decisions_not_scenarios]
  }
  scenario every_passing_row_marks_the_decision_proven {
    given: "a decision whose mapped matrix rows all passed"
    when: "the rollup computes its state"
    then: "the decision is proven"
    # delegates: decispec-gate coverage::tests::every_passing_row_marks_the_decision_proven
  }
  scenario one_failing_row_marks_the_decision_partial {
    given: "a decision with one passing row and one failing row"
    when: "the rollup computes its state"
    then: "the decision is partial, not proven"
    # delegates: decispec-gate coverage::tests::one_failing_row_marks_the_decision_partial
  }
  scenario a_skipped_row_is_not_proof {
    given: "a decision whose rows are all SKIPPED"
    when: "the rollup computes its state"
    then: "the decision is unproven because skipped is the absence of evidence"
    # delegates: decispec-gate coverage::tests::a_skipped_row_is_not_proof
  }
  scenario a_decision_with_no_rows_is_unproven {
    given: "a gatable decision whose requirements produced no matrix rows"
    when: "the rollup computes its state"
    then: "the decision is unproven rather than vacuously proven"
    # delegates: decispec-gate coverage::tests::a_decision_with_no_rows_is_unproven
  }
  scenario superseded_and_rejected_decisions_are_excluded {
    given: "a workspace mixing accepted, proposed, superseded, and rejected decisions"
    when: "the headline percentage is computed"
    then: "only accepted and proposed decisions appear in the numerator and the denominator"
    # delegates: decispec-gate coverage::tests::superseded_and_rejected_decisions_are_excluded
  }
  scenario headline_counts_decisions_not_scenarios {
    given: "a workspace where one decision has one scenario and another has five"
    when: "the headline percentage is computed"
    then: "each decision counts once, so the percentage divides proven decisions by gatable decisions"
    # delegates: decispec-gate coverage::tests::headline_counts_decisions_not_scenarios
    # (same glue also pins coverage::tests::empty_workspace_reports_zero_not_nan:
    # a zero-size denominator reports 0%, never a divide-by-zero)
  }
}

decision PROOF-003 "decispec coverage prints the marked rollup with an optional CI floor" {
  status: accepted
  context: "Console output is the one surface every project already has — CI, scripts, and humans all read it — and README's exit-code contract gives the command a natural mapping: an informational run exits 0, a --min floor breached is a finding (1), and a missing report is could-not-complete (2). The marks must also work piped into logs and on no-color terminals, so every state needs a symbol and a label, not color alone."
  consequences: "+ one command answers the question for any project, and --min turns the answer into a CI gate in a single flag. - the CLI surface grows another subcommand to document and keep stable. - a floor invites threshold gaming: teams may prove easy decisions first, which the per-decision marks at least make visible."
}

spec PROOF-003 {
  requirement PROOF-003-R1 (EARS) {
    text: "WHEN decispec coverage runs with a gate report present THEN it SHALL print one line per gatable decision carrying a mark and label for its proof state, then the headline percentage, and exit 0"
    layers: [unit]
    scenarios: [coverage_prints_marked_lines_and_headline]
  }
  requirement PROOF-003-R2 (EARS) {
    text: "WHEN --min is given and the headline percentage is below it THEN decispec coverage SHALL exit 1 naming both numbers"
    layers: [unit]
    scenarios: [coverage_below_the_min_floor_exits_1]
  }
  scenario coverage_prints_marked_lines_and_headline {
    given: "a report whose rows yield a mix of proven, partial, and unproven decisions"
    when: "decispec coverage runs"
    then: "each gatable decision prints with its mark and label and the final line reads coverage: N/M decisions proven (P%)"
    # delegates: decispec (cli) tests::coverage_prints_marked_lines_and_headline
  }
  scenario coverage_below_the_min_floor_exits_1 {
    given: "--min set above the current headline percentage"
    when: "decispec coverage runs"
    then: "it exits 1 and the message names the headline and the floor"
    # delegates: decispec (cli) tests::coverage_below_the_min_floor_exits_1
  }
}

decision PROOF-004 "The cockpit marks every decision with its proof state and shows the aggregate on the dashboard" {
  status: accepted
  context: "TUI-001 through TUI-006 built the spec cockpit where humans actually browse decisions, and its decision list already renders one row per decision with a status level — the natural home for the per-spec mark that was asked for. TUI-002 already mirrors report.json into the app, so the data path exists; what is missing is the rollup and the rendering. The dashboard already carries the last gate verdict, so the headline percentage belongs beside it."
  consequences: "+ proof status is visible exactly where decisions are read, per project across the registered fleet, using color plus a symbol so unstyled terminals and colorblind users read the same states. - the TUI's report mirror grows a coverage computation, and a project whose report predates its specs can show marks that disagree with the open files — the no-report state must say so rather than fake a zero."
}

spec PROOF-004 {
  requirement PROOF-004-R1 (EARS) {
    text: "WHEN the cockpit renders a project's spec list THEN every gatable decision SHALL carry a proof mark beside its status level"
    layers: [unit]
    scenarios: [tui_marks_each_decision_with_its_proof_state]
  }
  requirement PROOF-004-R2 (EARS) {
    text: "WHEN the dashboard renders THEN it SHALL show the headline percentage beside the last gate verdict, or an explicit no-gate-run state when no report exists"
    layers: [unit]
    scenarios: [tui_dashboard_shows_the_headline_percentage, tui_shows_no_gate_run_instead_of_a_fake_zero]
  }
  scenario tui_marks_each_decision_with_its_proof_state {
    given: "a registered project whose last gate report yields mixed proof states"
    when: "the spec list renders"
    then: "each decision row shows its proof mark alongside the accepted/proposed level, and the mark is not color-only"
    # delegates: decispec-tui views::tests::tui_marks_each_decision_with_its_proof_state
    # (same glue also pins snapshot::tests::coverage_rolls_up_mirror_rows, the
    # mirror→evidence mapping the marks render from)
  }
  scenario tui_dashboard_shows_the_headline_percentage {
    given: "a project with a gate report"
    when: "the dashboard renders"
    then: "the headline percentage appears beside the last gate verdict"
    # delegates: decispec-tui views::tests::tui_dashboard_shows_the_headline_percentage
  }
  scenario tui_shows_no_gate_run_instead_of_a_fake_zero {
    given: "a registered project with no gate report"
    when: "the dashboard or spec list renders"
    then: "coverage shows an explicit no-gate-run marker, never a fabricated zero percent"
    # delegates: decispec-tui views::tests::tui_shows_no_gate_run_instead_of_a_fake_zero
  }
}

decision PROOF-005 "The gate summary carries the proven percentage from the same matrix" {
  status: accepted
  context: "The number was asked for in the console, and the console surface developers already read after every run is the gate's own summary — a separate command they must discover will be read by fewer people. Because the percentage is a pure function of the matrix the gate just built, printing it costs one line and cannot disagree with the verdict. The summary must reuse PROOF-002's rollup rather than re-derive its own arithmetic, or the two surfaces could drift apart."
  consequences: "+ the percentage appears where developers already look, with no extra command for the common path. - the gate summary gains a line that CI log parsers did not expect, though the verdict line itself is unchanged. - the number reflects the matrix just built, so a skipped toolchain quietly lowers it — honest behavior, but a new red-looking number on projects that never installed their toolchains."
}

spec PROOF-005 {
  requirement PROOF-005-R1 (EARS) {
    text: "WHEN the gate prints its verdict THEN the summary SHALL include the headline proven percentage computed by the same rollup as decispec coverage"
    layers: [unit]
    scenarios: [gate_summary_prints_the_proven_percentage]
  }
  scenario gate_summary_prints_the_proven_percentage {
    given: "a gate run that just built its matrix"
    when: "the summary renders"
    then: "the verdict line is followed by the headline coverage line naming proven, gatable, and percent"
    # delegates: decispec (cli) tests::gate_summary_prints_the_proven_percentage
  }
}
