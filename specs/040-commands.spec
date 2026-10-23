# Commands: extract and config — prefix CMD; store — prefix STORE.
# Every scenario delegates to the extract/cli/store Rust test that pins it (F29).

decision CMD-001 "Containers come from top-level directories, promoting an umbrella src dir's children" {
  status: accepted
  context: "README extract: 'containers from top-level directories — or from the children of a single umbrella directory that holds the code', relationships from the import graph. This layout rule was tuned on real ports (tvgo's umbrella src/ promotion) and its inverse: two real top-level dirs are never promoted."
  consequences: "+ a standard layout produces a usable model with zero configuration. - unconventional layouts (code at repo root in one flat dir, multiple umbrellas) get a coarse or surprising container split."
}

spec CMD-001 {
  requirement CMD-001-R1 (EARS) {
    text: "WHEN extract analyzes a project THEN containers SHALL come from top-level directories or from the children of a single umbrella directory holding the code and relationships SHALL come from the import graph"
    layers: [unit]
    scenarios: [analyze_python_project_containers_and_rels, analyze_python_umbrella_project_promotes_children, analyze_ts_project_maps_relative_imports_between_top_level_dirs, analyze_ts_umbrella_project_promotes_src_children, two_top_level_dirs_with_children_are_not_promoted, single_dir_with_only_loose_files_stays_one_container, loose_root_py_files_land_in_a_project_named_container, python_import_forms, python_relative_imports_resolve_to_own_container, js_import_forms]
  }
  scenario analyze_python_project_containers_and_rels {
    given: "a python project with top-level packages importing each other"
    when: "analyze runs"
    then: "containers and import-graph relationships are recovered"
    # delegates: decispec-extract tests::analyze_python_project_containers_and_rels
  }
  scenario analyze_python_umbrella_project_promotes_children {
    given: "a python project whose code lives under one umbrella directory"
    when: "analyze runs"
    then: "the umbrella's children become the containers"
    # delegates: decispec-extract tests::analyze_python_umbrella_project_promotes_children
  }
  scenario analyze_ts_project_maps_relative_imports_between_top_level_dirs {
    given: "a JS/TS project with relative imports between top-level directories"
    when: "analyze runs"
    then: "the imports map to relationships between those containers"
    # delegates: decispec-extract tests::analyze_ts_project_maps_relative_imports_between_top_level_dirs
  }
  scenario analyze_ts_umbrella_project_promotes_src_children {
    given: "a JS/TS project whose code lives under src/"
    when: "analyze runs"
    then: "src's children become the containers"
    # delegates: decispec-extract tests::analyze_ts_umbrella_project_promotes_src_children
  }
  scenario two_top_level_dirs_with_children_are_not_promoted {
    given: "two top-level directories that both hold code"
    when: "analyze decides the layout"
    then: "the directories themselves stay the containers"
    # delegates: decispec-extract tests::two_top_level_dirs_with_children_are_not_promoted
  }
  scenario single_dir_with_only_loose_files_stays_one_container {
    given: "a single directory containing only loose files"
    when: "analyze runs"
    then: "it stays one container"
    # delegates: decispec-extract tests::single_dir_with_only_loose_files_stays_one_container
  }
  scenario loose_root_py_files_land_in_a_project_named_container {
    given: "loose python files at the project root"
    when: "analyze runs"
    then: "they land in a container named after the project"
    # delegates: decispec-extract tests::loose_root_py_files_land_in_a_project_named_container
  }
  scenario python_import_forms {
    given: "python files using assorted import forms"
    when: "the import graph is built"
    then: "each form resolves to a relationship"
    # delegates: decispec-extract tests::python_import_forms
  }
  scenario python_relative_imports_resolve_to_own_container {
    given: "python files using relative imports"
    when: "the import graph is built"
    then: "they resolve to the importing file's own container, not a fake edge"
    # delegates: decispec-extract tests::python_relative_imports_resolve_to_own_container
  }
  scenario js_import_forms {
    given: "JS/TS files using assorted import forms"
    when: "the import graph is built"
    then: "each form resolves to a relationship"
    # delegates: decispec-extract tests::js_import_forms
  }
}

decision CMD-002 "Decisions are recovered from ADR markdown; requirements are never inferred" {
  status: accepted
  context: "F15: ADR docs under docs/adr/, docs/decisions/, adr/, decisions/ become decision blocks (id from filename number or slug, title/status/context/consequences, deprecated becomes superseded with a TODO successor). Recovery stops at structure by design — the acceptance test round-trips the draft through the real parser and validator."
  consequences: "+ a project that wrote ADRs gets its decision history for free. - requirements stay the human/agent's job, so a recovered draft is never gateable as-is; - ADR conventions outside the parsed shapes (custom headings, tables) are skipped or flattened."
}

spec CMD-002 {
  requirement CMD-002-R1 (EARS) {
    text: "WHEN ADR markdown docs exist under the documented directories THEN they SHALL become decision blocks carrying id, title, status, context, and consequences, and the rendered draft SHALL pass the real parser and validator without inferred requirements"
    layers: [unit]
    scenarios: [adr_madr_fixture_maps_fields, adr_status_from_heading_and_inline_line, adr_case_insensitive_dirs_and_render_order, adr_slug_ids_scrubbed_and_collision_suffixed, adr_long_context_flattened_and_id_collision_with_container, adr_missing_title_skipped_and_missing_status_warns, adr_excluded_dir_contributes_nothing, rendered_draft_parses_and_validates_with_the_real_parser, render_spec_shows_header_containers_and_rels, extract_writes_draft_with_decisions_from_adrs]
  }
  scenario adr_madr_fixture_maps_fields {
    given: "a MADR-format ADR file"
    when: "decisions are ingested"
    then: "title, status, context, and consequences map onto the decision block"
    # delegates: decispec-extract tests::adr_madr_fixture_maps_fields
  }
  scenario adr_status_from_heading_and_inline_line {
    given: "an ADR with its status in a heading or an inline line"
    when: "decisions are ingested"
    then: "the status is recovered either way"
    # delegates: decispec-extract tests::adr_status_from_heading_and_inline_line
  }
  scenario adr_case_insensitive_dirs_and_render_order {
    given: "ADR directories with case variations and files in mixed order"
    when: "decisions are ingested and rendered"
    then: "all documented directories are read and the render order is deterministic"
    # delegates: decispec-extract tests::adr_case_insensitive_dirs_and_render_order
  }
  scenario adr_slug_ids_scrubbed_and_collision_suffixed {
    given: "ADR filenames whose slugs fall outside the ID charset or collide"
    when: "decision ids are derived"
    then: "ids are scrubbed and collision-suffixed"
    # delegates: decispec-extract tests::adr_slug_ids_scrubbed_and_collision_suffixed
  }
  scenario adr_long_context_flattened_and_id_collision_with_container {
    given: "an ADR with multi-line context, or a decision id colliding with a container id"
    when: "decisions are ingested"
    then: "the context flattens to one line and the collision is resolved"
    # delegates: decispec-extract tests::adr_long_context_flattened_and_id_collision_with_container
  }
  scenario adr_missing_title_skipped_and_missing_status_warns {
    given: "an ADR with no title, or one with no status"
    when: "decisions are ingested"
    then: "the titleless ADR is skipped and the statusless one warns"
    # delegates: decispec-extract tests::adr_missing_title_skipped_and_missing_status_warns
  }
  scenario adr_excluded_dir_contributes_nothing {
    given: "an ADR inside an excluded directory"
    when: "decisions are ingested"
    then: "it contributes nothing"
    # delegates: decispec-extract tests::adr_excluded_dir_contributes_nothing
  }
  scenario rendered_draft_parses_and_validates_with_the_real_parser {
    given: "an analyzed project with ADRs"
    when: "the draft is rendered and fed to the real parser and validator"
    then: "it validates clean"
    # delegates: decispec-extract tests::rendered_draft_parses_and_validates_with_the_real_parser
  }
  scenario render_spec_shows_header_containers_and_rels {
    given: "an analysis result"
    when: "the draft is rendered"
    then: "the header, containers, and relationships all appear"
    # delegates: decispec-extract tests::render_spec_shows_header_containers_and_rels
  }
  scenario extract_writes_draft_with_decisions_from_adrs {
    given: "a project whose docs/adr directory holds ADR files"
    when: "cmd_extract writes the draft"
    then: "the written file contains the recovered decision blocks"
    # delegates: decispec (cli) tests::extract_writes_draft_with_decisions_from_adrs
  }
}

decision CMD-003 "Extract refuses unsafe or empty inputs with exit 2 rather than writing a partial draft" {
  status: accepted
  context: "README: extract 'refuses to overwrite' an existing draft (exit 2), exits 2 'when the tree has no source files or cannot be analyzed', and DecisionSpec's own scaffold (glue dir, tests/generated) is excluded from analysis so a re-run over a ported project does not ingest its own output."
  consequences: "+ a draft is never half-written or silently clobbered; the failure is loud and in the standard exit vocabulary. - the refusal gives no merge path: the human must delete or --stdout the old draft by hand."
}

spec CMD-003 {
  requirement CMD-003-R1 (EARS) {
    text: "WHEN the tree has no source files, the path is not a directory, the draft already exists, or the target is DecisionSpec's own scaffold THEN extract SHALL exit 2 or exclude the scaffold instead of writing a partial draft"
    layers: [unit]
    scenarios: [empty_tree_reports_no_source_files, non_directory_reports_not_a_directory, extract_writes_draft_and_refuses_to_overwrite, extract_with_no_source_files_is_cannot_complete, extract_with_missing_path_arg_is_cannot_complete, extract_without_project_root_is_cannot_complete, extract_ignores_decispec_scaffold_when_detecting_language, extract_ignores_glue_scaffold_in_python_projects]
  }
  scenario empty_tree_reports_no_source_files {
    given: "a directory with no source files"
    when: "analyze runs"
    then: "it reports no source files"
    # delegates: decispec-extract tests::empty_tree_reports_no_source_files
  }
  scenario non_directory_reports_not_a_directory {
    given: "an extract path that is a file, not a directory"
    when: "analyze runs"
    then: "it reports not a directory"
    # delegates: decispec-extract tests::non_directory_reports_not_a_directory
  }
  scenario extract_writes_draft_and_refuses_to_overwrite {
    given: "an analyzable project"
    when: "cmd_extract runs twice"
    then: "the first run writes the draft and the second refuses with exit 2"
    # delegates: decispec (cli) tests::extract_writes_draft_and_refuses_to_overwrite
  }
  scenario extract_with_no_source_files_is_cannot_complete {
    given: "a tree with no source files"
    when: "cmd_extract runs"
    then: "the exit code is 2"
    # delegates: decispec (cli) tests::extract_with_no_source_files_is_cannot_complete
  }
  scenario extract_with_missing_path_arg_is_cannot_complete {
    given: "an extract invocation without its path argument"
    when: "cmd_extract runs"
    then: "the exit code is 2"
    # delegates: decispec (cli) tests::extract_with_missing_path_arg_is_cannot_complete
  }
  scenario extract_without_project_root_is_cannot_complete {
    given: "a directory that is not inside a DecisionSpec project"
    when: "cmd_extract runs"
    then: "the exit code is 2"
    # delegates: decispec (cli) tests::extract_without_project_root_is_cannot_complete
  }
  scenario extract_ignores_decispec_scaffold_when_detecting_language {
    given: "a project that already contains DecisionSpec's own scaffold"
    when: "extract detects the language"
    then: "the scaffold does not skew the result"
    # delegates: decispec (cli) tests::extract_ignores_decispec_scaffold_when_detecting_language
  }
  scenario extract_ignores_glue_scaffold_in_python_projects {
    given: "a python project containing the glue scaffold"
    when: "analyze walks the tree"
    then: "the glue scaffold is excluded"
    # delegates: decispec (cli) tests::extract_ignores_glue_scaffold_in_python_projects
  }
}

decision CMD-004 "Container ids are scrubbed to the ASCII ID grammar and collisions are suffixed, never fatal" {
  status: accepted
  context: "F8: container ids are scrubbed to the ASCII ID grammar before collision-suffixing, keeping generated drafts inside the documented ID charset (F13 wants the lexer tightened to match). Directory noise (node_modules, .git, ...) is never descended into, and layout oddities warn instead of aborting."
  consequences: "+ a directory named anything produces a legal draft. - scrubbing can map two distinct names to one id, so the numeric suffixes are load-bearing; - a tree with more than 30 containers warns about grouping and still emits."
}

spec CMD-004 {
  requirement CMD-004-R1 (EARS) {
    text: "WHEN container titles or ADR ids fall outside the ID charset or collide THEN extract SHALL scrub them, dedupe with numeric suffixes, and warn rather than fail"
    layers: [unit]
    scenarios: [container_ids_scrub_chars_outside_the_id_charset, container_ids_sanitize_and_dedupe_with_numeric_suffixes, scrubbed_id_collisions_still_get_distinct_suffixes, more_than_30_containers_warn_about_grouping, noise_dirs_are_never_descended_into, title_escaping_handles_quotes_and_backslashes, language_tie_breaks_to_python, js_family_without_ts_files_is_javascript, analyze_with_exclusions_skips_scaffold_dirs]
  }
  scenario container_ids_scrub_chars_outside_the_id_charset {
    given: "directory names with characters outside [A-Za-z0-9_-]"
    when: "container ids are derived"
    then: "the offending characters are scrubbed"
    # delegates: decispec-extract tests::container_ids_scrub_chars_outside_the_id_charset
  }
  scenario container_ids_sanitize_and_dedupe_with_numeric_suffixes {
    given: "names that sanitize to the same base"
    when: "container ids are derived"
    then: "dedupe appends numeric suffixes"
    # delegates: decispec-extract tests::container_ids_sanitize_and_dedupe_with_numeric_suffixes
  }
  scenario scrubbed_id_collisions_still_get_distinct_suffixes {
    given: "scrubbed ids that collide after a prior dedupe"
    when: "container ids are derived"
    then: "they still receive distinct suffixes"
    # delegates: decispec-extract tests::scrubbed_id_collisions_still_get_distinct_suffixes
  }
  scenario more_than_30_containers_warn_about_grouping {
    given: "a tree with more than 30 containers"
    when: "analyze runs"
    then: "a grouping warning is emitted, not an error"
    # delegates: decispec-extract tests::more_than_30_containers_warn_about_grouping
  }
  scenario noise_dirs_are_never_descended_into {
    given: "directories like node_modules and .git"
    when: "analyze walks the tree"
    then: "they are never descended into"
    # delegates: decispec-extract tests::noise_dirs_are_never_descended_into
  }
  scenario title_escaping_handles_quotes_and_backslashes {
    given: "a container title containing quotes or backslashes"
    when: "the draft is rendered"
    then: "the title is escaped and the draft still parses"
    # delegates: decispec-extract tests::title_escaping_handles_quotes_and_backslashes
  }
  scenario language_tie_breaks_to_python {
    given: "a tree whose contents are ambiguous between languages"
    when: "the language is detected"
    then: "the tie breaks to python"
    # delegates: decispec-extract tests::language_tie_breaks_to_python
  }
  scenario js_family_without_ts_files_is_javascript {
    given: "a JS-family tree with no TypeScript files"
    when: "the language is detected"
    then: "it is classified as javascript"
    # delegates: decispec-extract tests::js_family_without_ts_files_is_javascript
  }
  scenario analyze_with_exclusions_skips_scaffold_dirs {
    given: "configured exclusion directories"
    when: "analyze walks the tree"
    then: "the excluded directories are skipped"
    # delegates: decispec-extract tests::analyze_with_exclusions_skips_scaffold_dirs
  }
}

decision CMD-005 "decispec.toml drives stack, fitness, and runner configuration with documented defaults" {
  status: accepted
  context: "README 'decispec.toml': lang, glue_module, fitness root_package and packages map container ids to importable modules (GEN-007), and [runners] overrides per-tool commands (F11). The config parser tests pin defaults for missing keys and inline-comment stripping."
  consequences: "+ one small file steers every toolchain invocation, with defaults that keep a minimal config working. - config is a second source of truth next to the spec files; nothing yet lints a stale or contradictory config (F11's landing note records the exact keys)."
}

spec CMD-005 {
  requirement CMD-005-R1 (EARS) {
    text: "WHEN the config file is parsed THEN missing keys SHALL keep their defaults, inline comments SHALL be stripped, fitness package mappings SHALL be read, and runner overrides SHALL reach both probe and run"
    layers: [unit]
    scenarios: [config_parser_keeps_defaults_for_missing_keys, config_parser_strips_inline_comments, config_reads_fitness_package_map, config_fitness_defaults_are_empty_and_unmapped, config_reads_runners_table, adapter_uses_runner_override_for_probe_and_run]
  }
  scenario config_parser_keeps_defaults_for_missing_keys {
    given: "a config file with most keys absent"
    when: "parse_config runs"
    then: "the documented defaults apply"
    # delegates: decispec (cli) tests::config_parser_keeps_defaults_for_missing_keys
  }
  scenario config_parser_strips_inline_comments {
    given: "config values carrying inline comments"
    when: "parse_config runs"
    then: "the comments are stripped, not parsed as value"
    # delegates: decispec (cli) tests::config_parser_strips_inline_comments
  }
  scenario config_reads_fitness_package_map {
    given: "a [stack.fitness.packages] table"
    when: "parse_config runs"
    then: "the container-to-module map is read"
    # delegates: decispec (cli) tests::config_reads_fitness_package_map
  }
  scenario config_fitness_defaults_are_empty_and_unmapped {
    given: "no fitness configuration"
    when: "parse_config runs"
    then: "fitness defaults to empty and unmapped"
    # delegates: decispec (cli) tests::config_fitness_defaults_are_empty_and_unmapped
  }
  scenario config_reads_runners_table {
    given: "a [runners] table"
    when: "parse_config runs"
    then: "the per-tool overrides are read"
    # delegates: decispec (cli) tests::config_reads_runners_table
  }
  scenario adapter_uses_runner_override_for_probe_and_run {
    given: "a configured runner override"
    when: "adapters probe and run"
    then: "the override command is used for both"
    # delegates: decispec (cli) tests::adapter_uses_runner_override_for_probe_and_run
  }
}

decision STORE-001 "Every gate run is recorded in sqlite and re-readable" {
  status: accepted
  context: "README data flow: gate reports flow to store, and `decispec query` reads the last gate status from .decispec/decispec.db. The store tests pin the schema-once behavior and the timestamp shape query displays."
  consequences: "+ gate history survives across runs for query and future drift analysis. - the db is a local artifact, not committed, so history is per-checkout."
}

spec STORE-001 {
  requirement STORE-001-R1 (EARS) {
    text: "WHEN a gate run finishes THEN it SHALL be persisted to .decispec/decispec.db, readable back, with the schema created once and timestamps formatted as UTC RFC3339"
    layers: [unit]
    scenarios: [record_and_read_back, opening_twice_reuses_schema, now_utc_formats_rfc3339ish]
  }
  scenario record_and_read_back {
    given: "a finished gate report"
    when: "the run is recorded and queried"
    then: "the report reads back intact"
    # delegates: decispec-store tests::record_and_read_back
  }
  scenario opening_twice_reuses_schema {
    given: "an existing decispec.db"
    when: "the store opens again"
    then: "the existing schema is reused, not recreated"
    # delegates: decispec-store tests::opening_twice_reuses_schema
  }
  scenario now_utc_formats_rfc3339ish {
    given: "a recorded run"
    when: "its timestamp is formatted"
    then: "the shape is UTC RFC3339"
    # delegates: decispec-store tests::now_utc_formats_rfc3339ish
  }
}

decision CMD-006 "Extract proposes a [stack.fitness] mapping for the analyzed layout" {
  status: accepted
  context: "F8 ships a suggested [stack.fitness.packages] mapping alongside the draft (python: sanitized dotted modules under a suggested root package; JS/TS: directory paths, with a warning that the python-only glue does not apply). F17's dogfood finding (tvgo) records why the mapping matters: without it, library imports drown in 'did not map' warnings."
  consequences: "+ a ported project starts with fitness contracts that can actually evaluate. - the suggestion is derived from directory names, so a renamed or nonstandard package layout needs manual correction before the contracts are trustworthy."
}

spec CMD-006 {
  requirement CMD-006-R1 (EARS) {
    text: "WHEN extract renders the analysis THEN it SHALL suggest a [stack.fitness] mapping using dotted module paths for python and directory paths for JS/TS, sanitizing every proposed module name"
    layers: [unit]
    scenarios: [fitness_toml_suggests_python_root_package_and_dotted_modules, fitness_toml_sanitizes_python_package_names, fitness_toml_for_js_uses_dir_paths_and_warns_about_glue]
  }
  scenario fitness_toml_suggests_python_root_package_and_dotted_modules {
    given: "an analyzed python project"
    when: "the fitness TOML is suggested"
    then: "it names a root package and dotted modules for the containers"
    # delegates: decispec-extract tests::fitness_toml_suggests_python_root_package_and_dotted_modules
  }
  scenario fitness_toml_sanitizes_python_package_names {
    given: "container names that are not importable module names"
    when: "the fitness TOML is suggested"
    then: "the proposed modules are sanitized"
    # delegates: decispec-extract tests::fitness_toml_sanitizes_python_package_names
  }
  scenario fitness_toml_for_js_uses_dir_paths_and_warns_about_glue {
    given: "an analyzed JS/TS project"
    when: "the fitness TOML is suggested"
    then: "it uses directory paths and warns that the glue module does not apply"
    # delegates: decispec-extract tests::fitness_toml_for_js_uses_dir_paths_and_warns_about_glue
  }
}


decision CMD-007 "decispec tui owns the terminal, the registry file, and project loading — registered projects stay read-only" {
  status: accepted
  context: "F31 (2026-10-20, AGENT_TEAMS.md §6/§12): cli owns crossterm setup/teardown, the registry file at $XDG_CONFIG_HOME/decispec/registry.toml, first-run seeding of well-known projects that exist on the machine, and --register resolving the project name from the target's decispec.toml. Broken workspaces load as error-state cards instead of aborting the cockpit, and tui exits 0 on clean quit, 2 on setup failure — never 1 (decided 2026-10-20, §12)."
  consequences: "+ registered projects are read-only by construction: the TUI never writes into them and never runs their toolchains, and one broken project cannot take down the fleet view. - every registry/loading behavior lives only in the cli seam (no tui-crate API), and exit codes 0/2 were verified by a real-pty smoke rather than an automated test."
}

spec CMD-007 {
  requirement CMD-007-R1 (EARS) {
    text: "WHEN the tui subcommand is invoked or a project is registered THEN `decispec tui` SHALL parse with register absent, `decispec tui --register <dir>` SHALL parse with register present, registering SHALL resolve the project name from the target's decispec.toml and persist the registry with re-registration updating in place, a directory that is not a DecisionSpec project SHALL be rejected, and a first run SHALL persist the registry file even when seeding finds nothing"
    layers: [unit]
    scenarios: [tui_subcommand_parses, tui_register_adds_project_and_persists_registry, tui_register_rejects_a_non_project_dir, tui_load_or_seed_writes_registry_on_first_run]
  }
  scenario tui_subcommand_parses {
    given: "the decispec tui invocation, with and without --register"
    when: "the cli parses the subcommand line"
    then: "register is None for the plain invocation and Some for --register"
    # delegates: decispec (cli) tests::tui_subcommand_parses
  }
  scenario tui_register_adds_project_and_persists_registry {
    given: "a DecisionSpec project and a registry path"
    when: "register_project runs twice against the same project"
    then: "the entry names the project and carries its canonical path, and the second registration updates in place with no duplicate"
    # delegates: decispec (cli) tests::tui_register_adds_project_and_persists_registry
  }
  scenario tui_register_rejects_a_non_project_dir {
    given: "a plain directory without decispec.toml"
    when: "register_project runs"
    then: "it returns an error instead of registering"
    # delegates: decispec (cli) tests::tui_register_rejects_a_non_project_dir
  }
  scenario tui_load_or_seed_writes_registry_on_first_run {
    given: "a registry path that does not exist yet"
    when: "load_or_seed_registry runs with no seeds and then with seeds"
    then: "the first run persists an empty registry file and the seeded run persists the seeds"
    # delegates: decispec (cli) tests::tui_load_or_seed_writes_registry_on_first_run
  }
  requirement CMD-007-R2 (EARS) {
    text: "WHEN the registry is seeded or a register path is expanded THEN only well-known project directories that exist on the machine SHALL be suggested, and a leading ~/ SHALL expand against $HOME while absolute and relative paths SHALL be left alone"
    layers: [unit]
    scenarios: [tui_registry_seeding_suggests_only_existing_dirs, tui_expand_home_leaves_absolute_and_plain_paths_alone]
  }
  scenario tui_registry_seeding_suggests_only_existing_dirs {
    given: "a fake home containing Projects/FPF/tvgo but no verydian-app"
    when: "default_seed_entries builds the first-run suggestions"
    then: "only tvgo is suggested"
    # delegates: decispec (cli) tests::tui_registry_seeding_suggests_only_existing_dirs
  }
  scenario tui_expand_home_leaves_absolute_and_plain_paths_alone {
    given: "absolute, relative, and ~/ -prefixed paths"
    when: "expand_home runs"
    then: "the first two pass through untouched, ~/ expands against $HOME when set, and a missing $HOME never panics"
    # delegates: decispec (cli) tests::tui_expand_home_leaves_absolute_and_plain_paths_alone
  }
  requirement CMD-007-R3 (EARS) {
    text: "WHEN a registered project is loaded or a terminal key event arrives THEN a project with broken specs SHALL load as an error state naming the failing reference, a project carrying .decispec/report.json SHALL surface the gate verdict, crossterm events SHALL translate to key presses only with release events ignored, and editor keys SHALL map to their Input counterparts with control characters lowercased"
    layers: [unit]
    scenarios: [tui_load_project_keeps_broken_specs_as_error_state, tui_load_project_reads_gate_report_when_present, tui_translate_event_maps_presses_only, translate_event_maps_editor_keys_and_ctrl]
  }
  scenario tui_load_project_keeps_broken_specs_as_error_state {
    given: "a project whose spec references a scenario that does not exist"
    when: "load_tui_project runs"
    then: "the project loads with no workspace and a load error naming the missing scenario"
    # delegates: decispec (cli) tests::tui_load_project_keeps_broken_specs_as_error_state
  }
  scenario tui_load_project_reads_gate_report_when_present {
    given: "a valid project whose .decispec/report.json records a pass"
    when: "load_tui_project runs"
    then: "the workspace loads and the gate snapshot carries the pass verdict"
    # delegates: decispec (cli) tests::tui_load_project_reads_gate_report_when_present
  }
  scenario tui_translate_event_maps_presses_only {
    given: "crossterm key events of kind press and release"
    when: "translate_event runs"
    then: "presses map to the Input vocabulary and releases map to nothing"
    # delegates: decispec (cli) tests::tui_translate_event_maps_presses_only
  }
  scenario translate_event_maps_editor_keys_and_ctrl {
    given: "crossterm events for backspace, delete, home, end, page up, page down, and control characters"
    when: "translate_event runs"
    then: "each maps to its editor Input counterpart, with control characters lowercased"
    # delegates: decispec (cli) tests::translate_event_maps_editor_keys_and_ctrl
  }
}

decision CMD-008 "In-TUI authoring writes reach projects only through the explicit save and the draft-PR flow" {
  status: accepted
  context: "F32 (2026-10-20, AGENT_TEAMS.md §6 guardrails): Ctrl+S writes the edited file into the project's specs/ and then re-checks the whole workspace, reporting consistency in the status bar; `p` opens a draft PR per the repo's own rules (tvgo: base main, human merge, agent never merges). The cli shells out to git and gh: save_spec_file writes then validates, pr_readiness gates on a git repo with uncommitted changes and a gh binary, the branch is spec/tui-<UTC timestamp>, and create_spec_pr commits exactly the spec file and pushes before gh is invoked."
  consequences: "+ registered projects stay read-only except these two explicit, human-initiated actions, and every precheck failure lands in the status bar with a reason instead of a half-made PR. - the flow depends on git and gh binaries existing and only fails at the gh step against non-GitHub remotes, so the happy path is pinned against a local bare origin fixture rather than real GitHub; a save whose edit fails check still writes the file, by design — the error report is the guard, not a refusal."
}

spec CMD-008 {
  requirement CMD-008-R1 (EARS) {
    text: "WHEN a spec file is saved from the editor THEN the file SHALL be written into the project and a valid edit SHALL report the save with the workspace summary, while an edit that fails check SHALL still write but return a loud warning naming the check failure and its offending reference"
    layers: [unit]
    scenarios: [save_spec_file_writes_and_reports_workspace_health]
  }
  scenario save_spec_file_writes_and_reports_workspace_health {
    given: "a project with a valid spec, edited first validly then into a broken reference"
    when: "save_spec_file runs for each edit"
    then: "the valid edit writes and reports the save with the decision count, and the broken edit still writes and warns that the workspace fails check naming the reference"
    # delegates: decispec (cli) tests::save_spec_file_writes_and_reports_workspace_health
  }
  requirement CMD-008-R2 (EARS) {
    text: "WHEN a draft PR is requested THEN a directory that is not a git repository SHALL be refused, a clean tree SHALL be refused as having nothing to PR, a dirty tree without gh SHALL be refused naming gh, a dirty tree with gh SHALL pass readiness, and the branch name SHALL be spec/tui- followed by the UTC timestamp"
    layers: [unit]
    scenarios: [pr_readiness_flags_non_git_clean_tree_and_missing_gh, pr_branch_name_is_utc_timestamped]
  }
  scenario pr_readiness_flags_non_git_clean_tree_and_missing_gh {
    given: "a non-git directory, then a git fixture with a clean tree, then a dirty tree with and without gh"
    when: "pr_readiness runs in each state"
    then: "each refusal names its reason and only the dirty tree with gh passes"
    # delegates: decispec (cli) tests::pr_readiness_flags_non_git_clean_tree_and_missing_gh
  }
  scenario pr_branch_name_is_utc_timestamped {
    given: "two known instants in time"
    when: "pr_branch_name formats each"
    then: "the name is spec/tui- followed by the UTC date and time"
    # delegates: decispec (cli) tests::pr_branch_name_is_utc_timestamped
  }
  requirement CMD-008-R3 (EARS) {
    text: "WHEN the draft PR is created THEN a spec/tui- branch SHALL be created carrying a commit of exactly the spec file, that commit SHALL reach the origin, and the result SHALL nonetheless be an error naming gh when gh is missing or cannot work against the remote"
    layers: [unit]
    scenarios: [create_spec_pr_commits_pushes_then_reports_gh_failure]
  }
  scenario create_spec_pr_commits_pushes_then_reports_gh_failure {
    given: "a git repo with a local bare origin, an edited spec, and a non-spec file"
    when: "create_spec_pr runs with gh injected past readiness"
    then: "the error names gh, the spec/tui- branch's single-spec commit is pushed to the origin, the non-spec file stays uncommitted, and no other file rides along"
    # delegates: decispec (cli) tests::create_spec_pr_commits_pushes_then_reports_gh_failure
  }
  requirement CMD-008-R4 (EARS) {
    text: "WHEN the only uncommitted changes are outside specs/ THEN the draft PR SHALL be refused naming specs, the worktree SHALL be restored to its previous branch with no spec/tui- branch left behind, and the uncommitted changes SHALL survive uncommitted"
    layers: [unit]
    scenarios: [create_spec_pr_refuses_changes_outside_specs_and_restores]
  }
  scenario create_spec_pr_refuses_changes_outside_specs_and_restores {
    given: "a git repo whose only uncommitted change is a non-spec file"
    when: "create_spec_pr runs"
    then: "the error names specs, the previous branch is checked out with no spec/tui- branch remaining, and the change is still uncommitted"
    # delegates: decispec (cli) tests::create_spec_pr_refuses_changes_outside_specs_and_restores
  }
  requirement CMD-008-R5 (EARS) {
    text: "WHEN a spec file is saved THEN the write SHALL be confined to the project's specs/ tree — absolute paths, .. traversal, and non-specs first components SHALL be refused naming the refusal — and missing parent directories under specs/ SHALL be created so nested new files can be saved"
    layers: [unit]
    scenarios: [save_spec_file_creates_new_file_and_parent_dirs, save_spec_file_refuses_paths_outside_specs]
  }
  scenario save_spec_file_creates_new_file_and_parent_dirs {
    given: "a save targeting specs/payments/010-new.spec where the payments directory does not exist"
    when: "save_spec_file runs"
    then: "the parent directories are created and the new file is written with the given text"
    # delegates: decispec (cli) tests::save_spec_file_creates_new_file_and_parent_dirs
  }
  scenario save_spec_file_refuses_paths_outside_specs {
    given: "saves targeting ../evil.spec, specs/../evil.spec, and an absolute path"
    when: "save_spec_file runs for each"
    then: "each is refused with an error naming the refusal and no file is written outside specs/"
    # delegates: decispec (cli) tests::save_spec_file_refuses_paths_outside_specs
  }
}

decision CMD-009 "Query expands from exact IDs to glob listings without changing the exact path" {
  status: accepted
  context: "The agent view (query) could only resolve one exact id at a time, so listing every CMD decision meant a shell loop; README's agentic loop showed query AUTH-001 and nothing more. Agents asked (2026-10-20, console-queue F40) for `'CMD-*'` patterns and a --status narrowing. The exact-ID resolution (decision, then requirement/scenario/invariant/policy id mapped to its parent decision, full subtree print) is characterization-tested and must not move — the pattern is an additive fast path selected by the presence of `*`."
  consequences: "+ one query call lists a slice of the decision log compactly and --status answers 'what is still only proposed' without a pager. - two output shapes on one command (compact lines for patterns, full subtree for exact ids) means callers must know which they invoked, and a pattern with no match exits 1 exactly like an unknown id, so an over-specific glob is indistinguishable from a typo in the exit code."
}

spec CMD-009 {
  requirement CMD-009-R1 (EARS) {
    text: "WHEN query is given an id containing '*' THEN it SHALL list every matching decision one line each (id, status, title) sorted by id, WHILE an id without '*' SHALL keep the exact resolution and full-subtree output, and no match SHALL exit findings"
    layers: [unit]
    scenarios: [query_glob_match_follows_wildcard_semantics, query_pattern_selects_decisions_and_status_filters, query_pattern_with_no_match_exits_1]
  }
  requirement CMD-009-R2 (EARS) {
    text: "WHEN --status is given THEN only decisions of that status SHALL be listed or printed, and an exact id whose status differs SHALL be treated as no match"
    layers: [unit]
    scenarios: [query_exact_id_and_star_pattern_both_exit_0]
  }
  scenario query_glob_match_follows_wildcard_semantics {
    given: "patterns with stars in prefix, suffix and middle positions, a bare star, and a starless pattern"
    when: "each is matched against decision ids"
    then: "starless means exact equality and each star matches any run of characters anchored at both ends"
    # delegates: decispec tests::query_glob_match_follows_wildcard_semantics
  }
  scenario query_pattern_selects_decisions_and_status_filters {
    given: "a workspace with accepted CMD, proposed CMD and accepted TUI decisions"
    when: "a CMD pattern is applied alone and together with --status"
    then: "only the matching decisions are returned, sorted by id, and no match yields an empty list"
    # delegates: decispec tests::query_pattern_selects_decisions_and_status_filters
  }
  scenario query_pattern_with_no_match_exits_1 {
    given: "a project and a pattern that matches nothing"
    when: "query runs"
    then: "it prints a no-match message and exits findings, the same code as an unknown id"
    # delegates: decispec tests::query_pattern_with_no_match_exits_1
  }
  scenario query_exact_id_and_star_pattern_both_exit_0 {
    given: "a project with CMD-001 accepted and CMD-002 proposed"
    when: "query runs with the exact id, with a star pattern, and with an exact id plus a mismatching --status"
    then: "the exact id and the pattern both exit 0 while the mismatching status exits findings"
    # delegates: decispec tests::query_exact_id_and_star_pattern_both_exit_0
  }
}

decision CMD-010 "Partial generation writes only matching specs' artifacts and never sweeps" {
  status: accepted
  context: "F41 (2026-10-20 console queue): `gen` always rendered and swept the whole workspace, so touching one decision's scenarios rewrote every generated file and a filtered regeneration would have fed the stale sweep a partial render set — sweeping it would delete every artifact outside the filter. The prefix filter therefore owns two deliberate exclusions: workspace-scoped outputs (model/flow diagrams, shared conftest, fitness configs) are not decision-owned and are skipped, and the sweep is skipped entirely until a plain run reconciles."
  consequences: "+ one command regenerates a slice without churning the rest of the tree, and an unmatched prefix is caught before anything is written. - a filtered run can leave stale artifacts from decisions that shrank since the last full gen, and --only never refreshes diagrams — plain 'decispec gen' remains the only reconciling command."
}

spec CMD-010 {
  requirement CMD-010-R1 (EARS) {
    text: "WHEN gen runs with --only prefixes THEN it SHALL write only the artifacts of specs whose id starts with a prefix, SHALL skip workspace-scoped outputs and the stale sweep, and SHALL leave every other generated file untouched"
    layers: [unit]
    scenarios: [gen_only_writes_matching_artifacts_and_skips_the_sweep, render_all_for_selects_only_matching_spec_artifacts]
  }
  requirement CMD-010-R2 (EARS) {
    text: "WHEN no spec id matches any --only prefix THEN gen SHALL write nothing and exit findings, and WHEN --only is absent THEN rendering and sweeping SHALL be byte-identical to the unfiltered behavior"
    layers: [unit]
    scenarios: [gen_only_with_unmatched_prefix_exits_1_and_writes_nothing, render_all_for_with_no_prefixes_equals_render_all]
  }
  scenario gen_only_writes_matching_artifacts_and_skips_the_sweep {
    given: "a project with AUTH and TUI specs and a stale generated diagram"
    when: "gen runs with --only AUTH"
    then: "only AUTH artifacts are written, workspace outputs are absent, and the stale diagram survives"
    # delegates: decispec tests::gen_only_writes_matching_artifacts_and_skips_the_sweep
  }
  scenario render_all_for_selects_only_matching_spec_artifacts {
    given: "a rendered workspace with two specs and diagrams, conftest and fitness outputs"
    when: "render_all_for filters on one spec's prefix"
    then: "only that spec's artifacts are returned and no workspace-scoped output appears"
    # delegates: decispec-codegen tests::render_all_for_selects_only_matching_spec_artifacts
  }
  scenario gen_only_with_unmatched_prefix_exits_1_and_writes_nothing {
    given: "a project and a prefix no spec matches"
    when: "gen runs with --only"
    then: "it exits findings and creates no generated tree"
    # delegates: decispec tests::gen_only_with_unmatched_prefix_exits_1_and_writes_nothing
  }
  scenario render_all_for_with_no_prefixes_equals_render_all {
    given: "a workspace"
    when: "render_all_for is called with an empty prefix list"
    then: "its output equals render_all exactly"
    # delegates: decispec-codegen tests::render_all_for_with_no_prefixes_equals_render_all
  }
}
