//! The spec editor: a small pure text editor over a spec file's lines, with
//! live whole-workspace validation. No I/O — `cli` reads and writes the
//! file; this module only mutates [`EditorState`] and reports diagnostics.

use decispec_parse as parse;

/// Pure editing state for one spec file.
pub struct EditorState {
    /// Project-relative path of the file being edited.
    pub file: String,
    /// Buffer lines (no newline characters).
    pub lines: Vec<String>,
    /// Cursor position in chars.
    pub row: usize,
    pub col: usize,
    /// First visible line (scroll offset).
    pub top: usize,
    /// True when the buffer differs from what was loaded/saved.
    pub dirty: bool,
}

fn char_to_byte(line: &str, char_idx: usize) -> usize {
    line.char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(line.len())
}

impl EditorState {
    pub fn new(file: &str, text: &str) -> Self {
        EditorState {
            file: file.to_string(),
            lines: text.split('\n').map(str::to_string).collect(),
            row: 0,
            col: 0,
            top: 0,
            dirty: false,
        }
    }

    /// The buffer as file text (trailing newline not implied).
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    fn line_len(&self) -> usize {
        self.lines
            .get(self.row)
            .map(|l| l.chars().count())
            .unwrap_or(0)
    }

    fn touch(&mut self) {
        self.dirty = true;
    }

    fn clamp_cursor(&mut self) {
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        self.row = self.row.min(self.lines.len() - 1);
        self.col = self.col.min(self.line_len());
    }

    pub fn insert_char(&mut self, c: char) {
        self.clamp_cursor();
        let byte = char_to_byte(&self.lines[self.row], self.col);
        self.lines[self.row].insert(byte, c);
        self.col += 1;
        self.touch();
    }

    pub fn newline(&mut self) {
        self.clamp_cursor();
        let byte = char_to_byte(&self.lines[self.row], self.col);
        let rest = self.lines[self.row].split_off(byte);
        self.lines.insert(self.row + 1, rest);
        self.row += 1;
        self.col = 0;
        self.touch();
    }

    /// Backspace: delete the char before the cursor; at column 0 join with
    /// the previous line.
    pub fn backspace(&mut self) {
        self.clamp_cursor();
        if self.col > 0 {
            let start = char_to_byte(&self.lines[self.row], self.col - 1);
            let end = char_to_byte(&self.lines[self.row], self.col);
            self.lines[self.row].replace_range(start..end, "");
            self.col -= 1;
            self.touch();
        } else if self.row > 0 {
            let line = self.lines.remove(self.row);
            self.row -= 1;
            self.col = self.lines[self.row].chars().count();
            self.lines[self.row].push_str(&line);
            self.touch();
        }
    }

    /// Delete: delete the char under the cursor; at end of line join with the
    /// next line.
    pub fn delete(&mut self) {
        self.clamp_cursor();
        let len = self.line_len();
        if self.col < len {
            let start = char_to_byte(&self.lines[self.row], self.col);
            let end = char_to_byte(&self.lines[self.row], self.col + 1);
            self.lines[self.row].replace_range(start..end, "");
            self.touch();
        } else if self.row + 1 < self.lines.len() {
            let next = self.lines.remove(self.row + 1);
            self.lines[self.row].push_str(&next);
            self.touch();
        }
    }

    pub fn move_left(&mut self) {
        if self.col > 0 {
            self.col -= 1;
        } else if self.row > 0 {
            self.row -= 1;
            self.col = self.line_len();
        }
    }

    pub fn move_right(&mut self) {
        if self.col < self.line_len() {
            self.col += 1;
        } else if self.row + 1 < self.lines.len() {
            self.row += 1;
            self.col = 0;
        }
    }

    pub fn move_up(&mut self) {
        self.row = self.row.saturating_sub(1);
        self.col = self.col.min(self.line_len());
    }

    pub fn move_down(&mut self) {
        self.row = (self.row + 1).min(self.lines.len().saturating_sub(1));
        self.col = self.col.min(self.line_len());
    }

    pub fn move_home(&mut self) {
        self.col = 0;
    }

    pub fn move_end(&mut self) {
        self.col = self.line_len();
    }

    pub fn page_up(&mut self, height: usize) {
        self.row = self.row.saturating_sub(height.max(1));
        self.col = self.col.min(self.line_len());
    }

    pub fn page_down(&mut self, height: usize) {
        self.row = (self.row + height.max(1)).min(self.lines.len().saturating_sub(1));
        self.col = self.col.min(self.line_len());
    }

    /// Keep the cursor visible: adjusts `top` so `row` is within
    /// `[top, top + height)`.
    pub fn scroll_to_cursor(&mut self, height: usize) {
        self.top = visible_top(self.row, self.top, height);
    }
}

/// First visible line that keeps `row` in `[top, top + height)`. Shared by
/// the editor state and the (immutable) renderer.
pub fn visible_top(row: usize, top: usize, height: usize) -> usize {
    let height = height.max(1);
    if row < top {
        row
    } else if row >= top + height {
        row + 1 - height
    } else {
        top
    }
}

/// Live diagnostics for an in-progress edit: the edited text replaces `file`
/// in the project's `sources` (project-relative path -> text), everything is
/// parsed and merged, and both parse and cross-reference problems come back
/// as `file:line: message` strings. Parse errors are reported first and
/// alone (phase-by-phase, like `validate_spec`), and a file whose siblings
/// fail to parse reports those too — the editor never hides context.
pub fn validate_edit(sources: &[(String, String)], file: &str, text: &str) -> Vec<String> {
    let mut units = Vec::new();
    let mut diags = Vec::new();
    for (f, src) in sources {
        if f == file {
            continue;
        }
        match parse::parse_file(f, src) {
            Ok(u) => units.push(u),
            Err(e) => diags.push(e.to_string()),
        }
    }
    match parse::parse_file(file, text) {
        Ok(u) => units.push(u),
        Err(e) => {
            // parse errors alone, phase-by-phase like validate_spec
            diags.push(e.to_string());
            return diags;
        }
    }
    if !diags.is_empty() {
        return diags;
    }
    let ws = parse::merge(units);
    parse::validate(&ws).iter().map(|d| d.to_string()).collect()
}

/// Starter text for a brand-new spec file: a minimal valid decision + spec
/// with placeholder ids the author renames. Must pass [`validate_edit`]
/// clean so the new-file editor opens with zero diagnostics (pinned by
/// `skeleton_for_new_file_parses_clean`).
pub fn skeleton_for(file: &str) -> String {
    format!(
        r#"# {file} — new spec (created by decispec tui).
# Rename PREFIX to your id prefix; guide: docs/SPEC_AUTHORING.md.

decision PREFIX-001 "One-line decision title" {{
  status: proposed
  context: "Why this decision exists and what forced it now."
  consequences: "What saying yes commits the project to."
}}

spec PREFIX-001 {{
  requirement PREFIX-001-R1 (EARS) {{
    text: "WHEN an event THEN an outcome SHALL a constraint"
    layers: [unit]
    scenarios: [describe_the_behavior]
  }}
  scenario describe_the_behavior {{
    given: "the precondition"
    when: "the action"
    then: "the observable result"
  }}
}}
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn many_lines(n: usize) -> String {
        std::iter::repeat_n("x", n).collect::<Vec<_>>().join("\n")
    }

    const FILE_A: &str = r#"decision D1 "First" {
  status: accepted
  context: "c"
}

model {
  container x "X"
}

spec D1 {
  requirement D1-R1 (EARS) {
    text: "WHEN a THEN b SHALL c"
    layers: [unit]
    scenarios: [s1]
  }
  scenario s1 {
    given: "g"
    when: "w"
    then: "t"
  }
}
"#;

    const FILE_B: &str = r#"spec D1 {
  requirement D1-R2 (EARS) {
    text: "WHEN x THEN y SHALL z"
    layers: [unit]
    scenarios: [s2]
  }
  scenario s2 {
    given: "g"
    when: "w"
    then: "t"
  }
}
"#;

    fn sources() -> Vec<(String, String)> {
        vec![
            ("specs/000-a.spec".to_string(), FILE_A.to_string()),
            ("specs/001-b.spec".to_string(), FILE_B.to_string()),
        ]
    }

    #[test]
    fn new_editor_starts_clean_at_origin() {
        let ed = EditorState::new("specs/000-a.spec", "ab\ncde");
        assert_eq!(ed.lines, vec!["ab".to_string(), "cde".to_string()]);
        assert_eq!((ed.row, ed.col), (0, 0));
        assert!(!ed.dirty);
        assert_eq!(ed.text(), "ab\ncde");
    }

    #[test]
    fn insert_char_appends_at_cursor() {
        let mut ed = EditorState::new("f", "ac");
        ed.move_right();
        ed.insert_char('b');
        assert_eq!(ed.text(), "abc");
        assert!(ed.dirty);
    }

    #[test]
    fn insert_unicode_is_char_safe() {
        let mut ed = EditorState::new("f", "aéz");
        ed.move_right();
        ed.insert_char('ü');
        assert_eq!(ed.text(), "aüéz");
        ed.move_end();
        assert_eq!(ed.col, 4);
    }

    #[test]
    fn newline_splits_line_and_moves_down() {
        let mut ed = EditorState::new("f", "ab");
        ed.move_right();
        ed.newline();
        assert_eq!(ed.lines, vec!["a".to_string(), "b".to_string()]);
        assert_eq!((ed.row, ed.col), (1, 0));
        assert!(ed.dirty);
    }

    #[test]
    fn backspace_deletes_char_then_joins_lines() {
        let mut ed = EditorState::new("f", "abc");
        ed.move_end();
        ed.backspace();
        assert_eq!(ed.text(), "ab");
        assert_eq!(ed.col, 2);
        // at column 0: join with previous line
        ed.newline();
        ed.backspace();
        assert_eq!(ed.text(), "ab");
        assert_eq!((ed.row, ed.col), (0, 2));
    }

    #[test]
    fn backspace_at_origin_is_a_noop() {
        let mut ed = EditorState::new("f", "ab");
        ed.backspace();
        assert_eq!(ed.text(), "ab");
        assert!(!ed.dirty);
    }

    #[test]
    fn delete_removes_char_then_joins_with_next_line() {
        let mut ed = EditorState::new("f", "abc");
        ed.delete();
        assert_eq!(ed.text(), "bc");
        ed.move_end();
        ed.delete();
        assert_eq!(ed.text(), "bc");
        // at end of line: pull the next line up
        ed.newline();
        ed.move_up();
        ed.move_end();
        ed.delete();
        assert_eq!(ed.text(), "bc");
        assert_eq!(ed.row, 0);
    }

    #[test]
    fn arrow_moves_clamp_at_boundaries() {
        let mut ed = EditorState::new("f", "ab\nc");
        ed.move_left();
        assert_eq!((ed.row, ed.col), (0, 0));
        ed.move_up();
        assert_eq!(ed.row, 0);
        ed.move_right(); // col 1
        ed.move_right(); // col 2 (end)
        ed.move_right(); // wrap to next line, col 0
        assert_eq!((ed.row, ed.col), (1, 0));
        ed.move_down();
        assert_eq!(ed.row, 1); // clamp
        ed.move_end();
        assert_eq!(ed.col, 1);
        ed.move_right(); // nowhere left
        assert_eq!((ed.row, ed.col), (1, 1));
    }

    #[test]
    fn home_and_end() {
        let mut ed = EditorState::new("f", "hello");
        ed.move_end();
        ed.move_home();
        assert_eq!(ed.col, 0);
        ed.move_end();
        assert_eq!(ed.col, 5);
    }

    #[test]
    fn page_moves_by_height_and_clamps() {
        let mut ed = EditorState::new("f", &many_lines(30));
        ed.move_home();
        ed.page_down(10);
        assert_eq!(ed.row, 10);
        ed.page_down(10);
        assert_eq!(ed.row, 20);
        ed.page_down(10);
        assert_eq!(ed.row, 29); // clamps at last line
        ed.page_up(10);
        assert_eq!(ed.row, 19);
    }

    #[test]
    fn scroll_keeps_cursor_visible() {
        let mut ed = EditorState::new("f", &many_lines(30));
        ed.row = 15;
        ed.scroll_to_cursor(10);
        assert!(ed.row >= ed.top && ed.row < ed.top + 10);
        ed.row = 0;
        ed.scroll_to_cursor(10);
        assert_eq!(ed.top, 0);
    }

    // -- live validation ---------------------------------------------------

    #[test]
    fn validate_clean_edit_reports_nothing() {
        let diags = validate_edit(&sources(), "specs/001-b.spec", FILE_B);
        assert!(diags.is_empty(), "diags: {diags:?}");
    }

    #[test]
    fn validate_reports_parse_error_phase_first() {
        let broken = "decision D1 \"x\" {\n  status: accepted\n"; // unclosed
        let diags = validate_edit(&sources(), "specs/001-b.spec", broken);
        assert_eq!(diags.len(), 1, "diags: {diags:?}");
        assert!(diags[0].contains("specs/001-b.spec:2"), "diags: {diags:?}");
    }

    #[test]
    fn validate_reports_cross_file_reference_problems() {
        // point D1-R2 at a scenario that does not exist: the validator must
        // see the OTHER file's decision (cross-file ref), not flag D1 itself
        let bad = FILE_B.replace("scenarios: [s2]", "scenarios: [s2, nope]");
        let diags = validate_edit(&sources(), "specs/001-b.spec", &bad);
        assert!(
            diags.iter().any(|d| d.contains("nope")),
            "diags: {diags:?}"
        );
        // and it must NOT complain about the untouched sibling file
        assert!(
            diags.iter().all(|d| !d.contains("000-a.spec")),
            "diags: {diags:?}"
        );
    }

    #[test]
    fn validate_reports_broken_sibling_files() {
        let bad_a = FILE_A.replace("scenario s1", "scenario s1x");
        let srcs = vec![
            ("specs/000-a.spec".to_string(), bad_a),
            ("specs/001-b.spec".to_string(), FILE_B.to_string()),
        ];
        // editing B is fine, but A's D1-R1 now references a missing scenario
        let diags = validate_edit(&srcs, "specs/001-b.spec", FILE_B);
        assert!(
            diags.iter().any(|d| d.contains("s1") && d.contains("000-a.spec")),
            "diags: {diags:?}"
        );
    }

    #[test]
    fn validate_flags_undeclared_flow_participants() {
        // flow participant `q` is not declared as a container
        let bad = FILE_A.replace(
            "model {\n  container x \"X\"\n}",
            "model {\n  container x \"X\"\n  flow f2 {\n    q -> x: \"m\"\n  }\n}",
        );
        let diags = validate_edit(&sources(), "specs/000-a.spec", &bad);
        assert!(
            diags.iter().any(|d| d.contains("q")),
            "diags: {diags:?}"
        );
        let clean = validate_edit(&sources(), "specs/000-a.spec", FILE_A);
        assert!(clean.is_empty(), "diags: {clean:?}");
    }

    #[test]
    fn skeleton_for_new_file_parses_clean() {
        let skel = super::skeleton_for("specs/010-new.spec");
        assert!(skel.contains("decision PREFIX-001"));
        // clean against sibling sources and against an empty workspace
        let diags = validate_edit(&sources(), "specs/010-new.spec", &skel);
        assert!(diags.is_empty(), "diags: {diags:?}");
        let solo = validate_edit(&[], "specs/010-new.spec", &skel);
        assert!(solo.is_empty(), "diags: {solo:?}");
    }
}
