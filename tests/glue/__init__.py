"""Hand-written glue for the DecisionSpec dogfood specs (specs/*.spec).

Tier-1 delegation (docs/SPEC_AUTHORING.md section 5): every scenario delegates to
the exact Rust test in this workspace that pins its behavior, run via
`cargo test -p <crate> --lib|--bins <test> -- --exact`.

A renamed or deleted Rust test fails loudly, never silently passes: the
`--exact` filter matches nothing, cargo exits 0 with '0 passed', and the
'1 passed' assertion below turns that into a glue failure.

Every one of the workspace's 282 Rust tests is pinned to exactly one scenario
(F28 + F29 + F33 + F36 + F26 + F38; F33 added the decispec-tui pins with their full module
paths, e.g. `app::tests::…`, because the tui tests live in per-module
`tests` submodules; F46 pinned the GEN-008 diagram-move tests; F47 pinned the
GATE-008/009 skip-honesty tests; F48 pinned the PORT-001/002 portable-spec
tests). The bijection is enforced mechanically by the F34
self-check — the `every_rust_test_pinned_exactly_once` scenario below fails
the gate on any unpinned test, ghost pin, or duplicate (core in
pin_audit.py, unit-tested in test_pin_audit.py), so the hand-audit ceremony
F29/F33 re-invented is retired. Regenerate specs and re-run `decispec gate`
to re-verify.
"""

import os
import subprocess

from . import pin_audit

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
CARGO = os.environ.get("CARGO", "cargo")


def _delegate(package, test_name, target="--lib"):
    cmd = [CARGO, "test", "-p", package, target, test_name, "--", "--exact"]
    proc = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)
    output = proc.stdout + proc.stderr
    assert proc.returncode == 0, "delegated test failed: %s\n%s" % (" ".join(cmd), output)
    assert "1 passed" in output, (
        "delegation ran zero tests (renamed Rust test?): %s\n%s" % (" ".join(cmd), output)
    )


# --- specs/000-decisions.spec (F28) ---------------------------------------

def extract_stdout_writes_no_file():
    _delegate("decispec", "tests::extract_stdout_writes_no_file", target="--bins")


def managed_paths_cover_only_gen_output_trees():
    _delegate("decispec-codegen", "tests::managed_paths_cover_only_gen_output_trees")


def failing_junit_yields_fail():
    _delegate("decispec-gate", "tests::failing_junit_yields_fail")


def index_writes_decisions_md_and_exits_0():
    _delegate("decispec", "tests::index_writes_decisions_md_and_exits_0", target="--bins")


def gate_exits_1_on_fail_verdict():
    _delegate("decispec", "tests::gate_exits_1_on_fail_verdict", target="--bins")


def gen_exits_2_when_specs_are_broken():
    _delegate("decispec", "tests::gen_exits_2_when_specs_are_broken", target="--bins")


def contract_scenario_test_name_joins_and_sanitizes():
    _delegate("decispec-ir", "tests::contract_scenario_test_name_joins_and_sanitizes")


def e2e_test_name_matches_playwright_title():
    _delegate("decispec-ir", "tests::e2e_test_name_matches_playwright_title")


def runner_override_missing_binary_skips_quietly_and_test_completes():
    _delegate("decispec", "tests::runner_override_missing_binary_skips_quietly_and_test_completes", target="--bins")


def skipped_toolchain_is_not_a_failure():
    _delegate("decispec-gate", "tests::skipped_toolchain_is_not_a_failure")


def duplicate_id_reports_second_definition_line():
    _delegate("decispec-parse", "tests::duplicate_id_reports_second_definition_line")


def unterminated_string_reports_line():
    _delegate("decispec-parse", "tests::unterminated_string_reports_line")


# --- specs/010-gate.spec (F29) ---------------------------------------------

def decision_with_no_requirements_fails():
    _delegate("decispec-gate", "tests::decision_with_no_requirements_fails")


def requirement_with_no_scenarios_fails():
    _delegate("decispec-gate", "tests::requirement_with_no_scenarios_fails")


def no_artifacts_is_structural_failure():
    _delegate("decispec-gate", "tests::no_artifacts_is_structural_failure")


def missing_result_for_executed_toolchain_is_uncovered():
    _delegate("decispec-gate", "tests::missing_result_for_executed_toolchain_is_uncovered")


def passing_junit_yields_pass():
    _delegate("decispec-gate", "tests::passing_junit_yields_pass")


def strict_skipped_escalates_to_fail():
    _delegate("decispec-gate", "tests::strict_skipped_escalates_to_fail")


def gate_strict_skipped_turns_skipped_rows_into_findings():
    _delegate("decispec", "tests::gate_strict_skipped_turns_skipped_rows_into_findings", target="--bins")


def strict_skipped_escalates_when_manifest_absent():
    _delegate("decispec-gate", "tests::strict_skipped_escalates_when_manifest_absent")


def strict_skipped_does_not_touch_no_artifacts():
    _delegate("decispec-gate", "tests::strict_skipped_does_not_touch_no_artifacts")


def strict_skipped_leaves_passing_project_passing():
    _delegate("decispec-gate", "tests::strict_skipped_leaves_passing_project_passing")


def contract_row_fails_only_for_its_own_failing_scenario():
    _delegate("decispec-gate", "tests::contract_row_fails_only_for_its_own_failing_scenario")


def contract_row_uncovered_when_no_result_for_that_scenario():
    _delegate("decispec-gate", "tests::contract_row_uncovered_when_no_result_for_that_scenario")


def contract_falls_back_to_suite_level_when_ids_do_not_match():
    _delegate("decispec-gate", "tests::contract_falls_back_to_suite_level_when_ids_do_not_match")


def contract_falls_back_when_only_the_classname_carries_the_spec_id():
    _delegate("decispec-gate", "tests::contract_falls_back_when_only_the_classname_carries_the_spec_id")


def e2e_row_fails_only_for_its_own_failing_scenario():
    _delegate("decispec-gate", "tests::e2e_row_fails_only_for_its_own_failing_scenario")


def superseded_decision_is_warning_not_failure():
    _delegate("decispec-gate", "tests::superseded_decision_is_warning_not_failure")


def junit_parsing_handles_testsuite_root():
    _delegate("decispec-gate", "tests::junit_parsing_handles_testsuite_root")


def junit_parsing_rejects_non_junit():
    _delegate("decispec-gate", "tests::junit_parsing_rejects_non_junit")


def gate_exits_2_when_specs_are_broken():
    _delegate("decispec", "tests::gate_exits_2_when_specs_are_broken", target="--bins")


# --- specs/010-gate.spec, GATE-007 (F39): console filters -------------------

def gate_with_filters_prints_a_narrowed_matrix_but_exit_follows_verdict():
    _delegate("decispec", "tests::gate_with_filters_prints_a_narrowed_matrix_but_exit_follows_verdict", target="--bins")


def gate_json_with_prefix_filter_is_narrowed_while_report_json_stays_full():
    _delegate("decispec", "tests::gate_json_with_prefix_filter_is_narrowed_while_report_json_stays_full", target="--bins")


def prefix_filter_matches_only_named_decision_prefixes():
    _delegate("decispec-gate", "tests::prefix_filter_matches_only_named_decision_prefixes")


def status_filter_matches_only_that_decision_status():
    _delegate("decispec-gate", "tests::status_filter_matches_only_that_decision_status")


def only_failing_matches_fail_rows_and_only_uncovered_matches_uncovered():
    _delegate("decispec-gate", "tests::only_failing_matches_fail_rows_and_only_uncovered_matches_uncovered")


def empty_filter_matches_every_row():
    _delegate("decispec-gate", "tests::empty_filter_matches_every_row")


def filter_flags_parse_prefix_csv_and_reject_unknown_status():
    _delegate("decispec", "tests::filter_flags_parse_prefix_csv_and_reject_unknown_status", target="--bins")


# --- specs/020-codegen.spec (F29) ------------------------------------------

def generated_header_detection_matches_first_two_lines():
    _delegate("decispec-codegen", "tests::generated_header_detection_matches_first_two_lines")


def regeneration_is_byte_identical():
    _delegate("decispec-codegen", "tests::regeneration_is_byte_identical")


def gen_removes_stale_artifacts_and_keeps_foreign_files():
    _delegate("decispec", "tests::gen_removes_stale_artifacts_and_keeps_foreign_files", target="--bins")


def sweep_does_not_create_managed_dirs_when_absent():
    _delegate("decispec", "tests::sweep_does_not_create_managed_dirs_when_absent", target="--bins")


def pytest_files_embed_spec_and_item_ids():
    _delegate("decispec-codegen", "tests::pytest_files_embed_spec_and_item_ids")


def contract_feature_is_idempotent():
    _delegate("decispec-codegen", "tests::contract_feature_is_idempotent")


def playwright_spec_shape_is_pinned():
    _delegate("decispec-codegen", "tests::playwright_spec_shape_is_pinned")


def rust_skeletons_emitted_only_for_rust_stack():
    _delegate("decispec-codegen", "tests::rust_skeletons_emitted_only_for_rust_stack")


def contract_e2e_infra_fitness_artifacts():
    _delegate("decispec-codegen", "tests::contract_e2e_infra_fitness_artifacts")


def contract_and_e2e_names_survive_norm():
    _delegate("decispec-ir", "tests::contract_and_e2e_names_survive_norm")


def mermaid_edge_labels_are_plain_ascii_only():
    _delegate("decispec-codegen", "tests::mermaid_edge_labels_are_plain_ascii_only")


def mermaid_edge_labels_drop_parens():
    _delegate("decispec-codegen", "tests::mermaid_edge_labels_drop_parens")


def mermaid_edge_labels_replace_pipe():
    _delegate("decispec-codegen", "tests::mermaid_edge_labels_replace_pipe")


def plantuml_edge_labels_keep_raw_parens():
    _delegate("decispec-codegen", "tests::plantuml_edge_labels_keep_raw_parens")


def model_diagrams_include_containers_and_rels():
    _delegate("decispec-codegen", "tests::model_diagrams_include_containers_and_rels")


def mermaid_flow_with_alt_else():
    _delegate("decispec-codegen", "tests::mermaid_flow_with_alt_else")


def mermaid_flow_with_nested_alt():
    _delegate("decispec-codegen", "tests::mermaid_flow_with_nested_alt")


def plantuml_flow_with_alt_else():
    _delegate("decispec-codegen", "tests::plantuml_flow_with_alt_else")


def plantuml_flow_with_nested_alt():
    _delegate("decispec-codegen", "tests::plantuml_flow_with_nested_alt")


def index_renders_title_mermaid_fences_and_tables():
    _delegate("decispec-codegen", "tests::index_renders_title_mermaid_fences_and_tables")


def index_escapes_table_cells():
    _delegate("decispec-codegen", "tests::index_escapes_table_cells")


def index_renders_superseded_by_line():
    _delegate("decispec-codegen", "tests::index_renders_superseded_by_line")


def index_honors_out_override():
    _delegate("decispec", "tests::index_honors_out_override", target="--bins")


def index_exits_2_when_specs_are_broken():
    _delegate("decispec", "tests::index_exits_2_when_specs_are_broken", target="--bins")


def importlinter_emits_contracts_only_for_python_mapped_rels():
    _delegate("decispec-codegen", "tests::importlinter_emits_contracts_only_for_python_mapped_rels")


def unmapped_containers_get_todos_instead_of_bogus_contracts():
    _delegate("decispec-codegen", "tests::unmapped_containers_get_todos_instead_of_bogus_contracts")


def fitness_modules_use_configured_container_packages():
    _delegate("decispec-codegen", "tests::fitness_modules_use_configured_container_packages")


def fitness_contract_identity_survives_config_changes():
    _delegate("decispec-codegen", "tests::fitness_contract_identity_survives_config_changes")


def depcruise_forbids_the_to_side_from_importing_the_from_side():
    _delegate("decispec-codegen", "tests::depcruise_forbids_the_to_side_from_importing_the_from_side")


def depcruise_paths_are_segment_anchored_without_rejected_regex_constructs():
    _delegate("decispec-codegen", "tests::depcruise_paths_are_segment_anchored_without_rejected_regex_constructs")


def depcruise_keeps_container_ids_as_filesystem_paths():
    _delegate("decispec-codegen", "tests::depcruise_keeps_container_ids_as_filesystem_paths")


def depcruise_config_documents_that_paths_match_container_directories():
    _delegate("decispec-codegen", "tests::depcruise_config_documents_that_paths_match_container_directories")


# --- specs/030-parse.spec (F29) --------------------------------------------

def unknown_decision_ref_for_spec():
    _delegate("decispec-parse", "tests::unknown_decision_ref_for_spec")


def unknown_scenario_ref_reports_requirement_line():
    _delegate("decispec-parse", "tests::unknown_scenario_ref_reports_requirement_line")


def unknown_layer_reports_file_and_line():
    _delegate("decispec-parse", "tests::unknown_layer_reports_file_and_line")


def unknown_superseded_by_target():
    _delegate("decispec-parse", "tests::unknown_superseded_by_target")


def undeclared_flow_participant():
    _delegate("decispec-parse", "tests::undeclared_flow_participant")


def bad_status_reports_expected_values():
    _delegate("decispec-parse", "tests::bad_status_reports_expected_values")


def parses_nested_alt():
    _delegate("decispec-parse", "tests::parses_nested_alt")


def nested_alt_uses_nested_container():
    _delegate("decispec-parse", "tests::nested_alt_uses_nested_container")


def rejects_unclosed_nested_alt():
    _delegate("decispec-parse", "tests::rejects_unclosed_nested_alt")


def parses_demo_spec():
    _delegate("decispec-parse", "tests::parses_demo_spec")


def demo_spec_validates_clean():
    _delegate("decispec-parse", "tests::demo_spec_validates_clean")


# --- specs/030-parse.spec, DSL-004 (F26): check --strict style lint ----------

def lint_flags_decisions_missing_context_or_consequences():
    _delegate("decispec-parse", "tests::lint_flags_decisions_missing_context_or_consequences")


def lint_flags_consequences_without_a_minus_entry():
    _delegate("decispec-parse", "tests::lint_flags_consequences_without_a_minus_entry")


def lint_flags_requirement_text_missing_ears_keywords():
    _delegate("decispec-parse", "tests::lint_flags_requirement_text_missing_ears_keywords")


def lint_flags_scenario_names_that_are_not_snake_case():
    _delegate("decispec-parse", "tests::lint_flags_scenario_names_that_are_not_snake_case")


def lint_flags_scenarios_never_listed_by_a_requirement():
    _delegate("decispec-parse", "tests::lint_flags_scenarios_never_listed_by_a_requirement")


def lint_reports_nothing_on_a_conforming_workspace():
    _delegate("decispec-parse", "tests::lint_reports_nothing_on_a_conforming_workspace")


def check_strict_reports_style_findings_and_exits_1():
    _delegate("decispec", "tests::check_strict_reports_style_findings_and_exits_1", target="--bins")


def check_without_strict_ignores_style_findings():
    _delegate("decispec", "tests::check_without_strict_ignores_style_findings", target="--bins")


# --- specs/040-commands.spec (F29) -----------------------------------------

def analyze_python_project_containers_and_rels():
    _delegate("decispec-extract", "tests::analyze_python_project_containers_and_rels")


def analyze_python_umbrella_project_promotes_children():
    _delegate("decispec-extract", "tests::analyze_python_umbrella_project_promotes_children")


def analyze_ts_project_maps_relative_imports_between_top_level_dirs():
    _delegate("decispec-extract", "tests::analyze_ts_project_maps_relative_imports_between_top_level_dirs")


def analyze_ts_umbrella_project_promotes_src_children():
    _delegate("decispec-extract", "tests::analyze_ts_umbrella_project_promotes_src_children")


def two_top_level_dirs_with_children_are_not_promoted():
    _delegate("decispec-extract", "tests::two_top_level_dirs_with_children_are_not_promoted")


def single_dir_with_only_loose_files_stays_one_container():
    _delegate("decispec-extract", "tests::single_dir_with_only_loose_files_stays_one_container")


def loose_root_py_files_land_in_a_project_named_container():
    _delegate("decispec-extract", "tests::loose_root_py_files_land_in_a_project_named_container")


def python_import_forms():
    _delegate("decispec-extract", "tests::python_import_forms")


def python_relative_imports_resolve_to_own_container():
    _delegate("decispec-extract", "tests::python_relative_imports_resolve_to_own_container")


def js_import_forms():
    _delegate("decispec-extract", "tests::js_import_forms")


def adr_madr_fixture_maps_fields():
    _delegate("decispec-extract", "tests::adr_madr_fixture_maps_fields")


def adr_status_from_heading_and_inline_line():
    _delegate("decispec-extract", "tests::adr_status_from_heading_and_inline_line")


def adr_case_insensitive_dirs_and_render_order():
    _delegate("decispec-extract", "tests::adr_case_insensitive_dirs_and_render_order")


def adr_slug_ids_scrubbed_and_collision_suffixed():
    _delegate("decispec-extract", "tests::adr_slug_ids_scrubbed_and_collision_suffixed")


def adr_long_context_flattened_and_id_collision_with_container():
    _delegate("decispec-extract", "tests::adr_long_context_flattened_and_id_collision_with_container")


def adr_missing_title_skipped_and_missing_status_warns():
    _delegate("decispec-extract", "tests::adr_missing_title_skipped_and_missing_status_warns")


def adr_excluded_dir_contributes_nothing():
    _delegate("decispec-extract", "tests::adr_excluded_dir_contributes_nothing")


def rendered_draft_parses_and_validates_with_the_real_parser():
    _delegate("decispec-extract", "tests::rendered_draft_parses_and_validates_with_the_real_parser")


def render_spec_shows_header_containers_and_rels():
    _delegate("decispec-extract", "tests::render_spec_shows_header_containers_and_rels")


def extract_writes_draft_with_decisions_from_adrs():
    _delegate("decispec", "tests::extract_writes_draft_with_decisions_from_adrs", target="--bins")


def empty_tree_reports_no_source_files():
    _delegate("decispec-extract", "tests::empty_tree_reports_no_source_files")


def non_directory_reports_not_a_directory():
    _delegate("decispec-extract", "tests::non_directory_reports_not_a_directory")


def extract_writes_draft_and_refuses_to_overwrite():
    _delegate("decispec", "tests::extract_writes_draft_and_refuses_to_overwrite", target="--bins")


def extract_with_no_source_files_is_cannot_complete():
    _delegate("decispec", "tests::extract_with_no_source_files_is_cannot_complete", target="--bins")


def extract_with_missing_path_arg_is_cannot_complete():
    _delegate("decispec", "tests::extract_with_missing_path_arg_is_cannot_complete", target="--bins")


def extract_without_project_root_is_cannot_complete():
    _delegate("decispec", "tests::extract_without_project_root_is_cannot_complete", target="--bins")


def extract_ignores_decispec_scaffold_when_detecting_language():
    _delegate("decispec", "tests::extract_ignores_decispec_scaffold_when_detecting_language", target="--bins")


def extract_ignores_glue_scaffold_in_python_projects():
    _delegate("decispec", "tests::extract_ignores_glue_scaffold_in_python_projects", target="--bins")


def container_ids_scrub_chars_outside_the_id_charset():
    _delegate("decispec-extract", "tests::container_ids_scrub_chars_outside_the_id_charset")


def container_ids_sanitize_and_dedupe_with_numeric_suffixes():
    _delegate("decispec-extract", "tests::container_ids_sanitize_and_dedupe_with_numeric_suffixes")


def scrubbed_id_collisions_still_get_distinct_suffixes():
    _delegate("decispec-extract", "tests::scrubbed_id_collisions_still_get_distinct_suffixes")


def more_than_30_containers_warn_about_grouping():
    _delegate("decispec-extract", "tests::more_than_30_containers_warn_about_grouping")


def noise_dirs_are_never_descended_into():
    _delegate("decispec-extract", "tests::noise_dirs_are_never_descended_into")


def title_escaping_handles_quotes_and_backslashes():
    _delegate("decispec-extract", "tests::title_escaping_handles_quotes_and_backslashes")


def language_tie_breaks_to_python():
    _delegate("decispec-extract", "tests::language_tie_breaks_to_python")


def js_family_without_ts_files_is_javascript():
    _delegate("decispec-extract", "tests::js_family_without_ts_files_is_javascript")


def analyze_with_exclusions_skips_scaffold_dirs():
    _delegate("decispec-extract", "tests::analyze_with_exclusions_skips_scaffold_dirs")


def config_parser_keeps_defaults_for_missing_keys():
    _delegate("decispec", "tests::config_parser_keeps_defaults_for_missing_keys", target="--bins")


def config_parser_strips_inline_comments():
    _delegate("decispec", "tests::config_parser_strips_inline_comments", target="--bins")


def config_reads_fitness_package_map():
    _delegate("decispec", "tests::config_reads_fitness_package_map", target="--bins")


def config_fitness_defaults_are_empty_and_unmapped():
    _delegate("decispec", "tests::config_fitness_defaults_are_empty_and_unmapped", target="--bins")


def config_reads_runners_table():
    _delegate("decispec", "tests::config_reads_runners_table", target="--bins")


def adapter_uses_runner_override_for_probe_and_run():
    _delegate("decispec", "tests::adapter_uses_runner_override_for_probe_and_run", target="--bins")


def fitness_toml_suggests_python_root_package_and_dotted_modules():
    _delegate("decispec-extract", "tests::fitness_toml_suggests_python_root_package_and_dotted_modules")


def fitness_toml_sanitizes_python_package_names():
    _delegate("decispec-extract", "tests::fitness_toml_sanitizes_python_package_names")


def fitness_toml_for_js_uses_dir_paths_and_warns_about_glue():
    _delegate("decispec-extract", "tests::fitness_toml_for_js_uses_dir_paths_and_warns_about_glue")


# --- specs/040-commands.spec, STORE-001 (F29) ------------------------------

def record_and_read_back():
    _delegate("decispec-store", "tests::record_and_read_back")


def opening_twice_reuses_schema():
    _delegate("decispec-store", "tests::opening_twice_reuses_schema")


def now_utc_formats_rfc3339ish():
    _delegate("decispec-store", "tests::now_utc_formats_rfc3339ish")


# --- specs/050-mcp.spec (F29) ----------------------------------------------

def tools_list_has_six_tools_with_schemas():
    _delegate("decispec-mcp", "tests::tools_list_has_six_tools_with_schemas")


def serve_io_dispatches_ndjson_over_buffers():
    _delegate("decispec-mcp", "tests::serve_io_dispatches_ndjson_over_buffers")


def initialize_result_and_notifications_silent():
    _delegate("decispec-mcp", "tests::initialize_result_and_notifications_silent")


def mcp_subcommand_parses_and_dispatches():
    _delegate("decispec", "tests::mcp_subcommand_parses_and_dispatches", target="--bins")


def validate_spec_reports_diagnostics_and_error_codes():
    _delegate("decispec-mcp", "tests::validate_spec_reports_diagnostics_and_error_codes")


def workspace_overview_lists_porting_gaps():
    _delegate("decispec-mcp", "tests::workspace_overview_lists_porting_gaps")


def gate_status_reports_missing_and_parses_report():
    _delegate("decispec-mcp", "tests::gate_status_reports_missing_and_parses_report")


def extract_project_returns_decisions_and_containers():
    _delegate("decispec-mcp", "tests::extract_project_returns_decisions_and_containers")

# --- specs/060-tui.spec (F33) ----------------------------------------------

def to_toml_and_parse_round_trip():
    _delegate("decispec-tui", "registry::tests::to_toml_and_parse_round_trip")


def parse_ignores_comments_and_blank_lines():
    _delegate("decispec-tui", "registry::tests::parse_ignores_comments_and_blank_lines")


def parse_keeps_hash_inside_quotes():
    _delegate("decispec-tui", "registry::tests::parse_keeps_hash_inside_quotes")


def parse_errors_on_key_outside_project_block():
    _delegate("decispec-tui", "registry::tests::parse_errors_on_key_outside_project_block")


def parse_errors_on_unknown_key_with_line_number():
    _delegate("decispec-tui", "registry::tests::parse_errors_on_unknown_key_with_line_number")


def parse_errors_on_entry_missing_path():
    _delegate("decispec-tui", "registry::tests::parse_errors_on_entry_missing_path")


def parse_errors_on_garbage_line():
    _delegate("decispec-tui", "registry::tests::parse_errors_on_garbage_line")


def parse_empty_source_is_empty_registry():
    _delegate("decispec-tui", "registry::tests::parse_empty_source_is_empty_registry")


def upsert_replaces_same_name_and_same_path():
    _delegate("decispec-tui", "registry::tests::upsert_replaces_same_name_and_same_path")


def parses_a_full_report():
    _delegate("decispec-tui", "snapshot::tests::parses_a_full_report")


def parses_kebab_case_row_results():
    _delegate("decispec-tui", "snapshot::tests::parses_kebab_case_row_results")


def tolerates_missing_optional_fields():
    _delegate("decispec-tui", "snapshot::tests::tolerates_missing_optional_fields")


def ignores_unknown_fields():
    _delegate("decispec-tui", "snapshot::tests::ignores_unknown_fields")


def garbage_is_an_error():
    _delegate("decispec-tui", "snapshot::tests::garbage_is_an_error")


def new_app_starts_on_first_project_in_specs_mode():
    _delegate("decispec-tui", "app::tests::new_app_starts_on_first_project_in_specs_mode")


def empty_registry_says_so_in_status():
    _delegate("decispec-tui", "app::tests::empty_registry_says_so_in_status")


def right_and_left_switch_projects_and_clamp():
    _delegate("decispec-tui", "app::tests::right_and_left_switch_projects_and_clamp")


def switching_project_resets_selection_and_scroll():
    _delegate("decispec-tui", "app::tests::switching_project_resets_selection_and_scroll")


def tab_cycles_modes():
    _delegate("decispec-tui", "app::tests::tab_cycles_modes")


def down_clamps_to_decision_count():
    _delegate("decispec-tui", "app::tests::down_clamps_to_decision_count")


def artifact_files_render_in_memory_with_index_last():
    _delegate("decispec-tui", "app::tests::artifact_files_render_in_memory_with_index_last")


def artifact_files_empty_for_broken_project():
    _delegate("decispec-tui", "app::tests::artifact_files_empty_for_broken_project")


def q_quits_r_reloads_other_chars_do_nothing():
    _delegate("decispec-tui", "app::tests::q_quits_r_reloads_other_chars_do_nothing")


def enter_pages_scroll_and_esc_resets():
    _delegate("decispec-tui", "app::tests::enter_pages_scroll_and_esc_resets")


def status_counts_by_decision_level():
    _delegate("decispec-tui", "app::tests::status_counts_by_decision_level")


def layer_counts_requirements_per_layer():
    _delegate("decispec-tui", "app::tests::layer_counts_requirements_per_layer")


def specs_mode_shows_projects_decisions_and_detail():
    _delegate("decispec-tui", "views::tests::specs_mode_shows_projects_decisions_and_detail")


def broken_project_shows_its_error_not_a_panic():
    _delegate("decispec-tui", "views::tests::broken_project_shows_its_error_not_a_panic")


def artifacts_mode_lists_generated_files_and_content():
    _delegate("decispec-tui", "views::tests::artifacts_mode_lists_generated_files_and_content")


def dashboard_shows_levels_layers_and_gate_verdict():
    _delegate("decispec-tui", "views::tests::dashboard_shows_levels_layers_and_gate_verdict")


# --- specs/040-commands.spec, CMD-007 (F33) ----------------------------------

def tui_subcommand_parses():
    _delegate("decispec", "tests::tui_subcommand_parses", target="--bins")


def tui_register_adds_project_and_persists_registry():
    _delegate("decispec", "tests::tui_register_adds_project_and_persists_registry", target="--bins")


def tui_register_rejects_a_non_project_dir():
    _delegate("decispec", "tests::tui_register_rejects_a_non_project_dir", target="--bins")


def tui_load_or_seed_writes_registry_on_first_run():
    _delegate("decispec", "tests::tui_load_or_seed_writes_registry_on_first_run", target="--bins")


def tui_registry_seeding_suggests_only_existing_dirs():
    _delegate("decispec", "tests::tui_registry_seeding_suggests_only_existing_dirs", target="--bins")


def tui_expand_home_leaves_absolute_and_plain_paths_alone():
    _delegate("decispec", "tests::tui_expand_home_leaves_absolute_and_plain_paths_alone", target="--bins")


def tui_load_project_keeps_broken_specs_as_error_state():
    _delegate("decispec", "tests::tui_load_project_keeps_broken_specs_as_error_state", target="--bins")


def tui_load_project_reads_gate_report_when_present():
    _delegate("decispec", "tests::tui_load_project_reads_gate_report_when_present", target="--bins")


def tui_translate_event_maps_presses_only():
    _delegate("decispec", "tests::tui_translate_event_maps_presses_only", target="--bins")

# --- specs/060-tui.spec, editor + PR flow (F33 pins F32's tests) -------------

def e_opens_the_file_picker():
    _delegate("decispec-tui", "app::tests::e_opens_the_file_picker")


def picker_lists_only_spec_sources():
    _delegate("decispec-tui", "app::tests::picker_lists_only_spec_sources")


def enter_loads_file_text_clean():
    _delegate("decispec-tui", "app::tests::enter_loads_file_text_clean")


def picker_shows_live_diagnostics_for_broken_files():
    _delegate("decispec-tui", "app::tests::picker_shows_live_diagnostics_for_broken_files")


def typing_marks_dirty_recomputes_diagnostics_and_cannot_quit():
    _delegate("decispec-tui", "app::tests::typing_marks_dirty_recomputes_diagnostics_and_cannot_quit")


def ctrl_s_saves_only_a_dirty_buffer():
    _delegate("decispec-tui", "app::tests::ctrl_s_saves_only_a_dirty_buffer")


def mark_saved_clears_dirty():
    _delegate("decispec-tui", "app::tests::mark_saved_clears_dirty")


def esc_confirms_before_discarding_dirty_buffer():
    _delegate("decispec-tui", "app::tests::esc_confirms_before_discarding_dirty_buffer")


def editing_after_confirm_disarms_it():
    _delegate("decispec-tui", "app::tests::editing_after_confirm_disarms_it")


def p_requests_a_pr_for_a_loaded_project():
    _delegate("decispec-tui", "app::tests::p_requests_a_pr_for_a_loaded_project")


def new_editor_starts_clean_at_origin():
    _delegate("decispec-tui", "editor::tests::new_editor_starts_clean_at_origin")


def insert_char_appends_at_cursor():
    _delegate("decispec-tui", "editor::tests::insert_char_appends_at_cursor")


def insert_unicode_is_char_safe():
    _delegate("decispec-tui", "editor::tests::insert_unicode_is_char_safe")


def newline_splits_line_and_moves_down():
    _delegate("decispec-tui", "editor::tests::newline_splits_line_and_moves_down")


def backspace_deletes_char_then_joins_lines():
    _delegate("decispec-tui", "editor::tests::backspace_deletes_char_then_joins_lines")


def backspace_at_origin_is_a_noop():
    _delegate("decispec-tui", "editor::tests::backspace_at_origin_is_a_noop")


def delete_removes_char_then_joins_with_next_line():
    _delegate("decispec-tui", "editor::tests::delete_removes_char_then_joins_with_next_line")


def arrow_moves_clamp_at_boundaries():
    _delegate("decispec-tui", "editor::tests::arrow_moves_clamp_at_boundaries")


def home_and_end():
    _delegate("decispec-tui", "editor::tests::home_and_end")


def page_moves_by_height_and_clamps():
    _delegate("decispec-tui", "editor::tests::page_moves_by_height_and_clamps")


def scroll_keeps_cursor_visible():
    _delegate("decispec-tui", "editor::tests::scroll_keeps_cursor_visible")


def validate_clean_edit_reports_nothing():
    _delegate("decispec-tui", "editor::tests::validate_clean_edit_reports_nothing")


def validate_reports_parse_error_phase_first():
    _delegate("decispec-tui", "editor::tests::validate_reports_parse_error_phase_first")


def validate_reports_cross_file_reference_problems():
    _delegate("decispec-tui", "editor::tests::validate_reports_cross_file_reference_problems")


def validate_reports_broken_sibling_files():
    _delegate("decispec-tui", "editor::tests::validate_reports_broken_sibling_files")


def validate_flags_undeclared_flow_participants():
    _delegate("decispec-tui", "editor::tests::validate_flags_undeclared_flow_participants")


def editor_picker_lists_spec_files():
    _delegate("decispec-tui", "views::tests::editor_picker_lists_spec_files")


def editor_shows_text_diagnostics_and_discard_warning():
    _delegate("decispec-tui", "views::tests::editor_shows_text_diagnostics_and_discard_warning")

# --- specs/040-commands.spec, CMD-008 (F33 pins F32's save/PR tests) ---------

def translate_event_maps_editor_keys_and_ctrl():
    _delegate("decispec", "tests::translate_event_maps_editor_keys_and_ctrl", target="--bins")


def save_spec_file_writes_and_reports_workspace_health():
    _delegate("decispec", "tests::save_spec_file_writes_and_reports_workspace_health", target="--bins")


def pr_readiness_flags_non_git_clean_tree_and_missing_gh():
    _delegate("decispec", "tests::pr_readiness_flags_non_git_clean_tree_and_missing_gh", target="--bins")


def pr_branch_name_is_utc_timestamped():
    _delegate("decispec", "tests::pr_branch_name_is_utc_timestamped", target="--bins")


def create_spec_pr_commits_pushes_then_reports_gh_failure():
    _delegate("decispec", "tests::create_spec_pr_commits_pushes_then_reports_gh_failure", target="--bins")


def create_spec_pr_refuses_changes_outside_specs_and_restores():
    _delegate("decispec", "tests::create_spec_pr_refuses_changes_outside_specs_and_restores", target="--bins")


def save_spec_file_creates_new_file_and_parent_dirs():
    _delegate("decispec", "tests::save_spec_file_creates_new_file_and_parent_dirs", target="--bins")


def save_spec_file_refuses_paths_outside_specs():
    _delegate("decispec", "tests::save_spec_file_refuses_paths_outside_specs", target="--bins")


# --- specs/040-commands.spec, CMD-009 (F40): query glob patterns ------------

def query_glob_match_follows_wildcard_semantics():
    _delegate("decispec", "tests::query_glob_match_follows_wildcard_semantics", target="--bins")


def query_pattern_selects_decisions_and_status_filters():
    _delegate("decispec", "tests::query_pattern_selects_decisions_and_status_filters", target="--bins")


def query_pattern_with_no_match_exits_1():
    _delegate("decispec", "tests::query_pattern_with_no_match_exits_1", target="--bins")


def query_exact_id_and_star_pattern_both_exit_0():
    _delegate("decispec", "tests::query_exact_id_and_star_pattern_both_exit_0", target="--bins")


# --- specs/040-commands.spec, CMD-010 (F41): gen --only ---------------------

def gen_only_writes_matching_artifacts_and_skips_the_sweep():
    _delegate("decispec", "tests::gen_only_writes_matching_artifacts_and_skips_the_sweep", target="--bins")


def render_all_for_selects_only_matching_spec_artifacts():
    _delegate("decispec-codegen", "tests::render_all_for_selects_only_matching_spec_artifacts")


def gen_only_with_unmatched_prefix_exits_1_and_writes_nothing():
    _delegate("decispec", "tests::gen_only_with_unmatched_prefix_exits_1_and_writes_nothing", target="--bins")


def render_all_for_with_no_prefixes_equals_render_all():
    _delegate("decispec-codegen", "tests::render_all_for_with_no_prefixes_equals_render_all")


# --- specs/060-tui.spec TUI-006 (F36): new spec file creation ---------------

def e_on_project_without_specs_opens_naming_directly():
    _delegate("decispec-tui", "app::tests::e_on_project_without_specs_opens_naming_directly")


def picker_n_opens_naming_and_esc_returns_to_picker():
    _delegate("decispec-tui", "app::tests::picker_n_opens_naming_and_esc_returns_to_picker")


def esc_in_naming_with_no_files_closes_the_editor():
    _delegate("decispec-tui", "app::tests::esc_in_naming_with_no_files_closes_the_editor")


def naming_enter_opens_editor_on_parseable_skeleton():
    _delegate("decispec-tui", "app::tests::naming_enter_opens_editor_on_parseable_skeleton")


def naming_enter_refuses_invalid_names_and_stays_in_naming():
    _delegate("decispec-tui", "app::tests::naming_enter_refuses_invalid_names_and_stays_in_naming")


def skeleton_for_new_file_parses_clean():
    _delegate("decispec-tui", "editor::tests::skeleton_for_new_file_parses_clean")


# --- specs/070-proof.spec (F38): proof coverage -------------------------------

def coverage_uses_last_report_without_running_toolchains():
    _delegate("decispec-gate", "coverage::tests::report_coverage_rolls_up_its_own_decisions")


def coverage_without_a_gate_report_exits_2():
    _delegate("decispec", "tests::coverage_without_a_gate_report_exits_2", target="--bins")
    _delegate("decispec", "tests::coverage_with_corrupt_report_exits_2", target="--bins")


def coverage_names_the_run_it_reflects():
    _delegate("decispec", "tests::format_utc_converts_known_epochs", target="--bins")


def every_passing_row_marks_the_decision_proven():
    _delegate("decispec-gate", "coverage::tests::every_passing_row_marks_the_decision_proven")


def one_failing_row_marks_the_decision_partial():
    _delegate("decispec-gate", "coverage::tests::one_failing_row_marks_the_decision_partial")


def a_skipped_row_is_not_proof():
    _delegate("decispec-gate", "coverage::tests::a_skipped_row_is_not_proof")


def a_decision_with_no_rows_is_unproven():
    _delegate("decispec-gate", "coverage::tests::a_decision_with_no_rows_is_unproven")


def superseded_and_rejected_decisions_are_excluded():
    _delegate("decispec-gate", "coverage::tests::superseded_and_rejected_decisions_are_excluded")


def headline_counts_decisions_not_scenarios():
    _delegate("decispec-gate", "coverage::tests::headline_counts_decisions_not_scenarios")
    _delegate("decispec-gate", "coverage::tests::empty_workspace_reports_zero_not_nan")


def coverage_prints_marked_lines_and_headline():
    _delegate("decispec", "tests::coverage_prints_marked_lines_and_headline", target="--bins")


def coverage_below_the_min_floor_exits_1():
    _delegate("decispec", "tests::coverage_below_the_min_floor_exits_1", target="--bins")


def gate_summary_prints_the_proven_percentage():
    _delegate("decispec", "tests::gate_summary_prints_the_proven_percentage", target="--bins")


def tui_marks_each_decision_with_its_proof_state():
    _delegate("decispec-tui", "views::tests::tui_marks_each_decision_with_its_proof_state")
    _delegate("decispec-tui", "snapshot::tests::coverage_rolls_up_mirror_rows")


def tui_dashboard_shows_the_headline_percentage():
    _delegate("decispec-tui", "views::tests::tui_dashboard_shows_the_headline_percentage")


def tui_shows_no_gate_run_instead_of_a_fake_zero():
    _delegate("decispec-tui", "views::tests::tui_shows_no_gate_run_instead_of_a_fake_zero")


# --- specs/060-tui.spec TUI-007 (F42): picker type-to-filter -----------------

def picker_slash_enters_filter_mode_and_typing_narrows_to_the_match():
    _delegate("decispec-tui", "app::tests::picker_slash_enters_filter_mode_and_typing_narrows_to_the_match")


def picker_filter_is_case_insensitive_and_selection_anchors_to_top():
    _delegate("decispec-tui", "app::tests::picker_filter_is_case_insensitive_and_selection_anchors_to_top")


def picker_esc_clears_filter_then_closes_and_n_still_names():
    _delegate("decispec-tui", "app::tests::picker_esc_clears_filter_then_closes_and_n_still_names")


def picker_filter_line_shows_query_and_match_count():
    _delegate("decispec-tui", "views::tests::picker_filter_line_shows_query_and_match_count")


# --- specs/060-tui.spec TUI-008 (F43): specs-view decision filter -------------

def tui_slash_engages_decision_filter_and_typing_narrows():
    _delegate("decispec-tui", "app::tests::tui_slash_engages_decision_filter_and_typing_narrows")


def tui_decision_filter_is_case_insensitive_and_reanchors_selection():
    _delegate("decispec-tui", "app::tests::tui_decision_filter_is_case_insensitive_and_reanchors_selection")


def tui_esc_clears_decision_filter_and_normal_keys_resume():
    _delegate("decispec-tui", "app::tests::tui_esc_clears_decision_filter_and_normal_keys_resume")


def tui_switching_project_resets_decision_filter():
    _delegate("decispec-tui", "app::tests::tui_switching_project_resets_decision_filter")
    _delegate("decispec-tui", "app::tests::tui_tab_resets_decision_filter_with_the_rest_of_the_view")


def specs_view_shows_filter_line_and_narrows_rows():
    _delegate("decispec-tui", "views::tests::specs_view_shows_filter_line_and_narrows_rows")


# --- specs/020-codegen.spec GEN-008 (F46): diagrams under .decispec ----------

def gen_writes_diagrams_under_decispec_dir():
    _delegate("decispec", "tests::gen_writes_diagrams_under_decispec_dir_not_docs", target="--bins")
    _delegate("decispec-codegen", "tests::diagram_artifacts_live_under_decispec_dir")


def gen_migrates_committed_diagrams_out_of_docs_diagrams():
    _delegate("decispec", "tests::sweep_migrates_header_diagrams_out_of_docs_diagrams", target="--bins")


def gen_diagrams_regeneration_is_byte_identical():
    _delegate("decispec", "tests::gen_twice_keeps_diagram_bytes_identical", target="--bins")


def sweep_treats_decispec_diagrams_as_the_managed_tree():
    _delegate("decispec", "tests::sweep_removes_stale_diagrams_from_decispec_dir_only", target="--bins")


# --- specs/010-gate.spec GATE-008/009 + 020 GEN-003 (F47): skip honesty -----

def junit_skipped_case_maps_to_skipped_row():
    _delegate("decispec-gate", "tests::junit_skipped_case_maps_to_skipped_row")


def suite_level_all_skipped_maps_to_skipped_row():
    _delegate("decispec-gate", "tests::suite_level_all_skipped_maps_to_skipped_row")


def strict_skipped_escalates_junit_skips_to_fail():
    _delegate("decispec-gate", "tests::strict_skipped_escalates_junit_skips_to_fail")


def allow_skips_relieves_listed_rows_only():
    _delegate("decispec-gate", "tests::allow_skips_relieves_listed_rows_only")


def allow_skips_warns_on_stale_entries():
    _delegate("decispec-gate", "tests::allow_skips_warns_on_stale_entries")


def gate_allow_skips_relieves_listed_skips_end_to_end():
    _delegate("decispec", "tests::gate_allow_skips_relieves_listed_skips_end_to_end", target="--bins")


def gate_allow_skips_missing_file_exits_2():
    _delegate("decispec", "tests::gate_allow_skips_missing_file_exits_2", target="--bins")


def playwright_stub_marks_fixme_not_silent_pass():
    _delegate("decispec-codegen", "tests::playwright_stub_marks_fixme_not_silent_pass")


# --- specs/080-portable.spec PORT-001/002 (F48): portable specs --------------

def extract_to_writes_named_spec_outside_the_repo():
    _delegate("decispec", "tests::extract_to_writes_named_spec_outside_the_repo", target="--bins")


def extract_to_refuses_to_overwrite_existing_file():
    _delegate("decispec", "tests::extract_to_refuses_to_overwrite_existing_file", target="--bins")


def extract_to_and_stdout_are_mutually_exclusive():
    _delegate("decispec", "tests::extract_to_and_stdout_are_mutually_exclusive", target="--bins")


def init_from_files_and_dirs_scaffolds_a_project():
    _delegate("decispec", "tests::init_from_files_and_dirs_scaffolds_a_project", target="--bins")


def init_from_refuses_to_clobber_existing_specs():
    _delegate("decispec", "tests::init_from_refuses_to_clobber_existing_specs", target="--bins")


def init_from_broken_specs_exit_2_naming_diagnostics():
    _delegate("decispec", "tests::init_from_broken_specs_exit_2_naming_diagnostics", target="--bins")


# --- specs/000-decisions.spec DOG-001 (F34): the pin self-check ------------
# No single Rust test to delegate to — the scenario IS the end-to-end check:
# enumerate the workspace, extract every pin above, assert the bijection.

def every_rust_test_pinned_exactly_once():
    enumerated = pin_audit.enumerate_workspace(CARGO, ROOT)
    with open(os.path.join(ROOT, "tests/glue/__init__.py")) as f:
        glue_source = f.read()
    problems = pin_audit.audit_pins(enumerated, pin_audit.extract_pins(glue_source))
    assert not problems, "pin self-check failed:\n" + "\n".join(problems)
