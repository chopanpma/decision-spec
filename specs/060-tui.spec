# TUI spec cockpit — prefix TUI.
# Every scenario delegates to the decispec-tui Rust test that pins it (F33).
# The `decispec tui` cli command behavior lives in specs/040-commands.spec (CMD-007).

decision TUI-001 "The project registry is a hand-parsed TOML subset with line-numbered errors and one entry per project" {
  status: accepted
  context: "AGENT_TEAMS.md §6 (2026-10-20): the registry at ~/.config/decispec/registry.toml is cli-owned, and crates/tui keeps the workspace's deliberately minimal dependency set — no `toml` crate — so registry.rs hand-parses exactly the documented subset: comments, blank lines, [[project]] blocks with name/path. F31 seeded it with tvgo + verydian on first run."
  consequences: "+ zero new dependencies and a parse that cannot be surprised by TOML features the writer never emits. - the subset has no escapes and no multi-line strings, so malformed input gets a registry.toml:<line> error instead of a full TOML diagnosis; upsert matches on name OR path, so renaming both at once reads as a new project."
}

spec TUI-001 {
  requirement TUI-001-R1 (EARS) {
    text: "WHEN the registry text is parsed THEN comments and blank lines SHALL be ignored, a `#` inside quotes SHALL be kept, and a garbage line, a key outside a [[project]] block, an unknown key, or an entry missing name or path SHALL be rejected with a registry.toml line-numbered error, while empty source SHALL parse as an empty registry"
    layers: [unit]
    scenarios: [parse_ignores_comments_and_blank_lines, parse_keeps_hash_inside_quotes, parse_errors_on_garbage_line, parse_errors_on_key_outside_project_block, parse_errors_on_unknown_key_with_line_number, parse_errors_on_entry_missing_path, parse_empty_source_is_empty_registry]
  }
  scenario parse_ignores_comments_and_blank_lines {
    given: "registry text with a header comment, a blank line, and an inline comment"
    when: "Registry::parse runs"
    then: "the comments and blank lines contribute nothing and the project entry parses"
    # delegates: decispec-tui registry::tests::parse_ignores_comments_and_blank_lines
  }
  scenario parse_keeps_hash_inside_quotes {
    given: "a name and path containing `#` inside their quotes"
    when: "Registry::parse runs"
    then: "the quoted hashes survive — only unquoted `#` starts a comment"
    # delegates: decispec-tui registry::tests::parse_keeps_hash_inside_quotes
  }
  scenario parse_errors_on_garbage_line {
    given: "a line that is neither [[project]] nor key = \"value\""
    when: "Registry::parse runs"
    then: "it errors carrying registry.toml and the line number"
    # delegates: decispec-tui registry::tests::parse_errors_on_garbage_line
  }
  scenario parse_errors_on_key_outside_project_block {
    given: "a key/value line before any [[project]] block"
    when: "Registry::parse runs"
    then: "it errors that the key is outside a project block, at that line"
    # delegates: decispec-tui registry::tests::parse_errors_on_key_outside_project_block
  }
  scenario parse_errors_on_unknown_key_with_line_number {
    given: "a project block containing a key that is not name or path"
    when: "Registry::parse runs"
    then: "it errors naming the unknown key and its line number"
    # delegates: decispec-tui registry::tests::parse_errors_on_unknown_key_with_line_number
  }
  scenario parse_errors_on_entry_missing_path {
    given: "a [[project]] block with a name but no path"
    when: "Registry::parse runs"
    then: "it errors that the entry is missing name or path"
    # delegates: decispec-tui registry::tests::parse_errors_on_entry_missing_path
  }
  scenario parse_empty_source_is_empty_registry {
    given: "empty registry text"
    when: "Registry::parse runs"
    then: "the result is an empty registry, not an error"
    # delegates: decispec-tui registry::tests::parse_empty_source_is_empty_registry
  }
  requirement TUI-001-R2 (EARS) {
    text: "WHEN the registry is serialized or a project is upserted THEN to_toml SHALL round-trip through parse, and upsert SHALL replace the entry with the same name or the same path so one project never appears twice"
    layers: [unit]
    scenarios: [to_toml_and_parse_round_trip, upsert_replaces_same_name_and_same_path]
  }
  scenario to_toml_and_parse_round_trip {
    given: "a registry with two projects"
    when: "it is serialized with to_toml and parsed back"
    then: "the parsed registry equals the original"
    # delegates: decispec-tui registry::tests::to_toml_and_parse_round_trip
  }
  scenario upsert_replaces_same_name_and_same_path {
    given: "a registry containing tvgo and another project"
    when: "an entry upserts with tvgo's name under a new path, then with the other project's path under a new name"
    then: "both replace in place and the registry still holds exactly two entries"
    # delegates: decispec-tui registry::tests::upsert_replaces_same_name_and_same_path
  }
}

decision TUI-002 "The gate dashboard reads .decispec/report.json through a Deserialize mirror, decoupled from gate's Serialize-only types" {
  status: accepted
  context: "F31 constraint: gate's report types stay Serialize-only, so crates/tui cannot depend on them — snapshot.rs mirrors the report shape as a Deserialize read-model, reusing ir's Layer/Status (both Deserialize). serde defaults keep a slightly older or newer report.json readable, and parses_a_full_report pins the mirror against the real writer's shape."
  consequences: "+ the dashboard renders without any gate API change and tolerates report drift in both directions. - the mirror is a second copy of the report shape to keep aligned by hand; the F31 review caught exactly this class of gap when the kebab-case pin initially deserialized identically under snake_case."
}

spec TUI-002 {
  requirement TUI-002-R1 (EARS) {
    text: "WHEN GateSnapshot::parse reads report.json content THEN a full report SHALL parse into project, verdict, counts, warnings, and decision rows with layers and results, kebab-case row results SHALL deserialize, missing optional fields SHALL default, unknown fields SHALL be ignored, and non-JSON content SHALL be an error"
    layers: [unit]
    scenarios: [parses_a_full_report, parses_kebab_case_row_results, tolerates_missing_optional_fields, ignores_unknown_fields, garbage_is_an_error]
  }
  scenario parses_a_full_report {
    given: "report.json content shaped like decispec gate's real writer emits"
    when: "GateSnapshot::parse runs"
    then: "project, verdict, counts, warnings, decision outcomes, row layers, and row results all land in the snapshot"
    # delegates: decispec-tui snapshot::tests::parses_a_full_report
  }
  scenario parses_kebab_case_row_results {
    given: "a report whose row results include the kebab-case no-artifacts"
    when: "GateSnapshot::parse runs and rows() is collected"
    then: "every row result deserializes — a snake_case regression turns the parse into an error, pinned here"
    # delegates: decispec-tui snapshot::tests::parses_kebab_case_row_results
  }
  scenario tolerates_missing_optional_fields {
    given: "a report with failures, warnings, and decision rows absent"
    when: "GateSnapshot::parse runs"
    then: "the optional fields default and the snapshot still parses"
    # delegates: decispec-tui snapshot::tests::tolerates_missing_optional_fields
  }
  scenario ignores_unknown_fields {
    given: "a report carrying a future nested field the mirror does not know"
    when: "GateSnapshot::parse runs"
    then: "the unknown field is ignored and the parse succeeds"
    # delegates: decispec-tui snapshot::tests::ignores_unknown_fields
  }
  scenario garbage_is_an_error {
    given: "content that is not JSON"
    when: "GateSnapshot::parse runs"
    then: "the result is an Err naming report.json"
    # delegates: decispec-tui snapshot::tests::garbage_is_an_error
  }
}

decision TUI-003 "App state is a pure update over a terminal-independent input vocabulary" {
  status: accepted
  context: "AGENT_TEAMS.md §6 (2026-10-20): crates/tui holds pure UI logic — update(state, event) -> state — and cli owns the crossterm event pump, so the whole navigation matrix is TDD-able without a terminal. Keys per README: ←→ switch project, ⇥ cycles view, ↑↓ select, ⏎/esc scroll, r reload, q quit. Artifacts are rendered in memory from the live workspace via codegen's pure render functions."
  consequences: "+ every transition is a unit test on plain data, and the same state machine drives the real terminal. - each real key is translated twice by hand (crossterm event → Input → Action) and the two layers must stay in sync; in-memory artifact rendering re-runs codegen on every selection change, which is fine at cockpit scale but not free."
}

spec TUI-003 {
  requirement TUI-003-R1 (EARS) {
    text: "WHEN the app starts or receives navigation input THEN it SHALL start on the first project in specs mode with a status line (an empty registry says so), Left and Right SHALL switch projects with clamping at both ends and reset selection and scroll, Tab SHALL cycle specs to artifacts to dashboard, and Up and Down SHALL clamp the selection to the decision count"
    layers: [unit]
    scenarios: [new_app_starts_on_first_project_in_specs_mode, empty_registry_says_so_in_status, right_and_left_switch_projects_and_clamp, switching_project_resets_selection_and_scroll, tab_cycles_modes, down_clamps_to_decision_count]
  }
  scenario new_app_starts_on_first_project_in_specs_mode {
    given: "two projects, one loaded and one broken"
    when: "App::new builds the initial state"
    then: "the app starts on project 0 in specs mode and the status names the loaded count"
    # delegates: decispec-tui app::tests::new_app_starts_on_first_project_in_specs_mode
  }
  scenario empty_registry_says_so_in_status {
    given: "no registered projects"
    when: "App::new builds the initial state"
    then: "the status line tells the user to run decispec tui --register"
    # delegates: decispec-tui app::tests::empty_registry_says_so_in_status
  }
  scenario right_and_left_switch_projects_and_clamp {
    given: "two registered projects"
    when: "Right and Left are pressed past both ends"
    then: "the selection moves between projects and clamps at the first and last"
    # delegates: decispec-tui app::tests::right_and_left_switch_projects_and_clamp
  }
  scenario switching_project_resets_selection_and_scroll {
    given: "an app with a non-zero decision selection and scroll offset"
    when: "the project is switched"
    then: "the decision selection and scroll reset to zero"
    # delegates: decispec-tui app::tests::switching_project_resets_selection_and_scroll
  }
  scenario tab_cycles_modes {
    given: "an app in specs mode"
    when: "Tab is pressed three times"
    then: "the mode cycles specs, artifacts, dashboard, specs"
    # delegates: decispec-tui app::tests::tab_cycles_modes
  }
  scenario down_clamps_to_decision_count {
    given: "a project whose workspace has two decisions"
    when: "Down is pressed past the last decision and Up afterwards"
    then: "the selection clamps at the last decision and returns to the first"
    # delegates: decispec-tui app::tests::down_clamps_to_decision_count
  }
  requirement TUI-003-R2 (EARS) {
    text: "WHEN a key arrives THEN q SHALL return Action::Quit, r SHALL return Action::Reload, other characters together with Enter and Esc SHALL return Action::None, Enter SHALL page the scroll offset down, and Esc SHALL reset the scroll offset to zero"
    layers: [unit]
    scenarios: [q_quits_r_reloads_other_chars_do_nothing, enter_pages_scroll_and_esc_resets]
  }
  scenario q_quits_r_reloads_other_chars_do_nothing {
    given: "a running app"
    when: "q, r, then other keys arrive"
    then: "q returns Quit, r returns Reload, and the rest return None without touching scroll"
    # delegates: decispec-tui app::tests::q_quits_r_reloads_other_chars_do_nothing
  }
  scenario enter_pages_scroll_and_esc_resets {
    given: "an app in artifacts mode"
    when: "Enter pages the scroll down and Esc follows"
    then: "the scroll offset increases then returns to zero"
    # delegates: decispec-tui app::tests::enter_pages_scroll_and_esc_resets
  }
  requirement TUI-003-R3 (EARS) {
    text: "WHEN the artifacts view computes files or the dashboard computes counts THEN render_all plus the on-demand decision index SHALL be rendered in memory with .decispec/docs/decisions.md as the last entry and SHALL be empty for a project that failed to load, decision status counts SHALL follow accepted, proposed, rejected, superseded order, and layer counts SHALL follow Layer::ALL order"
    layers: [unit]
    scenarios: [artifact_files_render_in_memory_with_index_last, artifact_files_empty_for_broken_project, status_counts_by_decision_level, layer_counts_requirements_per_layer]
  }
  requirement TUI-003-R4 (EARS) {
    text: "WHEN the editor opens, the buffer changes, or a save or PR is requested THEN e SHALL open a file picker over the project's spec sources, Enter SHALL load the file text clean with live diagnostics, typing SHALL mark the buffer dirty and recompute diagnostics without quitting on q, Ctrl+S SHALL return Action::SaveSpec only for a dirty buffer and mark_saved SHALL clear the dirty flag and any discard confirmation, Esc SHALL close a clean buffer immediately and arm then confirm a discard for a dirty one, an edit made while the confirmation is armed SHALL disarm it so the next Esc does not discard, and p SHALL return Action::CreatePr only for a loaded project"
    layers: [unit]
    scenarios: [e_opens_the_file_picker, picker_lists_only_spec_sources, enter_loads_file_text_clean, picker_shows_live_diagnostics_for_broken_files, typing_marks_dirty_recomputes_diagnostics_and_cannot_quit, ctrl_s_saves_only_a_dirty_buffer, mark_saved_clears_dirty, esc_confirms_before_discarding_dirty_buffer, editing_after_confirm_disarms_it, p_requests_a_pr_for_a_loaded_project]
  }
  scenario artifact_files_render_in_memory_with_index_last {
    given: "a loaded project with a parsed workspace"
    when: "artifact_files renders the generated files in memory"
    then: "the diagrams, generated tests, and real codegen content appear, with .decispec/docs/decisions.md riding along as the final entry"
    # delegates: decispec-tui app::tests::artifact_files_render_in_memory_with_index_last
  }
  scenario artifact_files_empty_for_broken_project {
    given: "the currently selected project failed to load"
    when: "artifact_files is asked for its files"
    then: "the result is empty rather than a panic"
    # delegates: decispec-tui app::tests::artifact_files_empty_for_broken_project
  }
  scenario status_counts_by_decision_level {
    given: "a workspace with one accepted and one superseded decision"
    when: "decision_status_counts runs"
    then: "the counts are [1, 0, 0, 1] in accepted, proposed, rejected, superseded order"
    # delegates: decispec-tui app::tests::status_counts_by_decision_level
  }
  scenario layer_counts_requirements_per_layer {
    given: "a requirement carrying unit and contract layers"
    when: "layer_counts runs"
    then: "each Layer::ALL layer reports its requirement count, zero for the rest"
    # delegates: decispec-tui app::tests::layer_counts_requirements_per_layer
  }
  scenario e_opens_the_file_picker {
    given: "a loaded project"
    when: "e is pressed"
    then: "the editor opens in picking mode on the first spec source"
    # delegates: decispec-tui app::tests::e_opens_the_file_picker
  }
  scenario picker_lists_only_spec_sources {
    given: "a project with one spec source"
    when: "Down and Esc are pressed in the picker"
    then: "the selection clamps at the only entry and Esc closes the picker"
    # delegates: decispec-tui app::tests::picker_lists_only_spec_sources
  }
  scenario enter_loads_file_text_clean {
    given: "a picker over a loaded project"
    when: "Enter picks the spec file"
    then: "the buffer holds the file text, starts clean, and carries no diagnostics"
    # delegates: decispec-tui app::tests::enter_loads_file_text_clean
  }
  scenario picker_shows_live_diagnostics_for_broken_files {
    given: "a project whose spec references a scenario that does not exist"
    when: "the editor loads the file"
    then: "the live diagnostics name the missing reference"
    # delegates: decispec-tui app::tests::picker_shows_live_diagnostics_for_broken_files
  }
  scenario typing_marks_dirty_recomputes_diagnostics_and_cannot_quit {
    given: "an open editor over a valid spec"
    when: "a character is typed"
    then: "the buffer turns dirty, diagnostics recompute, and q inserts instead of quitting"
    # delegates: decispec-tui app::tests::typing_marks_dirty_recomputes_diagnostics_and_cannot_quit
  }
  scenario ctrl_s_saves_only_a_dirty_buffer {
    given: "an open editor, clean then dirty"
    when: "Ctrl+S is pressed in both states"
    then: "the clean press returns None and the dirty press returns Action::SaveSpec naming the file and text"
    # delegates: decispec-tui app::tests::ctrl_s_saves_only_a_dirty_buffer
  }
  scenario mark_saved_clears_dirty {
    given: "a dirty editor buffer with an armed discard confirmation"
    when: "mark_saved runs after a save"
    then: "the dirty flag and the confirmation clear"
    # delegates: decispec-tui app::tests::mark_saved_clears_dirty
  }
  scenario esc_confirms_before_discarding_dirty_buffer {
    given: "an open editor, first clean then dirty"
    when: "Esc is pressed once on each"
    then: "the clean buffer closes immediately and the dirty buffer needs a second Esc after the confirmation arms"
    # delegates: decispec-tui app::tests::esc_confirms_before_discarding_dirty_buffer
  }
  scenario editing_after_confirm_disarms_it {
    given: "a dirty buffer with the discard confirmation armed"
    when: "another character is typed before the second Esc"
    then: "the confirmation disarms and one Esc no longer discards"
    # delegates: decispec-tui app::tests::editing_after_confirm_disarms_it
  }
  scenario p_requests_a_pr_for_a_loaded_project {
    given: "a loaded project and then a broken one selected"
    when: "p is pressed on each"
    then: "the loaded project returns Action::CreatePr and the broken one returns None with a status message"
    # delegates: decispec-tui app::tests::p_requests_a_pr_for_a_loaded_project
  }
}

decision TUI-004 "Views render from &App into a ratatui buffer with no I/O, so a broken project shows its error instead of crashing" {
  status: accepted
  context: "AGENT_TEAMS.md §6 (2026-10-20): widget trees are functions from &App into a ratatui Buffer and cli owns the terminal — the Template-Method split that keeps crates/tui pure per AGENTS.md §3. Every view is tested headless through ratatui's TestBackend with buffer assertions, including the broken-project error pane (a registered project whose specs fail to parse loads as an error-state card)."
  consequences: "+ the whole cockpit renders in tests with zero terminal plumbing, and a broken project degrades to a visible error card. - buffer-text assertions pin rendered substrings, not layout geometry, so a reskin that moves the same words between panes can pass or fail for incidental reasons."
}

spec TUI-004 {
  requirement TUI-004-R1 (EARS) {
    text: "WHEN render draws a frame THEN specs mode SHALL show the project names, decision ids, titles, statuses, requirement layers, and the key hints, a broken project SHALL show its load error instead of panicking, artifacts mode SHALL list the generated file paths with the selected file's content, and the dashboard SHALL show the decision levels, the requirement layers, and the last gate verdict with its warnings"
    layers: [unit]
    scenarios: [specs_mode_shows_projects_decisions_and_detail, broken_project_shows_its_error_not_a_panic, artifacts_mode_lists_generated_files_and_content, dashboard_shows_levels_layers_and_gate_verdict]
  }
  requirement TUI-004-R2 (EARS) {
    text: "WHEN the editor or its file picker renders THEN the picker SHALL ask which file and list only the project's spec sources, and an open buffer SHALL show the modified marker when dirty, the live diagnostics with their file:line, and the esc close hint with the discard confirmation armed"
    layers: [unit]
    scenarios: [editor_picker_lists_spec_files, editor_shows_text_diagnostics_and_discard_warning]
  }
  scenario specs_mode_shows_projects_decisions_and_detail {
    given: "a loaded project and a broken one"
    when: "specs mode renders into a TestBackend buffer"
    then: "the frame shows both project names, the decision id, title, status, layers, and the q quit hint"
    # delegates: decispec-tui views::tests::specs_mode_shows_projects_decisions_and_detail
  }
  scenario broken_project_shows_its_error_not_a_panic {
    given: "a project whose load error names an unknown scenario reference"
    when: "specs mode renders into a TestBackend buffer"
    then: "the frame shows that error text and does not panic"
    # delegates: decispec-tui views::tests::broken_project_shows_its_error_not_a_panic
  }
  scenario artifacts_mode_lists_generated_files_and_content {
    given: "a loaded project switched to artifacts mode"
    when: "the view renders into a TestBackend buffer"
    then: "the frame lists the generated file paths and shows the selected artifact's rendered content"
    # delegates: decispec-tui views::tests::artifacts_mode_lists_generated_files_and_content
  }
  scenario dashboard_shows_levels_layers_and_gate_verdict {
    given: "a loaded project with a passing gate report carrying a warning"
    when: "the dashboard renders into a TestBackend buffer"
    then: "the frame shows the PASS verdict with its row counts, the decision-levels band, the requirement-layers band, and the warning"
    # delegates: decispec-tui views::tests::dashboard_shows_levels_layers_and_gate_verdict
  }
  scenario editor_picker_lists_spec_files {
    given: "a loaded project with the editor opened"
    when: "the frame renders into a TestBackend buffer"
    then: "the picker asks which file and lists the project's spec source"
    # delegates: decispec-tui views::tests::editor_picker_lists_spec_files
  }
  scenario editor_shows_text_diagnostics_and_discard_warning {
    given: "a dirty buffer whose edit broke the parse, with Esc armed"
    when: "the frame renders into a TestBackend buffer"
    then: "the buffer shows the modified marker, the diagnostics with their file:line, and the esc close hint"
    # delegates: decispec-tui views::tests::editor_shows_text_diagnostics_and_discard_warning
  }
}

decision TUI-005 "The in-TUI spec editor is a pure line buffer whose live validation reuses parse's string entry" {
  status: accepted
  context: "F32 (2026-10-20, AGENT_TEAMS.md §6): phase-2 authoring edits a spec file in a text pane with live file:line diagnostics on every change — met by reusing parse::parse_file/merge/validate over the project's in-memory sources (the string entry the mcp crate's validate_spec already uses), no parse API changes. EditorState is pure (cursor in chars, dirty tracking, no I/O); cli owns reading the file, the Ctrl+S write into the project's specs/, and the draft-PR flow per each repo's rules."
  consequences: "+ the editor validates cross-file exactly like check does, phase-by-phase, and the buffer is unit-testable without a terminal. - every keystroke re-parses and re-validates the whole workspace, fine at spec-file scale but quadratic in project size; and a saved file is only as consistent as the validator — grammar changes in parse move this behavior silently."
}

spec TUI-005 {
  requirement TUI-005-R1 (EARS) {
    text: "WHEN the buffer is edited THEN a new editor SHALL start clean at the origin, insert SHALL place the character at the cursor and mark the buffer dirty, insert SHALL be char-safe for multi-byte text, newline SHALL split the line and move below, backspace SHALL delete the previous character or join the previous line at column 0 and be a no-op at the origin, and delete SHALL remove the character under the cursor or join the next line at end of line"
    layers: [unit]
    scenarios: [new_editor_starts_clean_at_origin, insert_char_appends_at_cursor, insert_unicode_is_char_safe, newline_splits_line_and_moves_down, backspace_deletes_char_then_joins_lines, backspace_at_origin_is_a_noop, delete_removes_char_then_joins_with_next_line]
  }
  scenario new_editor_starts_clean_at_origin {
    given: "a file and its two-line text"
    when: "EditorState::new builds the buffer"
    then: "the lines split on newlines, the cursor sits at 0,0, the buffer is clean, and text round-trips"
    # delegates: decispec-tui editor::tests::new_editor_starts_clean_at_origin
  }
  scenario insert_char_appends_at_cursor {
    given: "a buffer with the cursor after the first character"
    when: "a character is inserted"
    then: "the text gains it at the cursor and the buffer turns dirty"
    # delegates: decispec-tui editor::tests::insert_char_appends_at_cursor
  }
  scenario insert_unicode_is_char_safe {
    given: "a buffer containing multi-byte characters"
    when: "a multi-byte character is inserted and the cursor moves to the end"
    then: "the text and the char-based column are both correct"
    # delegates: decispec-tui editor::tests::insert_unicode_is_char_safe
  }
  scenario newline_splits_line_and_moves_down {
    given: "a one-line buffer with the cursor past the first character"
    when: "newline is pressed"
    then: "the line splits at the cursor, the cursor moves to the new line's start, and the buffer turns dirty"
    # delegates: decispec-tui editor::tests::newline_splits_line_and_moves_down
  }
  scenario backspace_deletes_char_then_joins_lines {
    given: "a buffer with the cursor at the end of a line"
    when: "backspace is pressed at the end and then at column 0"
    then: "the first press deletes the previous character and the second joins the line with the previous one"
    # delegates: decispec-tui editor::tests::backspace_deletes_char_then_joins_lines
  }
  scenario backspace_at_origin_is_a_noop {
    given: "a buffer with the cursor at 0,0"
    when: "backspace is pressed"
    then: "the text is unchanged and the buffer stays clean"
    # delegates: decispec-tui editor::tests::backspace_at_origin_is_a_noop
  }
  scenario delete_removes_char_then_joins_with_next_line {
    given: "a buffer with the cursor at the start of a line"
    when: "delete is pressed mid-line and then at end of line"
    then: "the first press removes the character under the cursor and the second pulls the next line up"
    # delegates: decispec-tui editor::tests::delete_removes_char_then_joins_with_next_line
  }
  requirement TUI-005-R2 (EARS) {
    text: "WHEN the cursor moves THEN arrows SHALL wrap across lines and clamp at both boundaries, home and end SHALL jump to the column extremes, page up and page down SHALL move by the pane height and clamp at the last line, and scrolling SHALL keep the cursor row visible"
    layers: [unit]
    scenarios: [arrow_moves_clamp_at_boundaries, home_and_end, page_moves_by_height_and_clamps, scroll_keeps_cursor_visible]
  }
  scenario arrow_moves_clamp_at_boundaries {
    given: "a two-line buffer"
    when: "the arrows are pressed past every edge"
    then: "left wraps to the previous line's end, right wraps to the next line's start, and up and down clamp"
    # delegates: decispec-tui editor::tests::arrow_moves_clamp_at_boundaries
  }
  scenario home_and_end {
    given: "a one-line buffer"
    when: "home and end are pressed"
    then: "the column jumps between 0 and the line length"
    # delegates: decispec-tui editor::tests::home_and_end
  }
  scenario page_moves_by_height_and_clamps {
    given: "a thirty-line buffer and a pane height of ten"
    when: "page down is pressed three times and page up once"
    then: "the cursor moves by ten each time, clamps at the last line, and returns to line nineteen"
    # delegates: decispec-tui editor::tests::page_moves_by_height_and_clamps
  }
  scenario scroll_keeps_cursor_visible {
    given: "a thirty-line buffer with a pane height of ten"
    when: "the cursor row is set past the viewport and scroll_to_cursor runs, then the cursor returns to the top"
    then: "the scroll offset places the cursor inside the viewport, then returns to zero"
    # delegates: decispec-tui editor::tests::scroll_keeps_cursor_visible
  }
  requirement TUI-005-R3 (EARS) {
    text: "WHEN an edit is validated against the project's sources THEN a clean edit SHALL report nothing, a parse error SHALL be reported alone with its file:line before any cross-reference noise, cross-file reference problems SHALL name the offending id without blaming untouched siblings, broken sibling files SHALL surface their own diagnostics, and undeclared flow participants SHALL be flagged"
    layers: [unit]
    scenarios: [validate_clean_edit_reports_nothing, validate_reports_parse_error_phase_first, validate_reports_cross_file_reference_problems, validate_reports_broken_sibling_files, validate_flags_undeclared_flow_participants]
  }
  scenario validate_clean_edit_reports_nothing {
    given: "a two-file workspace edited without changes"
    when: "validate_edit runs"
    then: "no diagnostics come back"
    # delegates: decispec-tui editor::tests::validate_clean_edit_reports_nothing
  }
  scenario validate_reports_parse_error_phase_first {
    given: "an edit that leaves a decision block unclosed"
    when: "validate_edit runs"
    then: "exactly one diagnostic comes back, a parse error at the edited file's line two"
    # delegates: decispec-tui editor::tests::validate_reports_parse_error_phase_first
  }
  scenario validate_reports_cross_file_reference_problems {
    given: "an edit pointing a requirement at a scenario that does not exist"
    when: "validate_edit runs"
    then: "a diagnostic names the missing scenario and none blames the untouched sibling file"
    # delegates: decispec-tui editor::tests::validate_reports_cross_file_reference_problems
  }
  scenario validate_reports_broken_sibling_files {
    given: "a sibling file whose requirement lost its scenario, edited file untouched"
    when: "validate_edit runs on the clean file"
    then: "a diagnostic names the broken sibling and its missing scenario"
    # delegates: decispec-tui editor::tests::validate_reports_broken_sibling_files
  }
  scenario validate_flags_undeclared_flow_participants {
    given: "an edit adding a flow over an undeclared participant"
    when: "validate_edit runs"
    then: "a diagnostic names the participant, and the unedited file stays clean"
    # delegates: decispec-tui editor::tests::validate_flags_undeclared_flow_participants
  }
}

decision TUI-006 "A new spec file is born from a name prompt and a parseable skeleton, and touches disk only through the existing explicit save" {
  status: accepted
  context: "F36 (2026-10-20, user-reported gap): the F32 editor's picker opened only existing spec files — a project with none got 'no spec files to edit', so the cockpit could not start work on a fresh project. Creation reuses the F32 save seam: the name prompt (n in the picker, or e with no files) validates the name (relative to specs/, must end in .spec, no .., no collision), the editor opens on skeleton_for's minimal PREFIX-001 decision+spec that already passes live validation, and Ctrl+S is the only write."
  consequences: "+ a zero-spec project is fully authorable from the cockpit, and the skeleton opens with zero diagnostics so validation noise means the author's edits, not the template. - the PREFIX-001 placeholders must be renamed before check passes, and the picker title must advertise the second sub-mode (n = new)."
}

spec TUI-006 {
  requirement TUI-006-R1 (EARS) {
    text: "WHEN the editor is requested on a project without spec files THEN the name prompt SHALL open directly, and WHEN the picker is open THEN n SHALL switch to the name prompt and esc SHALL return to the picker, or close the editor when there is no picker to return to"
    layers: [unit]
    scenarios: [e_on_project_without_specs_opens_naming_directly, picker_n_opens_naming_and_esc_returns_to_picker, esc_in_naming_with_no_files_closes_the_editor]
  }
  scenario e_on_project_without_specs_opens_naming_directly {
    given: "a registered project whose sources list is empty"
    when: "the editor is requested"
    then: "the name prompt opens directly instead of a status message"
    # delegates: decispec-tui app::tests::e_on_project_without_specs_opens_naming_directly
  }
  scenario picker_n_opens_naming_and_esc_returns_to_picker {
    given: "the file picker over a project with spec files"
    when: "n is pressed, then esc"
    then: "the name prompt opens and esc returns to the picker"
    # delegates: decispec-tui app::tests::picker_n_opens_naming_and_esc_returns_to_picker
  }
  scenario esc_in_naming_with_no_files_closes_the_editor {
    given: "the name prompt on a project without spec files"
    when: "esc is pressed"
    then: "the editor closes"
    # delegates: decispec-tui app::tests::esc_in_naming_with_no_files_closes_the_editor
  }
  requirement TUI-006-R2 (EARS) {
    text: "WHEN a valid new-file name is confirmed THEN the editor SHALL open on a parseable skeleton at specs/<name> with a dirty buffer and an empty diagnostics pane, and WHEN the name is invalid — missing .spec, traversal, or a collision with an existing file — THEN it SHALL be refused with a status message and the prompt SHALL stay open"
    layers: [unit]
    scenarios: [naming_enter_opens_editor_on_parseable_skeleton, naming_enter_refuses_invalid_names_and_stays_in_naming, skeleton_for_new_file_parses_clean]
  }
  scenario naming_enter_opens_editor_on_parseable_skeleton {
    given: "the name prompt"
    when: "010-new.spec is typed and confirmed"
    then: "the editor opens on specs/010-new.spec with the PREFIX-001 skeleton, a dirty buffer, and no diagnostics"
    # delegates: decispec-tui app::tests::naming_enter_opens_editor_on_parseable_skeleton
  }
  scenario naming_enter_refuses_invalid_names_and_stays_in_naming {
    given: "the name prompt"
    when: "a name without .spec, a .. traversal, and a collision with the existing spec file are each confirmed"
    then: "each is refused with a status message naming the reason and the prompt stays open"
    # delegates: decispec-tui app::tests::naming_enter_refuses_invalid_names_and_stays_in_naming
  }
  scenario skeleton_for_new_file_parses_clean {
    given: "the new-file skeleton text"
    when: "validate_edit runs against sibling sources and against an empty workspace"
    then: "both report no diagnostics"
    # delegates: decispec-tui editor::tests::skeleton_for_new_file_parses_clean
  }
}

decision TUI-007 "The spec-file picker filters on demand through '/' so every printable key stays a command" {
  status: accepted
  context: "F42 (2026-10-20 console queue): the picker was arrow-only, so projects with dozens of specs cost one keystroke per row of scanning. The TUI's keymap is command-letter based (e/n/p/q are all taken), so making every printable character a live filter would collide with existing commands — notably `n`, whose naming behavior is pinned by TUI-006. Filtering is therefore opt-in: '/' engages it, the query narrows case-insensitively by substring, and Esc backs out in one step."
  consequences: "+ finding a spec in a large project is a slash and a few characters, and entering filter mode is reversible without losing the list. - filtering costs an extra keystroke versus fzf-style always-on typing, and while filter mode is engaged printable keys (including `n`) cannot reach the command keymap, which is exactly the intent — Esc is the documented way back."
}

spec TUI-007 {
  requirement TUI-007-R1 (EARS) {
    text: "WHEN the picker is open and '/' is pressed THEN subsequent characters SHALL narrow the list by case-insensitive substring, Backspace SHALL edit the query, the selection SHALL re-anchor to the first match on every edit, Enter SHALL open the highlighted match and clear the filter, and Esc SHALL clear the filter and leave filter mode while a further Esc closes the picker"
    layers: [unit]
    scenarios: [picker_slash_enters_filter_mode_and_typing_narrows_to_the_match, picker_filter_is_case_insensitive_and_selection_anchors_to_top, picker_esc_clears_filter_then_closes_and_n_still_names]
  }
  requirement TUI-007-R2 (EARS) {
    text: "WHILE the picker renders THEN it SHALL advertise the '/' filter in its title and, once a query is non-empty, SHALL show the query and the match count beside the narrowed list"
    layers: [unit]
    scenarios: [picker_filter_line_shows_query_and_match_count]
  }
  scenario picker_slash_enters_filter_mode_and_typing_narrows_to_the_match {
    given: "a picker over three spec files"
    when: "the filter is engaged, 'user' is typed, an 'n' is typed, then Backspace and Enter"
    then: "'n' lands in the query instead of opening naming, backspacing restores the match, and Enter opens the matching file with the filter cleared"
    # delegates: decispec-tui app::tests::picker_slash_enters_filter_mode_and_typing_narrows_to_the_match
  }
  scenario picker_filter_is_case_insensitive_and_selection_anchors_to_top {
    given: "the same three-file picker"
    when: "'ADMIN' is typed, the query is cleared by Backspace, the selection moves down, and another character is typed"
    then: "matching is case-insensitive, clearing shows all files, and every query edit re-anchors the selection to the first match"
    # delegates: decispec-tui app::tests::picker_filter_is_case_insensitive_and_selection_anchors_to_top
  }
  scenario picker_esc_clears_filter_then_closes_and_n_still_names {
    given: "an engaged filter with one match"
    when: "Esc is pressed twice, then 'n' is pressed in a fresh picker"
    then: "the first Esc clears the query and keeps the picker open, the second closes it, and 'n' still opens the naming prompt outside filter mode"
    # delegates: decispec-tui app::tests::picker_esc_clears_filter_then_closes_and_n_still_names
  }
  scenario picker_filter_line_shows_query_and_match_count {
    given: "the picker rendered on a terminal"
    when: "the filter hint is shown before use and a query narrowing one of two files is typed"
    then: "the title advertises '/' , then shows the query and its match count while the non-match is absent from the list"
    # delegates: decispec-tui views::tests::picker_filter_line_shows_query_and_match_count
  }
}

decision TUI-008 "The specs view decision list filters on demand through '/' over id and title" {
  status: accepted
  context: "User request 2026-10-21: with 44 decision blocks in this repo's own spec and larger suites in registered projects, scanning the specs view cost one keystroke per row of scrolling. TUI-007 had just proven the on-demand filter idiom for the file picker, and the same command-letter keymap collision argument applies unchanged — '/' engages, so e/p/q stay commands until engaged. The match spans id and title because agents remember ids (CMD-001) and humans remember words in the choice sentence. Enter deliberately keeps the query here where the picker cleared it: a picker acts on its highlight, while a narrowed decision list is for browsing, so arrows must work on the matches after Enter."
  consequences: "+ a module prefix or a word in the title narrows the list in a few keystrokes, reversible in one Esc, and the match count says how much of the suite is showing. - printable keys are captured while filter mode is engaged (the documented way back is Esc), and a kept query can surprise users coming from the picker, where Enter clears — the render line makes the kept query visible."
}

spec TUI-008 {
  requirement TUI-008-R1 (EARS) {
    text: "WHEN '/' is pressed in the specs view with no overlay active THEN subsequent characters SHALL narrow the decision list by case-insensitive substring over id and title, Backspace SHALL edit the query, the selection SHALL re-anchor to the first match on every edit, Enter SHALL leave filter mode keeping the query so the arrows browse the narrowed list, and Esc SHALL clear the query and leave filter mode"
    layers: [unit]
    scenarios: [tui_slash_engages_decision_filter_and_typing_narrows, tui_decision_filter_is_case_insensitive_and_reanchors_selection, tui_esc_clears_decision_filter_and_normal_keys_resume, tui_switching_project_resets_decision_filter]
  }
  requirement TUI-008-R2 (EARS) {
    text: "WHILE the specs view renders with filter mode engaged or a non-empty query THEN it SHALL show the query and the match count beside the narrowed decision list"
    layers: [unit]
    scenarios: [specs_view_shows_filter_line_and_narrows_rows]
  }
  scenario tui_slash_engages_decision_filter_and_typing_narrows {
    given: "a specs view over several decisions"
    when: "'/' is pressed and a substring of one decision's title is typed"
    then: "filter mode is engaged, the list narrows to the matching decisions, and keys that are commands outside filtering (like 'e') land in the query"
    # delegates: decispec-tui app::tests::tui_slash_engages_decision_filter_and_typing_narrows
  }
  scenario tui_decision_filter_is_case_insensitive_and_reanchors_selection {
    given: "the same decision list"
    when: "an uppercase query is typed, the query is edited by Backspace, and the selection sits below the first match"
    then: "matching is case-insensitive and every query edit re-anchors the selection to the first match"
    # delegates: decispec-tui app::tests::tui_decision_filter_is_case_insensitive_and_reanchors_selection
  }
  scenario tui_esc_clears_decision_filter_and_normal_keys_resume {
    given: "an engaged filter with a non-empty query"
    when: "Esc is pressed, then 'q' is pressed"
    then: "the query is cleared, filter mode is left, and 'q' quits again instead of landing in a filter"
    # delegates: decispec-tui app::tests::tui_esc_clears_decision_filter_and_normal_keys_resume
  }
  scenario tui_switching_project_resets_decision_filter {
    given: "a kept query on one project"
    when: "the next project is selected"
    then: "the query, filter mode, and selection are reset for the new project"
    # delegates: decispec-tui app::tests::tui_switching_project_resets_decision_filter
  }
  scenario specs_view_shows_filter_line_and_narrows_rows {
    given: "the specs view rendered on a terminal"
    when: "a query narrowing several decisions to one is typed"
    then: "the filter line shows the query and its match count and the non-matching decisions are absent from the list"
    # delegates: decispec-tui views::tests::specs_view_shows_filter_line_and_narrows_rows
  }
}

decision TUI-009 "The cockpit opens on the project you are in, not on the registry" {
  status: accepted
  context: "User request 2026-10-26: `decispec tui` used to open with every registered project (seeded on first run), so editing the spec of the project at hand meant landing in a foreign multi-project switcher — and first run silently wrote a registry the user never asked for. The cockpit is a spec-authoring tool; the common case is one project, entered from its own directory. F50: inside a project (cwd walking up to decispec.toml) the app opens with exactly that project, the registry file is not even read, and no seeding happens. The registry survives only as the fallback when cwd is outside any project, and `--all` forces it."
  consequences: "+ zero ceremony in the common case: run `decispec tui` and you are editing this project's specs. - the multi-project dashboard becomes the exception (--all or a cwd outside projects), and a first run no longer pre-populates the registry, so `--all` on a fresh machine opens empty until `tui --register` adds something."
}

spec TUI-009 {
  requirement TUI-009-R1 (EARS) {
    text: "WHEN the cockpit starts with cwd inside a project and without --all THEN it SHALL open with exactly that project and SHALL NOT read, create, or seed the registry"
    layers: [unit]
    scenarios: [tui_opens_current_project_only_when_inside_one, tui_all_flag_keeps_registry_mode_inside_a_project]
  }
  requirement TUI-009-R2 (EARS) {
    text: "WHEN the cockpit starts with cwd outside any project THEN it SHALL fall back to the registered projects (seeding on first run as before)"
    layers: [unit]
    scenarios: [tui_falls_back_to_registry_outside_a_project]
  }
  scenario tui_opens_current_project_only_when_inside_one {
    given: "a project and a cwd nested inside it, plus an absent registry file"
    when: "the startup project list is resolved without --all"
    then: "the list holds exactly that project and the registry file was never created"
    # delegates: decispec (cli) tests::tui_opens_current_project_only_when_inside_one
  }
  scenario tui_all_flag_keeps_registry_mode_inside_a_project {
    given: "a project, a cwd inside it, and a registry naming another project"
    when: "the startup list is resolved with --all"
    then: "the registry projects are used, not the current one"
    # delegates: decispec (cli) tests::tui_all_flag_keeps_registry_mode_inside_a_project
  }
  scenario tui_falls_back_to_registry_outside_a_project {
    given: "a cwd in a bare directory and a registry naming one project"
    when: "the startup list is resolved without --all"
    then: "the registry projects are used"
    # delegates: decispec (cli) tests::tui_falls_back_to_registry_outside_a_project
  }
}
