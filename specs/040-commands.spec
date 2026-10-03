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

