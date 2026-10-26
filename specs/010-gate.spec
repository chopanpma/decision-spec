# Gate semantics — prefix GATE.
# Every scenario delegates to the gate/cli Rust test that pins it (F29).

decision GATE-001 "A gate row fails only for a named structural or evidentiary reason" {
  status: accepted
  context: "README 'Gate semantics' enumerates exactly five failure shapes (a-e): decision without requirements, requirement without scenarios, layer with zero artifacts, an executed failing test, and an item whose toolchain ran but produced no result. The gate crate's tests pin four of them directly; the fifth (failing test) is pinned under ARCH-001."
  consequences: "+ the matrix is explainable row by row; a red gate always names its reason. - the enumeration is closed by convention: a new failure kind needs a new rule, a test, and a README row together."
}

spec GATE-001 {
  requirement GATE-001-R1 (EARS) {
    text: "WHEN the gate builds the matrix THEN a row SHALL fail only when a decision lacks requirements, a requirement lacks scenarios, a layer has no generated artifacts, or the item's toolchain ran but produced no JUnit result"
    layers: [unit]
    scenarios: [decision_with_no_requirements_fails, requirement_with_no_scenarios_fails, no_artifacts_is_structural_failure, missing_result_for_executed_toolchain_is_uncovered, passing_junit_yields_pass]
  }
  scenario decision_with_no_requirements_fails {
    given: "an accepted decision with no requirements"
    when: "the gate evaluates the matrix"
    then: "the decision's row fails structurally"
    # delegates: decispec-gate tests::decision_with_no_requirements_fails
  }
  scenario requirement_with_no_scenarios_fails {
    given: "a requirement listing no scenarios"
    when: "the gate evaluates the matrix"
    then: "the requirement's row fails"
    # delegates: decispec-gate tests::requirement_with_no_scenarios_fails
  }
  scenario no_artifacts_is_structural_failure {
    given: "an item mapped to a layer whose generated directory is empty"
    when: "the gate evaluates the matrix"
    then: "the row reports NO-ARTIFACTS"
    # delegates: decispec-gate tests::no_artifacts_is_structural_failure
  }
  scenario missing_result_for_executed_toolchain_is_uncovered {
    given: "an item whose toolchain ran but wrote no result for it"
    when: "the gate evaluates the matrix"
    then: "the row is UNCOVERED"
    # delegates: decispec-gate tests::missing_result_for_executed_toolchain_is_uncovered
  }
  scenario passing_junit_yields_pass {
    given: "a JUnit report whose cases all pass"
    when: "the gate evaluates the matrix"
    then: "the mapped rows pass"
    # delegates: decispec-gate tests::passing_junit_yields_pass
  }
}

decision GATE-002 "Strict mode escalates skipped rows to failures and nothing else" {
  status: accepted
  context: "README: 'Pass --strict-skipped to close that hole in CI' — a missing toolchain is a silent pass by default, so CI needs the escalation. F5 built it as a policy on the existing Skipped value (ARCHITECTURE.md Null Object), and six tests pin the boundary: skipped becomes FAIL, passing projects stay passing, and structurally-failing rows are not double-counted."
  consequences: "+ CI gets one flag that turns toolchain absence red. - strict mode says nothing about rows that never had a toolchain (NO-ARTIFACTS stays its own failure); - local runs without the flag keep the silent-pass hole by design."
}

spec GATE-002 {
  requirement GATE-002-R1 (EARS) {
    text: "WHEN --strict-skipped is passed THEN every row whose toolchain did not run SHALL fail while passing projects and no-artifacts rows keep their existing verdicts"
    layers: [unit]
    scenarios: [strict_skipped_escalates_to_fail, gate_strict_skipped_turns_skipped_rows_into_findings, strict_skipped_escalates_when_manifest_absent, strict_skipped_does_not_touch_no_artifacts, strict_skipped_leaves_passing_project_passing]
  }
  scenario strict_skipped_escalates_to_fail {
    given: "an adapter record with status Skipped"
    when: "the gate evaluates with --strict-skipped"
    then: "the row fails and report.json marks it strict-escalated"
    # delegates: decispec-gate tests::strict_skipped_escalates_to_fail
  }
  scenario gate_strict_skipped_turns_skipped_rows_into_findings {
    given: "a gate run with skipped rows"
    when: "cmd_gate maps a strict-mode FAIL verdict"
    then: "the process exits 1"
    # delegates: decispec (cli) tests::gate_strict_skipped_turns_skipped_rows_into_findings
  }
  scenario strict_skipped_escalates_when_manifest_absent {
    given: "a --results directory with no manifest at all"
    when: "the gate evaluates with --strict-skipped"
    then: "rows fail rather than passing on the empty directory"
    # delegates: decispec-gate tests::strict_skipped_escalates_when_manifest_absent
  }
  scenario strict_skipped_does_not_touch_no_artifacts {
    given: "a row already failing structurally for missing artifacts"
    when: "the gate evaluates with --strict-skipped"
    then: "the row is not counted twice"
    # delegates: decispec-gate tests::strict_skipped_does_not_touch_no_artifacts
  }
  scenario strict_skipped_leaves_passing_project_passing {
    given: "a project whose executed tests all pass"
    when: "the gate evaluates with --strict-skipped"
    then: "the verdict stays PASS"
    # delegates: decispec-gate tests::strict_skipped_leaves_passing_project_passing
  }
}

decision GATE-003 "Contract and e2e results map per item by generated name, degrading to suite level with a warning" {
  status: accepted
  context: "README MVP limitations: precise per-item matching 'degrades to suite level' when a JUnit name does not carry the generated id, with a printed WARNING naming the item; F6's names registry (ARCH-003) is what makes the precise path possible, and the classname-only probe exists because cucumber reports the feature path there."
  consequences: "+ one broken scenario fails only its own row; a naming drift never silently reds unrelated rows. - suite-level fallback can mask a partially matching reporter, which is why the gate prints the WARNING instead of staying silent."
}

spec GATE-003 {
  requirement GATE-003-R1 (EARS) {
    text: "WHEN a contract or e2e JUnit result matches a generated item id THEN only that item's row SHALL take the result and when no name matches the suite SHALL be judged as a whole with a warning"
    layers: [unit]
    scenarios: [contract_row_fails_only_for_its_own_failing_scenario, contract_row_uncovered_when_no_result_for_that_scenario, contract_falls_back_to_suite_level_when_ids_do_not_match, contract_falls_back_when_only_the_classname_carries_the_spec_id, e2e_row_fails_only_for_its_own_failing_scenario]
  }
  scenario contract_row_fails_only_for_its_own_failing_scenario {
    given: "one failing contract scenario among several"
    when: "the gate maps results per item"
    then: "only that scenario's row fails"
    # delegates: decispec-gate tests::contract_row_fails_only_for_its_own_failing_scenario
  }
  scenario contract_row_uncovered_when_no_result_for_that_scenario {
    given: "a contract scenario with no JUnit result"
    when: "the toolchain ran"
    then: "that row alone is UNCOVERED"
    # delegates: decispec-gate tests::contract_row_uncovered_when_no_result_for_that_scenario
  }
  scenario contract_falls_back_to_suite_level_when_ids_do_not_match {
    given: "results whose names carry no generated id"
    when: "per-item matching finds nothing"
    then: "the suite is judged whole and a warning names the item"
    # delegates: decispec-gate tests::contract_falls_back_to_suite_level_when_ids_do_not_match
  }
  scenario contract_falls_back_when_only_the_classname_carries_the_spec_id {
    given: "a cucumber report with the feature path in classname only"
    when: "per-item matching probes names and classname"
    then: "the classname match keeps the row precise"
    # delegates: decispec-gate tests::contract_falls_back_when_only_the_classname_carries_the_spec_id
  }
  scenario e2e_row_fails_only_for_its_own_failing_scenario {
    given: "one failing e2e scenario among several"
    when: "the gate maps results per item"
    then: "only that scenario's row fails"
    # delegates: decispec-gate tests::e2e_row_fails_only_for_its_own_failing_scenario
  }
}

decision GATE-004 "A superseded decision warns instead of hard-failing the gate" {
  status: accepted
  context: "README gate semantics: superseded decisions are excluded from hard failures, with coverage-drift detection if tests still map to them. That keeps history in the spec without letting retired decisions block the build."
  consequences: "+ old decisions stay documented and queryable at zero gate cost. - nothing forces removal of drifted coverage; the warning is easy to ignore for months."
}

spec GATE-004 {
  requirement GATE-004-R1 (EARS) {
    text: "WHEN a decision's status is superseded THEN its rows SHALL produce warnings rather than hard failures"
    layers: [unit]
    scenarios: [superseded_decision_is_warning_not_failure]
  }
  scenario superseded_decision_is_warning_not_failure {
    given: "a superseded decision with requirements"
    when: "the gate evaluates the matrix"
    then: "it is listed as a warning, not a failure"
    # delegates: decispec-gate tests::superseded_decision_is_warning_not_failure
  }
}

decision GATE-005 "JUnit input is parsed strictly: testsuite roots accepted, non-JUnit rejected" {
  status: accepted
  context: "The gate consumes whatever adapters emit (roxmltree), and the two parsing tests pin both ends of the contract: a testsuite-rooted document is accepted, and XML that is not JUnit is rejected rather than misread."
  consequences: "+ malformed adapter output surfaces as a parse error instead of a nonsense matrix. - the parser accepts any testsuite-shaped XML; a tool emitting nonstandard but valid JUnit may still need adapter massaging first."
}

spec GATE-005 {
  requirement GATE-005-R1 (EARS) {
    text: "WHEN the gate reads a result file THEN it SHALL accept JUnit with a testsuite root and reject XML that is not JUnit"
    layers: [unit]
    scenarios: [junit_parsing_handles_testsuite_root, junit_parsing_rejects_non_junit]
  }
  scenario junit_parsing_handles_testsuite_root {
    given: "a result file rooted at testsuite instead of testsuites"
    when: "the gate parses it"
    then: "its cases are read"
    # delegates: decispec-gate tests::junit_parsing_handles_testsuite_root
  }
  scenario junit_parsing_rejects_non_junit {
    given: "an XML document with no JUnit shape"
    when: "the gate parses it"
    then: "parsing fails instead of yielding an empty matrix"
    # delegates: decispec-gate tests::junit_parsing_rejects_non_junit
  }
}

decision GATE-006 "A gate that cannot load its workspace exits 2 without a verdict" {
  status: accepted
  context: "README's deliberate asymmetry: check reports malformed specs as findings (exit 1) because diagnosing specs is its job, while gen and gate abort on the same specs and exit 2 — they have nothing to work with. The cli pins the gate half of that contract."
  consequences: "+ CI never mistakes a spec typo for a compliance violation. - the exit-2 path produces no matrix, so a broken run and a violated gate are distinguishable only by exit code, which is exactly the intent."
}

spec GATE-006 {
  requirement GATE-006-R1 (EARS) {
    text: "WHEN specs fail parse or validation THEN gate SHALL exit 2 without rendering a verdict"
    layers: [unit]
    scenarios: [gate_exits_2_when_specs_are_broken]
  }
  scenario gate_exits_2_when_specs_are_broken {
    given: "specs that fail validation"
    when: "cmd_gate runs"
    then: "it exits 2 because no matrix can be built"
    # delegates: decispec (cli) tests::gate_exits_2_when_specs_are_broken
  }
}

decision GATE-007 "Console filters narrow what the gate prints and never what it records" {
  status: accepted
  context: "Asked for on 2026-10-20: slicing the matrix in the terminal (by decision prefix, by status, to failing or uncovered rows only) so a human or agent reading a 200-row gate does not need a pager. The verdict, counts, failures, the persisted .decispec/report.json, and the store record are the compliance record — a display concern must not be able to move an exit code or a stored run, or CI and the terminal would disagree about the same gate."
  consequences: "+ one flags slice any matrix in the console without tools outside the gate, and a filter that hides everything still cannot green a red build. - the printed matrix and report.json can now differ by design, so a log excerpt is not the record — the file is, and the output says so when a filter is active. - four more flags to keep stable on the gate, the most CI-visible command."
}

spec GATE-007 {
  requirement GATE-007-R1 (EARS) {
    text: "WHEN gate runs with console filters THEN the verdict, counts, failures, warnings, persisted report, and recorded run SHALL be those of the full unfiltered matrix, and the printed output SHALL say a filter is active or that no rows match"
    layers: [unit]
    scenarios: [gate_with_filters_prints_a_narrowed_matrix_but_exit_follows_verdict, gate_json_with_prefix_filter_is_narrowed_while_report_json_stays_full]
  }
  requirement GATE-007-R2 (EARS) {
    text: "WHEN the filter criteria are applied THEN decision-id prefixes SHALL match as OR, a status criterion SHALL restrict to decisions of that status, only-failing and only-uncovered SHALL restrict rows and combine as AND, an empty filter SHALL match every row, and an unknown status name SHALL be rejected as bad usage"
    layers: [unit]
    scenarios: [prefix_filter_matches_only_named_decision_prefixes, status_filter_matches_only_that_decision_status, only_failing_matches_fail_rows_and_only_uncovered_matches_uncovered, empty_filter_matches_every_row, filter_flags_parse_prefix_csv_and_reject_unknown_status]
  }
  scenario gate_with_filters_prints_a_narrowed_matrix_but_exit_follows_verdict {
    given: "a project whose gate verdict is FAIL"
    when: "gate runs with a matching filter and with a filter matching nothing"
    then: "the exit is findings both times, the verdict never following the filter"
    # delegates: decispec tests::gate_with_filters_prints_a_narrowed_matrix_but_exit_follows_verdict
  }
  scenario gate_json_with_prefix_filter_is_narrowed_while_report_json_stays_full {
    given: "a FAIL-verdict project and a prefix filter matching no decision"
    when: "gate runs with --json and the filter"
    then: "the persisted report.json still names every decision the full run produced"
    # delegates: decispec tests::gate_json_with_prefix_filter_is_narrowed_while_report_json_stays_full
  }
  scenario prefix_filter_matches_only_named_decision_prefixes {
    given: "a matrix with CMD, DSL, and TUI decisions"
    when: "the prefix filter is applied"
    then: "only decisions whose id starts with a named prefix remain, prefixes OR-ed"
    # delegates: decispec-gate tests::prefix_filter_matches_only_named_decision_prefixes
  }
  scenario status_filter_matches_only_that_decision_status {
    given: "a matrix mixing accepted, proposed, and superseded decisions"
    when: "the status filter is applied"
    then: "only decisions carrying that status remain"
    # delegates: decispec-gate tests::status_filter_matches_only_that_decision_status
  }
  scenario only_failing_matches_fail_rows_and_only_uncovered_matches_uncovered {
    given: "a matrix with passing, failing, and uncovered rows"
    when: "the only-failing and only-uncovered criteria are applied alone and together"
    then: "each keeps only its own row results and together they match nothing"
    # delegates: decispec-gate tests::only_failing_matches_fail_rows_and_only_uncovered_matches_uncovered
  }
  scenario empty_filter_matches_every_row {
    given: "a report with no filter criteria set"
    when: "the filter is applied"
    then: "every row remains and the verdict, counts, failures, and warnings stay those of the full report"
    # delegates: decispec-gate tests::empty_filter_matches_every_row
  }
  scenario filter_flags_parse_prefix_csv_and_reject_unknown_status {
    given: "the gate's filter flags, including a comma-separated prefix list and an unknown status name"
    when: "they are parsed"
    then: "prefixes split and trim, known statuses map, and the unknown name is rejected as bad usage"
    # delegates: decispec tests::filter_flags_parse_prefix_csv_and_reject_unknown_status
  }
}

decision GATE-008 "A JUnit <skipped> case is a SKIPPED row, never a silent pass, and strict mode escalates it like any other skip" {
  status: accepted
  context: "Real toolchains report intentionally-unimplemented work with JUnit's <skipped/> element (Playwright's test.fixme and test.skip, pytest's @pytest.mark.skip), but the gate's parser only read failure/error children, so a skipped case was indistinguishable from a passing one — 86 false-green e2e rows in the tvgo port before TASH-515, per the user's retro table (F47, 2026-10-23). The fix sits in parse_junit: <skipped> becomes row evidence, and the row reuses the same skipped_outcome seam as toolchain skips so --strict-skipped escalates both with one policy."
  consequences: "+ an intentionally skipped test is visible in the matrix and never inflates proof coverage, and CI strict mode reds it unless the project allowlists it (GATE-009). - a suite whose cases are ALL skipped now reads SKIPPED instead of the old coarse 'N passed', which can flip legacy projects' rows from pass to skipped until stubs are implemented or allowlisted — a one-time honest accounting, not a regression."
}

spec GATE-008 {
  requirement GATE-008-R1 (EARS) {
    text: "WHEN a JUnit testcase carries a <skipped> element THEN the mapped row SHALL be SKIPPED with the skip message in its detail and SHALL never be counted as passed"
    layers: [unit]
    scenarios: [junit_skipped_case_maps_to_skipped_row, suite_level_all_skipped_maps_to_skipped_row]
  }
  requirement GATE-008-R2 (EARS) {
    text: "WHEN --strict-skipped is active THEN JUnit-evidenced skips SHALL escalate to FAIL with the strict detail, exactly like toolchain skips, and the NoArtifacts precedence SHALL be unchanged"
    layers: [unit]
    scenarios: [strict_skipped_escalates_junit_skips_to_fail]
  }
  scenario junit_skipped_case_maps_to_skipped_row {
    given: "a JUnit case whose only deviation from passing is a <skipped> element"
    when: "the gate maps it to its scenario row"
    then: "the row is SKIPPED, the detail carries the skip message, and a non-strict verdict is not findings"
    # delegates: decispec-gate tests::junit_skipped_case_maps_to_skipped_row
  }
  scenario suite_level_all_skipped_maps_to_skipped_row {
    given: "a contract/e2e suite that fell back to whole-suite matching with every case skipped"
    when: "the gate judges the suite as a whole"
    then: "the row is SKIPPED, not the old 'N passed'"
    # delegates: decispec-gate tests::suite_level_all_skipped_maps_to_skipped_row
  }
  scenario strict_skipped_escalates_junit_skips_to_fail {
    given: "a row whose evidence is a JUnit-skipped case"
    when: "the gate evaluates with --strict-skipped"
    then: "the row fails with the strict detail, as toolchain skips already do"
    # delegates: decispec-gate tests::strict_skipped_escalates_junit_skips_to_fail
  }
}

decision GATE-009 "A skip allowlist relieves named rows from strict escalation, and drift is a warning" {
  status: accepted
  context: "Strict mode (GATE-002, GATE-008) turns every skip red, but a porting project carries known, intentional gaps for weeks. The user's retro table calls the allowlist 'the ratchet, and every project gets it': `decispec gate --allow-skips specs/skips.toml` names each row allowed to stay skipped, so CI is strict everywhere else. The file is a list of [[skips]] tables (decision, item, reason; requirement optional), versioned next to the specs it excuses."
  consequences: "+ strict CI and honest stubs coexist: a gap is either implemented, allowlisted with a reason, or red — there is no silent fourth state. - one more hand-maintained file; an entry that matches no skipped row has drifted, so the gate warns on it instead of letting the list rot silently."
}

spec GATE-009 {
  requirement GATE-009-R1 (EARS) {
    text: "WHEN --allow-skips names a readable file and --strict-skipped is active THEN a skipped row matched by an entry SHALL remain SKIPPED with the allowance reason in its detail while every unmatched skipped row SHALL still escalate to FAIL"
    layers: [unit]
    scenarios: [allow_skips_relieves_listed_rows_only, gate_allow_skips_relieves_listed_skips_end_to_end]
  }
  requirement GATE-009-R2 (EARS) {
    text: "WHEN the allowlist file is missing or unparseable THEN gate SHALL exit 2 without a verdict, and WHEN an entry matches no skipped row THEN the report SHALL carry a stale-allowance warning"
    layers: [unit]
    scenarios: [gate_allow_skips_missing_file_exits_2, allow_skips_warns_on_stale_entries]
  }
  scenario allow_skips_relieves_listed_rows_only {
    given: "a strict run with two JUnit-skipped rows and an allowlist naming one of them"
    when: "the gate evaluates"
    then: "the listed row stays SKIPPED with its reason and the other fails"
    # delegates: decispec-gate tests::allow_skips_relieves_listed_rows_only
  }
  scenario gate_allow_skips_relieves_listed_skips_end_to_end {
    given: "a project whose only deviations are allowlisted skips"
    when: "cmd_gate runs with --strict-skipped and --allow-skips"
    then: "the process exits 0 and the verdict names the allowance"
    # delegates: decispec (cli) tests::gate_allow_skips_relieves_listed_skips_end_to_end
  }
  scenario gate_allow_skips_missing_file_exits_2 {
    given: "a --allow-skips path that does not exist"
    when: "cmd_gate runs"
    then: "it exits 2 because the ratchet cannot be read"
    # delegates: decispec (cli) tests::gate_allow_skips_missing_file_exits_2
  }
  scenario allow_skips_warns_on_stale_entries {
    given: "an allowlist whose entry matches no skipped row"
    when: "the gate evaluates"
    then: "the report carries a stale-allowance warning for that entry"
    # delegates: decispec-gate tests::allow_skips_warns_on_stale_entries
  }
}
