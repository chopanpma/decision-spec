# DecisionSpec's own decision log — first bootstrap slice.
#
# Authored by hand (F28): `decispec extract` bootstraps Python/JS/TS projects
# only, so this draft follows docs/SPEC_AUTHORING.md instead. Every scenario
# delegates (tests/glue/__init__.py, tier 1) to the exact workspace Rust test
# that pins its behavior; the `# delegates:` comments name that test.

model {
  container cli "decispec CLI binary"
  container parse "spec parser and validator"
  container ir "typed workspace IR"
  container codegen "artifact renderer"
  container gate "compliance gate"
  container store "run history store"

  rel parse -> ir: "builds the Workspace"
  rel codegen -> ir: "reads the Workspace"
  rel gate -> ir: "reads decisions and layers"
  rel cli -> parse: "loads specs"
  rel cli -> codegen: "renders and writes artifacts"
  rel cli -> gate: "runs adapters and evaluates"
  rel cli -> store: "records every run"

  flow gate_run {
    cli -> gate: "evaluate workspace and JUnit results"
    gate --> cli: "GateReport and verdict"
  }
}

decision ARCH-001 "Library crates are pure: filesystem-in and report-out; every real write and process spawn lives in cli" {
  status: accepted
  context: "AGENTS.md's crate table fixes the split in the 'Never contains' column (codegen: filesystem writes; gate: process spawning; extract: filesystem writes), and ARCHITECTURE.md's patterns presuppose it: pure libraries are what make the 131 inline unit tests possible while the F10 audit (2026-10-01) recorded zero integration coverage of the binary."
  consequences: "+ libraries are testable with no disk fixtures, and cli is the single file to audit for I/O. - a library that needs I/O must grow a parameter or move to cli. - the boundary is convention, not enforced by the type system, until F10's integration suite lands."
}

spec ARCH-001 {
  requirement ARCH-001-R1 (EARS) {
    text: "WHEN a library crate renders artifacts, evaluates reports, or extracts a draft THEN it SHALL return its result in memory without writing files or spawning processes"
    layers: [unit]
    scenarios: [extract_stdout_writes_no_file, managed_paths_cover_only_gen_output_trees, failing_junit_yields_fail]
  }
  scenario extract_stdout_writes_no_file {
    given: "an existing Python/JS/TS project tree that analyze() accepted"
    when: "the draft spec is rendered with --stdout"
    then: "no spec file is created; the write path belongs to cli, not extract"
    # delegates: decispec (cli) tests::extract_stdout_writes_no_file — pins the
    # extract LIBRARY's purity at the command boundary: --stdout writes no file
  }
  scenario managed_paths_cover_only_gen_output_trees {
    given: "a workspace whose specs render to artifacts"
    when: "codegen computes the set of managed output paths"
    then: "the set covers only the gen output trees, so the cli-side sweep writes nothing else"
    # delegates: decispec-codegen tests::managed_paths_cover_only_gen_output_trees
  }
  scenario failing_junit_yields_fail {
    given: "a JUnit report supplied as an in-memory value with one failing testcase"
    when: "gate evaluates the report against the workspace"
    then: "the mapped row fails without gate touching the filesystem"
    # delegates: decispec-gate tests::failing_junit_yields_fail
  }
}

decision ARCH-002 "The exit vocabulary is exactly three codes: 0 clean, 1 completed-with-findings, 2 could-not-complete" {
  status: accepted
  context: "README's exit-code section is normative: a CI step treats 1 as 'there is work to do' and 2 as 'the run itself is broken', and bad CLI usage exits 2 so a typo never reads as a compliance finding. Cli tests pin all three codes across gen, gate, index, and extract."
  consequences: "+ every command speaks the same three-way contract; CI sketches stay three lines. - new failure kinds must fold into 1 or 2, never a fourth code. - the contract is unit-tested only; F10 covers the binary end-to-end."
}

spec ARCH-002 {
  requirement ARCH-002-R1 (EARS) {
    text: "WHEN any decispec command finishes THEN it SHALL exit 0 when clean, 1 when it completed with findings, and 2 when it could not complete, and no other code"
    layers: [unit]
    scenarios: [index_writes_decisions_md_and_exits_0, gate_exits_1_on_fail_verdict, gen_exits_2_when_specs_are_broken]
  }
  scenario index_writes_decisions_md_and_exits_0 {
    given: "a valid spec workspace"
    when: "decispec index runs"
    then: "docs/decisions.md is written and the exit code is 0"
    # delegates: decispec (cli) tests::index_writes_decisions_md_and_exits_0
  }
  scenario gate_exits_1_on_fail_verdict {
    given: "a workspace whose gate verdict is FAIL"
    when: "cmd_gate maps the verdict to an exit code"
    then: "the process exits 1"
    # delegates: decispec (cli) tests::gate_exits_1_on_fail_verdict
  }
  scenario gen_exits_2_when_specs_are_broken {
    given: "specs that fail parse or validation"
    when: "decispec gen runs"
    then: "it exits 2 because codegen has no workspace to work with"
    # delegates: decispec (cli) tests::gen_exits_2_when_specs_are_broken
  }
}

decision ARCH-003 "Every generated identifier is produced and matched through the ir::names registry" {
  status: accepted
  context: "ARCHITECTURE.md's Registry pattern: F6's whole safety argument rests on contract_scenario_test_name being called by both codegen and gate, so the emitted name and the matched name cannot drift. The F9 rule-direction and F23 label-escaping fixes were tractable precisely because each format string lives in exactly one place."
  consequences: "+ one format string per identifier, so a rename propagates to both emitter and matcher at compile time. - the registry is a discipline rather than a type: a hand-rolled format string elsewhere still compiles, so review is the enforcement."
}

spec ARCH-003 {
  requirement ARCH-003-R1 (EARS) {
    text: "WHEN codegen names a generated test and the gate matches a JUnit result THEN both SHALL derive the name through ir::names"
    layers: [unit]
    scenarios: [contract_scenario_test_name_joins_and_sanitizes, e2e_test_name_matches_playwright_title]
  }
  scenario contract_scenario_test_name_joins_and_sanitizes {
    given: "a contract-layer scenario id containing hyphens"
    when: "the scenario test name is generated"
    then: "the id is joined and sanitized to the pytest-safe form the gate will match"
    # delegates: decispec-ir tests::contract_scenario_test_name_joins_and_sanitizes
  }
  scenario e2e_test_name_matches_playwright_title {
    given: "an e2e-layer scenario"
    when: "the Playwright spec title is generated"
    then: "it equals the title string the gate matches JUnit results against"
    # delegates: decispec-ir tests::e2e_test_name_matches_playwright_title
  }
}

decision ARCH-004 "A missing toolchain is a Skipped value, and new toolchains are wired only in run_adapters()" {
  status: accepted
  context: "ARCHITECTURE.md's Adapter and Null Object patterns: run_adapters normalizes pytest, cucumber, karate, playwright, conftest, and import-linter into one AdapterRecord, which is what let F5 express strict-skipped as a policy on an existing value instead of a special case in error handling, and let F11 land [runners] overrides through the same seam."
  consequences: "+ gate never learns a toolchain's name or invocation; adding a tool touches one function. - adapter behavior for a new tool is assumed, not verified: contract/e2e reporter naming is an explicit MVP limitation for exactly this reason."
}

spec ARCH-004 {
  requirement ARCH-004-R1 (EARS) {
    text: "WHEN a toolchain binary is absent THEN decispec test SHALL record the layer as skipped and complete normally, exiting 1 only when an executed toolchain fails"
    layers: [unit]
    scenarios: [runner_override_missing_binary_skips_quietly_and_test_completes, skipped_toolchain_is_not_a_failure]
  }
  scenario runner_override_missing_binary_skips_quietly_and_test_completes {
    given: "a [runners] override whose binary does not exist on PATH"
    when: "decispec test probes and runs the adapters"
    then: "the layer is recorded as skipped and decispec test still completes and writes the manifest"
    # delegates: decispec (cli) tests::runner_override_missing_binary_skips_quietly_and_test_completes
  }
  scenario skipped_toolchain_is_not_a_failure {
    given: "an adapter record with status Skipped for a row's layer"
    when: "the gate evaluates the row"
    then: "the row is skipped rather than failed"
    # delegates: decispec-gate tests::skipped_toolchain_is_not_a_failure
  }
}

decision PARSE-001 "Spec errors are reported as file:line diagnostics, one line per error" {
  status: accepted
  context: "README:142 pins the shape — errors print as file:line: message — and both agents and CI parse these diagnostics (docs/USAGE.md troubleshooting). Parse tests pin the line reporting for duplicate ids, unterminated strings, and unknown layers."
  consequences: "+ errors are navigable in editors and greppable in logs. - diagnostics stop at the first error per phase (parse before cross-reference), so fixing one error can reveal the next."
}

spec PARSE-001 {
  requirement PARSE-001-R1 (EARS) {
    text: "WHEN a spec file contains an error THEN decispec check SHALL report it with the file path and line number"
    layers: [unit]
    scenarios: [duplicate_id_reports_second_definition_line, unterminated_string_reports_line]
  }
  scenario duplicate_id_reports_second_definition_line {
    given: "two spec blocks that declare the same id"
    when: "cross-reference validation runs"
    then: "the diagnostic points at the second definition's file and line"
    # delegates: decispec-parse tests::duplicate_id_reports_second_definition_line
  }
  scenario unterminated_string_reports_line {
    given: "a spec file whose string literal is never closed"
    when: "the parser fails"
    then: "the diagnostic names the line where the string started"
    # delegates: decispec-parse tests::unterminated_string_reports_line
  }
}
