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
