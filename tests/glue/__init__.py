"""Hand-written glue for the DecisionSpec dogfood specs (specs/*.spec).

Tier-1 delegation (docs/SPEC_AUTHORING.md section 5): every scenario delegates to
the exact Rust test in this workspace that pins its behavior, run via
`cargo test -p <crate> --lib|--bins <test> -- --exact`.

A renamed or deleted Rust test fails loudly, never silently passes: the
`--exact` filter matches nothing, cargo exits 0 with '0 passed', and the
'1 passed' assertion below turns that into a glue failure.

Every one of the workspace's 131 Rust tests is pinned to exactly one scenario
(F28 + F29); regenerate specs and re-run `decispec gate` to re-verify.
"""

import os
import subprocess

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
