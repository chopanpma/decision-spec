# DSL semantics — prefix DSL.
# Every scenario delegates to the parse Rust test that pins it (F29).

decision DSL-002 "Every cross-reference in a spec must resolve to a declared item" {
  status: accepted
  context: "README: 'IDs must be unique across the workspace' and check 'enforces ID uniqueness and cross-references with file:line errors'. PARSE-001 pins the diagnostic shape; this decision pins the referential closure a valid workspace must have."
  consequences: "+ a workspace that checks clean cannot dangle: no spec without its decision, no requirement without its scenario, no flow participant that is not a container. - uniqueness is global, so two teams landing the same id collide even when their domains never meet (F24 wants per-owner scoping for exactly this)."
}

spec DSL-002 {
  requirement DSL-002-R1 (EARS) {
    text: "WHEN a workspace is validated THEN every spec-to-decision reference, requirement-to-scenario reference, layer name, flow participant, and superseded_by target SHALL resolve to a declared item or check SHALL report it with file and line"
    layers: [unit]
    scenarios: [unknown_decision_ref_for_spec, unknown_scenario_ref_reports_requirement_line, unknown_layer_reports_file_and_line, unknown_superseded_by_target, undeclared_flow_participant, bad_status_reports_expected_values]
  }
  scenario unknown_decision_ref_for_spec {
    given: "a spec block naming a decision that does not exist"
    when: "validation runs"
    then: "the error points at the spec block"
    # delegates: decispec-parse tests::unknown_decision_ref_for_spec
  }
  scenario unknown_scenario_ref_reports_requirement_line {
    given: "a requirement listing a scenario that does not exist"
    when: "validation runs"
    then: "the error points at the requirement line"
    # delegates: decispec-parse tests::unknown_scenario_ref_reports_requirement_line
  }
  scenario unknown_layer_reports_file_and_line {
    given: "a requirement naming a layer outside the pinned set"
    when: "validation runs"
    then: "the error names the file and line"
    # delegates: decispec-parse tests::unknown_layer_reports_file_and_line
  }
  scenario unknown_superseded_by_target {
    given: "a superseded decision whose superseded_by names nothing"
    when: "validation runs"
    then: "the error points at the dangling target"
    # delegates: decispec-parse tests::unknown_superseded_by_target
  }
  scenario undeclared_flow_participant {
    given: "a flow message whose endpoint is not a declared container"
    when: "validation runs"
    then: "the participant error names the flow"
    # delegates: decispec-parse tests::undeclared_flow_participant
  }
  scenario bad_status_reports_expected_values {
    given: "a decision whose status is not in the pinned enum"
    when: "validation runs"
    then: "the error lists the expected values"
    # delegates: decispec-parse tests::bad_status_reports_expected_values
  }
}

decision DSL-003 "Flow alt/else blocks nest to arbitrary depth and must be closed" {
  status: accepted
  context: "README flow semantics: 'either branch may itself contain alt blocks, to arbitrary depth', and an unclosed block is a parse error. F4 (ARCHITECTURE.md) is the cautionary tale: the validator once walked one level deep while renderers recursed; the traversal is now uniform, and these tests keep it that way."
  consequences: "+ flows can express nested decision trees without a depth limit. - the recursion is hand-written in every consumer of FlowStep, so a future consumer can reintroduce the F4 class of bug without these tests."
}

spec DSL-003 {
  requirement DSL-003-R1 (EARS) {
    text: "WHEN a flow contains alt blocks THEN they SHALL nest to arbitrary depth and parse and an unclosed block SHALL be a parse error"
    layers: [unit]
    scenarios: [parses_nested_alt, nested_alt_uses_nested_container, rejects_unclosed_nested_alt, parses_demo_spec, demo_spec_validates_clean]
  }
  scenario parses_nested_alt {
    given: "a flow with an alt inside an alt"
    when: "the spec is parsed"
    then: "the nested structure survives the parse"
    # delegates: decispec-parse tests::parses_nested_alt
  }
  scenario nested_alt_uses_nested_container {
    given: "a nested alt flow"
    when: "the flow is validated"
    then: "the nested container is walked, not just the top level"
    # delegates: decispec-parse tests::nested_alt_uses_nested_container
  }
  scenario rejects_unclosed_nested_alt {
    given: "a flow whose inner alt is never closed"
    when: "the spec is parsed"
    then: "parsing fails with a parse error"
    # delegates: decispec-parse tests::rejects_unclosed_nested_alt
  }
  scenario parses_demo_spec {
    given: "the README quickstart demo spec"
    when: "it is parsed"
    then: "it produces the documented IR"
    # delegates: decispec-parse tests::parses_demo_spec
  }
  scenario demo_spec_validates_clean {
    given: "the README quickstart demo spec"
    when: "it is validated"
    then: "no diagnostics are produced"
    # delegates: decispec-parse tests::demo_spec_validates_clean
  }
}

decision DSL-004 "Authoring-style rules are machine-checked as an opt-in lint under check --strict" {
  status: accepted
  context: "docs/SPEC_AUTHORING.md (F25, reviewed 2026-10-20) pins what makes a spec worth writing — EARS requirement text, decisions with context AND honest consequences, snake_case scenario ids, every scenario claimed by a requirement — and F29/F33/F34 each audited the workspace by hand, the same mechanical comparison re-invented thrice. Grammar legality (refs, statuses, uniqueness) already lives in validate and stays unconditional; style is opinionated and legitimate early-draft variation exists, so it is a flag instead of a hard failure."
  consequences: "+ one command grades the guide's mechanical subset and the hand-audit becomes code; check --strict on this repository itself is the lint's own bar. - the rules are lexical (keyword presence, minus-entry presence, name shape), so prose can be style-compliant and still worthless, and an author mid-draft stays on plain check until ready. - a new rule must stay green on this workspace or --strict is a lie on day one."
}

spec DSL-004 {
  requirement DSL-004-R1 (EARS) {
    text: "WHEN the style lint runs over a workspace THEN every decision SHALL have non-empty context and consequences carrying at least one minus entry, every requirement text SHALL contain a WHEN/IF/WHILE trigger and a SHALL response as separate words, every scenario id SHALL be snake_case, and every scenario SHALL be listed by a requirement, each violation reported with file and line"
    layers: [unit]
    scenarios: [lint_flags_decisions_missing_context_or_consequences, lint_flags_consequences_without_a_minus_entry, lint_flags_requirement_text_missing_ears_keywords, lint_flags_scenario_names_that_are_not_snake_case, lint_flags_scenarios_never_listed_by_a_requirement, lint_reports_nothing_on_a_conforming_workspace]
  }
  requirement DSL-004-R2 (EARS) {
    text: "WHEN decispec check runs without --strict THEN style findings SHALL NOT affect the exit code, and WHEN it runs with --strict THEN style findings SHALL exit 1 printed as file:line findings while a conforming workspace SHALL exit 0"
    layers: [unit]
    scenarios: [check_strict_reports_style_findings_and_exits_1, check_without_strict_ignores_style_findings]
  }
  scenario lint_flags_decisions_missing_context_or_consequences {
    given: "a decision with neither context nor consequences"
    when: "the style lint runs"
    then: "it reports one finding per missing field against the decision"
    # delegates: decispec-parse tests::lint_flags_decisions_missing_context_or_consequences
  }
  scenario lint_flags_consequences_without_a_minus_entry {
    given: "a decision whose consequences list only plus entries"
    when: "the style lint runs"
    then: "it reports that no minus entry is listed"
    # delegates: decispec-parse tests::lint_flags_consequences_without_a_minus_entry
  }
  scenario lint_flags_requirement_text_missing_ears_keywords {
    given: "requirement texts missing a SHALL response and texts missing a WHEN/IF/WHILE trigger"
    when: "the style lint runs"
    then: "each is reported as not EARS"
    # delegates: decispec-parse tests::lint_flags_requirement_text_missing_ears_keywords
  }
  scenario lint_flags_scenario_names_that_are_not_snake_case {
    given: "a scenario with a camel-case id"
    when: "the style lint runs"
    then: "it reports the name as not snake_case"
    # delegates: decispec-parse tests::lint_flags_scenario_names_that_are_not_snake_case
  }
  scenario lint_flags_scenarios_never_listed_by_a_requirement {
    given: "a scenario defined in a spec but listed by no requirement"
    when: "the style lint runs"
    then: "it reports the orphan scenario, since no gate row would cover it"
    # delegates: decispec-parse tests::lint_flags_scenarios_never_listed_by_a_requirement
  }
  scenario lint_reports_nothing_on_a_conforming_workspace {
    given: "a workspace that follows every mechanical style rule"
    when: "the style lint runs"
    then: "it reports nothing"
    # delegates: decispec-parse tests::lint_reports_nothing_on_a_conforming_workspace
  }
  scenario check_strict_reports_style_findings_and_exits_1 {
    given: "a project whose specs parse clean but break a style rule"
    when: "decispec check --strict runs"
    then: "the findings print as file:line lines and the command exits 1"
    # delegates: decispec tests::check_strict_reports_style_findings_and_exits_1
  }
  scenario check_without_strict_ignores_style_findings {
    given: "the same project with style-broken specs"
    when: "decispec check runs without --strict"
    then: "it exits 0, the lint staying opt-in"
    # delegates: decispec tests::check_without_strict_ignores_style_findings
  }
}
