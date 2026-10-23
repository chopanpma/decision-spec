//! App state and input transitions. Pure: `update` mutates state and returns
//! an [`Action`] for the (cli-owned) event loop to act on.

use std::path::PathBuf;

use decispec_codegen::{CodegenConfig, GeneratedFile, render_all, render_index};
use decispec_gate::coverage::Coverage;
use decispec_ir::{Decision, Layer, Status, Workspace};

use crate::editor::{EditorState, validate_edit};
use crate::snapshot::GateSnapshot;

/// One registered project, fully loaded (or carrying its load error so the
/// UI can show it instead of crashing the cockpit).
pub struct ProjectData {
    pub name: String,
    pub path: PathBuf,
    /// `None` when the project's specs failed to parse/validate.
    pub workspace: Option<Workspace>,
    /// `file:line` diagnostics, set when `workspace` is `None`.
    pub load_error: Option<String>,
    /// Every spec file's project-relative path and text — the editor's
    /// buffer source and live-validation context. Populated even when the
    /// workspace failed to parse (that is exactly when editing is needed).
    pub sources: Vec<(String, String)>,
    /// Codegen settings from the project's decispec.toml (artifact preview).
    pub codegen: Option<CodegenConfig>,
    /// Last gate report, when the project has run one.
    pub gate: Option<GateSnapshot>,
    /// Proof rollup of `gate` (PROOF-004): per-decision proof states plus the
    /// headline percentage. `None` when there is no report — the views render
    /// an explicit "no gate run" state rather than a fabricated zero.
    pub coverage: Option<Coverage>,
}

impl ProjectData {
    pub fn loaded(
        name: String,
        path: PathBuf,
        workspace: Workspace,
        sources: Vec<(String, String)>,
        codegen: CodegenConfig,
        gate: Option<GateSnapshot>,
    ) -> Self {
        let coverage = gate.as_ref().map(GateSnapshot::coverage);
        ProjectData {
            name,
            path,
            workspace: Some(workspace),
            load_error: None,
            sources,
            codegen: Some(codegen),
            gate,
            coverage,
        }
    }

    pub fn error(
        name: String,
        path: PathBuf,
        message: String,
        sources: Vec<(String, String)>,
    ) -> Self {
        ProjectData {
            name,
            path,
            workspace: None,
            load_error: Some(message),
            sources,
            codegen: None,
            gate: None,
            coverage: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Specs,
    Artifacts,
    Dashboard,
}

impl Mode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Mode::Specs => "specs",
            Mode::Artifacts => "artifacts",
            Mode::Dashboard => "dashboard",
        }
    }
}

/// Terminal-independent input vocabulary. `cli` translates crossterm events.
/// `Ctrl(c)` carries control-modified chars (e.g. Ctrl+S save in the editor).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Up,
    Down,
    Left,
    Right,
    Tab,
    Enter,
    Esc,
    Home,
    End,
    PageUp,
    PageDown,
    Backspace,
    Delete,
    Char(char),
    Ctrl(char),
}

/// What the owning event loop must do after an update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    None,
    Quit,
    Reload,
    /// Write the edited spec file into the current project, then re-check.
    SaveSpec {
        file: String,
        text: String,
    },
    /// Open a draft PR for the current project's uncommitted spec changes.
    CreatePr,
}

pub struct App {
    pub projects: Vec<ProjectData>,
    pub project: usize,
    pub mode: Mode,
    /// Selected decision within the current project's decision list.
    pub decision: usize,
    /// Selected artifact within the artifact list.
    pub artifact: usize,
    /// Scroll offset shared by the detail/artifact panes.
    pub scroll: u16,
    /// One-line message for the status bar (e.g. after a reload).
    pub status: String,
    /// Open spec editor (file picker or editing). While open it owns input.
    pub editor: Option<Editor>,
    /// `true` = characters narrow the specs-view decision list instead of
    /// firing the command keymap (F43). Separate from the editor picker's
    /// filter — the overlay has its own.
    pub filtering: bool,
    /// Case-insensitive substring over `id + " " + title` narrowing the
    /// specs-view decision list (F43). Survives leaving filter-edit mode
    /// (Enter) so the narrowed list stays browsable.
    pub filter: String,
}

/// The spec editor session: pick a file or name a new one, edit it, watch
/// live diagnostics.
pub struct Editor {
    /// `true` = choosing a file; `false` = editing `state.file`.
    pub picking: bool,
    pub pick_idx: usize,
    /// `true` = characters go to the picker filter, not the keymap (F42).
    pub filtering: bool,
    /// Case-insensitive substring narrowing the picker list while `filtering`.
    pub filter: String,
    /// `true` = typing a new file's name; `false` = picking or editing.
    pub naming: bool,
    /// The typed name in naming mode (relative to `specs/`).
    pub name: String,
    pub state: EditorState,
    /// Live `file:line` diagnostics for the current buffer.
    pub diagnostics: Vec<String>,
    /// Set after the first Esc on a dirty buffer; the second Esc discards.
    pub confirm_discard: bool,
}

impl App {
    pub fn new(projects: Vec<ProjectData>) -> Self {
        App {
            status: if projects.is_empty() {
                "no projects registered — run `decispec tui --register <path>`".to_string()
            } else {
                format!("{} project(s) loaded", projects.len())
            },
            projects,
            project: 0,
            mode: Mode::Specs,
            decision: 0,
            artifact: 0,
            scroll: 0,
            editor: None,
            filtering: false,
            filter: String::new(),
        }
    }

    pub fn current(&self) -> Option<&ProjectData> {
        self.projects.get(self.project)
    }

    fn reset_view_state(&mut self) {
        self.decision = 0;
        self.artifact = 0;
        self.scroll = 0;
        // the filter belongs to the view being reset (F43)
        self.filtering = false;
        self.filter.clear();
    }

    /// File names the picker currently shows: every source while no filter
    /// is engaged, the case-insensitive substring matches otherwise (F42).
    pub fn pickable(&self) -> Vec<String> {
        let Some(p) = self.current() else {
            return Vec::new();
        };
        let query = self
            .editor
            .as_ref()
            .filter(|e| e.picking)
            .map(|e| e.filter.to_lowercase())
            .unwrap_or_default();
        p.sources
            .iter()
            .map(|(f, _)| f.clone())
            .filter(|f| f.to_lowercase().contains(&query))
            .collect()
    }

    /// Decisions the specs view lists: every decision while the filter is
    /// disengaged and empty, otherwise the case-insensitive substring
    /// matches on `id + " " + title` (F43). The single source of truth for
    /// update()'s selection clamp and views.rs rendering — the decision
    /// list's `pickable()`.
    pub fn visible_decisions(&self) -> Vec<&Decision> {
        let Some(ws) = self.current().and_then(|p| p.workspace.as_ref()) else {
            return Vec::new();
        };
        if !self.filtering && self.filter.is_empty() {
            return ws.decisions.iter().collect();
        }
        let query = self.filter.to_lowercase();
        ws.decisions
            .iter()
            .filter(|d| format!("{} {}", d.id, d.title).to_lowercase().contains(&query))
            .collect()
    }

    fn open_picker(&mut self) {
        let n = self.current().map(|p| p.sources.len()).unwrap_or(0);
        self.editor = Some(Editor {
            picking: n > 0,
            pick_idx: 0,
            filtering: false,
            filter: String::new(),
            naming: n == 0,
            name: String::new(),
            state: EditorState::new("", ""),
            diagnostics: Vec::new(),
            confirm_discard: false,
        });
        if n == 0 {
            self.status = "name a new spec file (relative to specs/)".to_string();
        }
    }

    /// After a successful save (cli wrote the file and refreshed the
    /// project): the buffer is clean again; keep the editor open.
    pub fn mark_saved(&mut self) {
        let sources = self
            .current()
            .map(|p| p.sources.clone())
            .unwrap_or_default();
        if let Some(ed) = self.editor.as_mut() {
            ed.state.dirty = false;
            ed.confirm_discard = false;
            ed.diagnostics =
                validate_edit(&sources, &ed.state.file, &ed.state.text());
        }
    }

    fn update_editor(&mut self, input: Input) -> Action {
        // Ctrl+S: save only a dirty buffer.
        if input == Input::Ctrl('s') {
            let ed = self.editor.as_ref().expect("editor open");
            if ed.state.dirty {
                return Action::SaveSpec {
                    file: ed.state.file.clone(),
                    text: ed.state.text(),
                };
            }
            return Action::None;
        }
        // naming mode: type the new file's name; Enter opens the skeleton.
        if self.editor.as_ref().is_some_and(|e| e.naming) {
            let has_files = self.current().is_some_and(|p| !p.sources.is_empty());
            let sources = self
                .current()
                .map(|p| p.sources.clone())
                .unwrap_or_default();
            let ed = self.editor.as_mut().expect("editor open");
            match input {
                Input::Esc => {
                    if has_files {
                        ed.naming = false;
                        ed.picking = true;
                        ed.name.clear();
                    } else {
                        self.editor = None;
                    }
                }
                Input::Char(c) => ed.name.push(c),
                Input::Backspace => {
                    ed.name.pop();
                }
                Input::Enter => {
                    let result = validate_new_name(&ed.name, &sources);
                    match result {
                        Ok(rel) => {
                            let skel = crate::editor::skeleton_for(&rel);
                            ed.state = EditorState::new(&rel, &skel);
                            ed.state.dirty = true;
                            ed.naming = false;
                            ed.name.clear();
                            ed.diagnostics = validate_edit(&sources, &rel, &skel);
                        }
                        Err(msg) => self.status = msg,
                    }
                }
                _ => {}
            }
            return Action::None;
        }
        if input == Input::Esc {
            let ed = self.editor.as_mut().expect("editor open");
            // Esc inside an engaged picker filter clears the filter first;
            // only a second Esc leaves the picker (F42).
            if ed.picking && ed.filtering {
                ed.filtering = false;
                ed.filter.clear();
                ed.pick_idx = 0;
                return Action::None;
            }
            if ed.state.dirty && !ed.confirm_discard {
                ed.confirm_discard = true;
            } else {
                self.editor = None;
            }
            return Action::None;
        }
        let sources = self
            .current()
            .map(|p| p.sources.clone())
            .unwrap_or_default();
        let ed = self.editor.as_mut().expect("editor open");
        if ed.picking {
            let query = ed.filter.to_lowercase();
            let visible: Vec<usize> = (0..sources.len())
                .filter(|&i| sources[i].0.to_lowercase().contains(&query))
                .collect();
            let nv = visible.len();
            match input {
                Input::Up => ed.pick_idx = ed.pick_idx.saturating_sub(1),
                Input::Down => ed.pick_idx = (ed.pick_idx + 1).min(nv.saturating_sub(1)),
                Input::Enter if nv > 0 => {
                    let (file, text) = sources[visible[ed.pick_idx.min(nv - 1)]].clone();
                    let mut state = EditorState::new(&file, &text);
                    state.move_home();
                    ed.diagnostics = validate_edit(&sources, &file, &text);
                    ed.state = state;
                    ed.picking = false;
                    ed.filtering = false;
                    ed.filter.clear();
                }
                Input::Char('/') if !ed.filtering => ed.filtering = true,
                Input::Char(c) if ed.filtering => {
                    ed.filter.push(c);
                    ed.pick_idx = 0;
                }
                Input::Backspace if ed.filtering => {
                    ed.filter.pop();
                    ed.pick_idx = 0;
                }
                Input::Char('n') => {
                    ed.picking = false;
                    ed.naming = true;
                    ed.name.clear();
                }
                _ => {}
            }
            return Action::None;
        }
        let before = ed.state.text();
        match input {
            Input::Char(c) => ed.state.insert_char(c),
            Input::Enter => ed.state.newline(),
            Input::Backspace => ed.state.backspace(),
            Input::Delete => ed.state.delete(),
            Input::Left => ed.state.move_left(),
            Input::Right => ed.state.move_right(),
            Input::Up => ed.state.move_up(),
            Input::Down => ed.state.move_down(),
            Input::Home => ed.state.move_home(),
            Input::End => ed.state.move_end(),
            Input::PageUp => ed.state.page_up(20),
            Input::PageDown => ed.state.page_down(20),
            // specs are two-space indented; Tab types two spaces
            Input::Tab => {
                ed.state.insert_char(' ');
                ed.state.insert_char(' ');
            }
            _ => {}
        }
        if ed.state.text() != before {
            // any real change disarms a pending discard confirmation
            ed.confirm_discard = false;
            if ed.state.dirty {
                ed.diagnostics =
                    validate_edit(&sources, &ed.state.file, &ed.state.text());
            }
        }
        Action::None
    }

    /// Generated artifacts for the current project, rendered **in memory**
    /// from the live workspace — byte-identical to `decispec gen` output,
    /// plus the on-demand decision index as the last entry. Empty when the
    /// project failed to load.
    pub fn artifact_files(&self) -> Vec<GeneratedFile> {
        let Some(p) = self.current() else {
            return Vec::new();
        };
        let (Some(ws), Some(cfg)) = (p.workspace.as_ref(), p.codegen.as_ref()) else {
            return Vec::new();
        };
        let mut files = render_all(ws, cfg);
        files.push(render_index(ws));
        files
    }

    pub fn update(&mut self, input: Input) -> Action {
        if self.editor.is_some() {
            return self.update_editor(input);
        }
        match input {
            // F43: while the specs-view filter is engaged, printable
            // characters narrow the list instead of firing the command
            // keymap (the same precedence F42 gave the picker); every edit
            // re-anchors the selection. Enter leaves filter-edit mode but
            // keeps the keyword; Esc clears both. Arrows stay inert until
            // Enter — they browse the narrowed list afterwards.
            Input::Char(c) if self.filtering => {
                self.filter.push(c);
                self.decision = 0;
            }
            Input::Backspace if self.filtering => {
                self.filter.pop();
                self.decision = 0;
            }
            Input::Enter if self.filtering => self.filtering = false,
            Input::Esc if self.filtering => {
                self.filter.clear();
                self.filtering = false;
                self.decision = 0;
            }
            Input::Up | Input::Down if self.filtering => {}
            Input::Char('/') if self.mode == Mode::Specs => self.filtering = true,
            Input::Char('q') => return Action::Quit,
            Input::Char('r') => return Action::Reload,
            Input::Char('e') => self.open_picker(),
            Input::Char('p') => {
                if self.current().is_some_and(|p| p.workspace.is_some()) {
                    return Action::CreatePr;
                }
                self.status = "PR needs a loaded project".to_string();
            }
            Input::Char(_) => {}
            Input::Left | Input::Right => {
                let n = self.projects.len();
                if n > 0 {
                    if input == Input::Right {
                        self.project = (self.project + 1).min(n - 1);
                    } else {
                        self.project = self.project.saturating_sub(1);
                    }
                    self.reset_view_state();
                }
            }
            Input::Tab => {
                self.mode = match self.mode {
                    Mode::Specs => Mode::Artifacts,
                    Mode::Artifacts => Mode::Dashboard,
                    Mode::Dashboard => Mode::Specs,
                };
                self.reset_view_state();
            }
            Input::Up | Input::Down => {
                let delta = if input == Input::Down { 1 } else { -1i32 };
                match self.mode {
                    Mode::Specs => {
                        let n = self.visible_decisions().len();
                        if n > 0 {
                            self.decision = (self.decision as i32 + delta).clamp(0, n as i32 - 1) as usize;
                        }
                    }
                    Mode::Artifacts => {
                        let n = self.artifact_files().len();
                        if n > 0 {
                            self.artifact = (self.artifact as i32 + delta).clamp(0, n as i32 - 1) as usize;
                            self.scroll = 0;
                        }
                    }
                    Mode::Dashboard => {}
                }
            }
            Input::Enter => self.scroll = self.scroll.saturating_add(10),
            // keyword kept after Enter: one Esc clears it and re-anchors
            Input::Esc if self.mode == Mode::Specs && !self.filter.is_empty() => {
                self.filter.clear();
                self.decision = 0;
            }
            Input::Esc => self.scroll = 0,
            _ => {}
        }
        Action::None
    }
}

/// Count decisions by status level: [accepted, proposed, rejected, superseded].
pub fn decision_status_counts(ws: &Workspace) -> [usize; 4] {
    let mut counts = [0usize; 4];
    for d in &ws.decisions {
        match d.status {
            Status::Accepted => counts[0] += 1,
            Status::Proposed => counts[1] += 1,
            Status::Rejected => counts[2] += 1,
            Status::Superseded => counts[3] += 1,
        }
    }
    counts
}

/// Validate a typed new-file name (relative to `specs/`). Returns the
/// project-relative path (`specs/<name>`) on success; errors are status-bar
/// messages. A leading `specs/` is accepted and stripped so either
/// `010-x.spec` or `specs/010-x.spec` names the same file.
pub fn validate_new_name(name: &str, existing: &[(String, String)]) -> Result<String, String> {
    let trimmed = name.trim();
    let rel_name = trimmed.strip_prefix("specs/").unwrap_or(trimmed);
    if rel_name.is_empty() {
        return Err("file name is empty".to_string());
    }
    if rel_name.starts_with('/') {
        return Err("use a path relative to specs/".to_string());
    }
    if std::path::Path::new(rel_name)
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("path must not contain ..".to_string());
    }
    if !rel_name.ends_with(".spec") {
        return Err("spec files end in .spec".to_string());
    }
    let rel = format!("specs/{rel_name}");
    if existing.iter().any(|(f, _)| *f == rel) {
        return Err(format!(
            "{rel} already exists — pick it from the list to edit it"
        ));
    }
    Ok(rel)
}

/// How many requirements carry each layer, in `Layer::ALL` order.
pub fn layer_counts(ws: &Workspace) -> Vec<(&'static str, usize)> {
    let mut counts = vec![0usize; Layer::ALL.len()];
    for spec in &ws.specs {
        for req in &spec.requirements {
            for layer in &req.layers {
                if let Some(i) = Layer::ALL.iter().position(|l| l == layer) {
                    counts[i] += 1;
                }
            }
        }
    }
    Layer::ALL
        .iter()
        .map(|l| l.as_str())
        .zip(counts)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::Verdict;

    const FIXTURE_SPEC: &str = r#"
decision AUTH-001 "Auth decision" {
  status: accepted
  context: "needs auth"
  consequences: "+ safe; - latency"
}

decision OLD-1 "Old decision" {
  status: superseded
  superseded_by: AUTH-001
}

model {
  container api "API"
}

spec AUTH-001 {
  requirement AUTH-001-R1 (EARS) {
    text: "WHEN a request arrives THEN it SHALL be authenticated"
    layers: [unit, contract]
    scenarios: [ok]
  }
  scenario ok {
    given: "a request"
    when: "it arrives"
    then: "it is authenticated"
  }
}
"#;

    fn fixture_workspace() -> Workspace {
        let unit = decispec_parse::parse_file("0001.spec", FIXTURE_SPEC).unwrap();
        decispec_parse::merge(vec![unit])
    }

    fn codegen_cfg() -> CodegenConfig {
        CodegenConfig {
            lang: "python".to_string(),
            glue_module: "tests.glue".to_string(),
            rust_glue: "tests/glue/rust".to_string(),
            root_package: String::new(),
            container_packages: Default::default(),
        }
    }

    fn fixture_gate() -> GateSnapshot {
        GateSnapshot {
            project: "demo".to_string(),
            verdict: Verdict::Pass,
            strict_skipped: false,
            counts: crate::snapshot::Counts {
                rows: 2,
                passed: 2,
                failed: 0,
                uncovered: 0,
                skipped: 0,
                no_artifacts: 0,
            },
            failures: vec![],
            warnings: vec![],
            decisions: vec![],
        }
    }

    fn fixture_projects() -> Vec<ProjectData> {
        vec![
            ProjectData::loaded(
                "demo".to_string(),
                PathBuf::from("/tmp/demo"),
                fixture_workspace(),
                vec![("specs/0001.spec".to_string(), FIXTURE_SPEC.to_string())],
                codegen_cfg(),
                Some(fixture_gate()),
            ),
            ProjectData::error(
                "broken".to_string(),
                PathBuf::from("/tmp/broken"),
                "specs/000.spec:3: unknown ref".to_string(),
                vec![],
            ),
        ]
    }

    fn load_editor(app: &mut App) {
        app.update(Input::Char('e'));
        assert!(app.editor.is_some(), "picker opens");
        app.update(Input::Enter);
        let ed = app.editor.as_ref().unwrap();
        assert!(!ed.picking, "enter leaves the picker");
    }

    #[test]
    fn e_opens_the_file_picker() {
        let mut app = App::new(fixture_projects());
        app.update(Input::Char('e'));
        let ed = app.editor.as_ref().expect("editor open");
        assert!(ed.picking);
        assert_eq!(ed.pick_idx, 0);
    }

    #[test]
    fn picker_lists_only_spec_sources() {
        let mut app = App::new(fixture_projects());
        app.update(Input::Char('e'));
        assert_eq!(app.editor.as_ref().unwrap().pick_idx, 0);
        // one source -> Down clamps, Esc closes
        app.update(Input::Down);
        assert_eq!(app.editor.as_ref().unwrap().pick_idx, 0);
        app.update(Input::Esc);
        assert!(app.editor.is_none());
    }

    // -- F42: picker type-to-filter -----------------------------------------

    fn multi_source_app() -> App {
        App::new(vec![ProjectData::loaded(
            "demo".to_string(),
            PathBuf::from("/tmp/demo"),
            fixture_workspace(),
            vec![
                (
                    "specs/0001-auth.spec".to_string(),
                    FIXTURE_SPEC.to_string(),
                ),
                (
                    "specs/0002-user.spec".to_string(),
                    FIXTURE_SPEC.to_string(),
                ),
                (
                    "specs/0003-admin.spec".to_string(),
                    FIXTURE_SPEC.to_string(),
                ),
            ],
            codegen_cfg(),
            Some(fixture_gate()),
        )])
    }

    #[test]
    fn picker_slash_enters_filter_mode_and_typing_narrows_to_the_match() {
        let mut app = multi_source_app();
        app.update(Input::Char('e'));
        app.update(Input::Char('/'));
        assert!(app.editor.as_ref().unwrap().filtering);
        for c in "user".chars() {
            app.update(Input::Char(c));
        }
        let ed = app.editor.as_ref().unwrap();
        assert_eq!(ed.filter, "user");
        assert_eq!(
            app.pickable(),
            vec!["specs/0002-user.spec".to_string()],
            "only the matching source remains visible"
        );
        // 'n' while filtering is a filter char, never the naming shortcut
        app.update(Input::Char('n'));
        let ed = app.editor.as_ref().unwrap();
        assert!(!ed.naming, "'n' must type into the filter");
        assert_eq!(ed.filter, "usern");
        // back it off, then open the highlighted match
        app.update(Input::Backspace);
        assert_eq!(app.pickable(), vec!["specs/0002-user.spec".to_string()]);
        app.update(Input::Enter);
        let ed = app.editor.as_ref().unwrap();
        assert!(!ed.picking);
        assert_eq!(ed.state.file, "specs/0002-user.spec");
        assert!(ed.filter.is_empty(), "opening a file clears the filter");
    }

    #[test]
    fn picker_filter_is_case_insensitive_and_selection_anchors_to_top() {
        let mut app = multi_source_app();
        app.update(Input::Char('e'));
        app.update(Input::Char('/'));
        for c in "ADMIN".chars() {
            app.update(Input::Char(c));
        }
        assert_eq!(
            app.pickable(),
            vec!["specs/0003-admin.spec".to_string()],
            "filter matches case-insensitively"
        );
        // widen back to the full list, move down, then edit: selection
        // re-anchors at 0 and the list narrows again
        for _ in 0.."ADMIN".len() {
            app.update(Input::Backspace);
        }
        assert_eq!(app.pickable().len(), 3, "clearing the filter shows all");
        app.update(Input::Down);
        assert_eq!(app.editor.as_ref().unwrap().pick_idx, 1);
        app.update(Input::Char('d'));
        assert_eq!(
            app.editor.as_ref().unwrap().pick_idx,
            0,
            "any filter edit re-anchors the selection"
        );
        assert_eq!(
            app.pickable(),
            vec!["specs/0003-admin.spec".to_string()],
            "only the file containing 'd' remains"
        );
    }

    #[test]
    fn picker_esc_clears_filter_then_closes_and_n_still_names() {
        let mut app = multi_source_app();
        app.update(Input::Char('e'));
        app.update(Input::Char('/'));
        for c in "auth".chars() {
            app.update(Input::Char(c));
        }
        assert_eq!(app.pickable().len(), 1);
        // first Esc: leave filter mode, clear the filter, stay in picker
        app.update(Input::Esc);
        let ed = app.editor.as_ref().unwrap();
        assert!(ed.picking, "still in the picker");
        assert!(!ed.filtering);
        assert!(ed.filter.is_empty(), "filter cleared");
        assert_eq!(app.pickable().len(), 3, "full list is back");
        // second Esc: close the picker (pre-F42 behavior)
        app.update(Input::Esc);
        assert!(app.editor.is_none());
        // 'n' still opens naming when no filter is engaged
        let mut app = multi_source_app();
        app.update(Input::Char('e'));
        app.update(Input::Char('n'));
        assert!(app.editor.as_ref().unwrap().naming);
    }

    // -- F43: specs-view decision filter -------------------------------------

    #[test]
    fn tui_slash_engages_decision_filter_and_typing_narrows() {
        let mut app = App::new(fixture_projects());
        app.update(Input::Char('/'));
        assert!(app.filtering, "'/' engages filter-edit mode in the specs view");
        for c in "auth".chars() {
            app.update(Input::Char(c));
        }
        assert_eq!(app.filter, "auth");
        let ids: Vec<&str> = app
            .visible_decisions()
            .iter()
            .map(|d| d.id.as_str())
            .collect();
        assert_eq!(ids, vec!["AUTH-001"], "only the match stays visible");
        // while engaged, command letters type into the filter instead of firing
        let action = app.update(Input::Char('e'));
        assert_eq!(action, Action::None, "'e' must not open the editor");
        assert!(app.editor.is_none());
        assert_eq!(app.filter, "authe");
        assert!(
            app.visible_decisions().is_empty(),
            "no decision matches 'authe'"
        );
        // Backspace edits; Enter leaves filter-edit mode but keeps the keyword
        app.update(Input::Backspace);
        assert_eq!(app.visible_decisions().len(), 1);
        app.update(Input::Enter);
        assert!(!app.filtering, "enter leaves filter-edit mode");
        assert_eq!(app.filter, "auth", "enter keeps the keyword for browsing");
        assert_eq!(app.visible_decisions().len(), 1);
        // arrows browse the narrowed list; '/' re-engages editing
        app.update(Input::Down);
        assert_eq!(app.decision, 0, "one row: down clamps in place");
        app.update(Input::Char('/'));
        assert!(app.filtering, "'/' re-engages with the keyword kept");
        assert_eq!(app.filter, "auth");
    }

    #[test]
    fn tui_decision_filter_is_case_insensitive_and_reanchors_selection() {
        let mut app = App::new(fixture_projects());
        app.update(Input::Char('/'));
        for c in "AUTH".chars() {
            app.update(Input::Char(c));
        }
        let ids: Vec<&str> = app
            .visible_decisions()
            .iter()
            .map(|d| d.id.as_str())
            .collect();
        assert_eq!(ids, vec!["AUTH-001"], "matching is case-insensitive");
        // the query matches across the id + " " + title concatenation
        for _ in 0.."AUTH".len() {
            app.update(Input::Backspace);
        }
        for c in "1 old".chars() {
            app.update(Input::Char(c));
        }
        let ids: Vec<&str> = app
            .visible_decisions()
            .iter()
            .map(|d| d.id.as_str())
            .collect();
        assert_eq!(
            ids,
            vec!["OLD-1"],
            "'1 old' matches only the concatenated id+title of OLD-1"
        );
        // clearing re-shows everything; leaving filter-edit mode lets the
        // arrows browse, and the next edit re-anchors to the top
        for _ in 0.."1 old".len() {
            app.update(Input::Backspace);
        }
        assert_eq!(app.visible_decisions().len(), 2, "empty query shows all");
        app.update(Input::Enter);
        app.update(Input::Down);
        assert_eq!(app.decision, 1);
        app.update(Input::Char('/'));
        app.update(Input::Char('l'));
        assert_eq!(
            app.decision, 0,
            "every filter edit re-anchors the selection"
        );
        let ids: Vec<&str> = app
            .visible_decisions()
            .iter()
            .map(|d| d.id.as_str())
            .collect();
        assert_eq!(ids, vec!["OLD-1"], "'l' narrows to the title/id match");
    }

    #[test]
    fn tui_esc_clears_decision_filter_and_normal_keys_resume() {
        let mut app = App::new(fixture_projects());
        app.update(Input::Char('/'));
        for c in "old".chars() {
            app.update(Input::Char(c));
        }
        assert_eq!(app.visible_decisions().len(), 1);
        app.update(Input::Esc);
        assert!(!app.filtering, "esc exits filter mode");
        assert!(app.filter.is_empty(), "esc clears the keyword");
        assert_eq!(app.visible_decisions().len(), 2, "full list is back");
        assert_eq!(app.decision, 0);
        // normal keys resume once the filter is gone
        assert_eq!(app.update(Input::Char('q')), Action::Quit);

        // a keyword kept by Enter is still cleared by a later Esc
        let mut app = App::new(fixture_projects());
        app.update(Input::Char('/'));
        for c in "old".chars() {
            app.update(Input::Char(c));
        }
        app.update(Input::Enter);
        assert!(!app.filtering && !app.filter.is_empty());
        app.update(Input::Esc);
        assert!(app.filter.is_empty(), "esc clears a kept keyword too");
        assert_eq!(app.visible_decisions().len(), 2);

        // empty filter + not engaged: Esc keeps its scroll-reset behavior
        let mut app = App::new(fixture_projects());
        app.update(Input::Char('/'));
        app.update(Input::Esc);
        assert!(!app.filtering, "esc disengages an empty filter");
        app.update(Input::Enter);
        assert!(app.scroll > 0);
        app.update(Input::Esc);
        assert_eq!(app.scroll, 0, "esc still returns the detail pane to top");
    }

    #[test]
    fn tui_switching_project_resets_decision_filter() {
        let mut app = App::new(fixture_projects());
        app.update(Input::Char('/'));
        for c in "auth".chars() {
            app.update(Input::Char(c));
        }
        assert!(app.filtering);
        assert_eq!(app.filter, "auth");
        app.update(Input::Right);
        assert!(!app.filtering, "project switch exits filter mode");
        assert!(app.filter.is_empty(), "project switch clears the keyword");
        assert_eq!(app.decision, 0);
        assert_eq!(app.scroll, 0);
        assert!(
            app.visible_decisions().is_empty(),
            "the broken project has no decisions"
        );
        // and back: the demo project's full list is shown again
        app.update(Input::Left);
        assert_eq!(app.visible_decisions().len(), 2);
    }

    #[test]
    fn tui_tab_resets_decision_filter_with_the_rest_of_the_view() {
        let mut app = App::new(fixture_projects());
        app.update(Input::Char('/'));
        for c in "auth".chars() {
            app.update(Input::Char(c));
        }
        app.update(Input::Tab);
        assert_eq!(app.mode, Mode::Artifacts);
        assert!(!app.filtering, "leaving the view exits filter mode");
        assert!(app.filter.is_empty(), "leaving the view clears the keyword");
    }

    #[test]
    fn e_on_project_without_specs_opens_naming_directly() {
        let mut app = App::new(fixture_projects());
        app.update(Input::Right); // broken project: zero sources
        app.update(Input::Char('e'));
        let ed = app.editor.as_ref().expect("editor open");
        assert!(ed.naming, "no spec files -> straight to the name prompt");
        assert!(!ed.picking);
    }

    #[test]
    fn picker_n_opens_naming_and_esc_returns_to_picker() {
        let mut app = App::new(fixture_projects());
        app.update(Input::Char('e'));
        app.update(Input::Char('n'));
        assert!(app.editor.as_ref().unwrap().naming);
        app.update(Input::Esc);
        let ed = app.editor.as_ref().unwrap();
        assert!(ed.picking, "esc from naming returns to the file picker");
        assert!(!ed.naming);
    }

    #[test]
    fn esc_in_naming_with_no_files_closes_the_editor() {
        let mut app = App::new(fixture_projects());
        app.update(Input::Right);
        app.update(Input::Char('e')); // naming, no picker to return to
        app.update(Input::Esc);
        assert!(app.editor.is_none());
    }

    #[test]
    fn naming_enter_opens_editor_on_parseable_skeleton() {
        let mut app = App::new(fixture_projects());
        app.update(Input::Char('e'));
        app.update(Input::Char('n'));
        for c in "010-new.spec".chars() {
            app.update(Input::Char(c));
        }
        app.update(Input::Enter);
        let ed = app.editor.as_ref().unwrap();
        assert!(!ed.picking && !ed.naming);
        assert_eq!(ed.state.file, "specs/010-new.spec");
        assert!(ed.state.dirty, "skeleton starts dirty so Ctrl+S writes it");
        assert!(ed.state.text().contains("decision PREFIX-001"));
        assert!(ed.diagnostics.is_empty(), "diags: {:?}", ed.diagnostics);
    }

    #[test]
    fn naming_enter_refuses_invalid_names_and_stays_in_naming() {
        let mut app = App::new(fixture_projects());
        app.update(Input::Char('e'));
        app.update(Input::Char('n'));
        // missing .spec extension
        for c in "010-new".chars() {
            app.update(Input::Char(c));
        }
        app.update(Input::Enter);
        assert!(app.editor.as_ref().unwrap().naming, "stays in naming");
        assert!(app.status.contains(".spec"), "status: {}", app.status);
        // traversal is refused
        for _ in 0.."010-new".len() {
            app.update(Input::Backspace);
        }
        for c in "../evil.spec".chars() {
            app.update(Input::Char(c));
        }
        app.update(Input::Enter);
        assert!(app.status.contains(".."), "status: {}", app.status);
        // collision with an existing spec file
        let mut app = App::new(fixture_projects());
        app.update(Input::Char('e'));
        app.update(Input::Char('n'));
        for c in "0001.spec".chars() {
            app.update(Input::Char(c));
        }
        app.update(Input::Enter);
        assert!(app.status.contains("already exists"), "status: {}", app.status);
        assert!(app.editor.as_ref().unwrap().naming);
    }

    #[test]
    fn enter_loads_file_text_clean() {
        let mut app = App::new(fixture_projects());
        load_editor(&mut app);
        let ed = app.editor.as_ref().unwrap();
        assert_eq!(ed.state.file, "specs/0001.spec");
        assert_eq!(ed.state.text(), FIXTURE_SPEC);
        assert!(!ed.state.dirty);
        assert!(ed.diagnostics.is_empty(), "diags: {:?}", ed.diagnostics);
    }

    #[test]
    fn picker_shows_live_diagnostics_for_broken_files() {
        let broken = FIXTURE_SPEC.replace("scenarios: [ok]", "scenarios: [ok, nope]");
        let mut projects = fixture_projects();
        projects[0].sources = vec![("specs/0001.spec".to_string(), broken)];
        let mut app = App::new(projects);
        load_editor(&mut app);
        let ed = app.editor.as_ref().unwrap();
        assert!(
            ed.diagnostics.iter().any(|d| d.contains("nope")),
            "diags: {:?}",
            ed.diagnostics
        );
    }

    #[test]
    fn typing_marks_dirty_recomputes_diagnostics_and_cannot_quit() {
        let mut app = App::new(fixture_projects());
        load_editor(&mut app);
        // 'q' inserts — it must NOT quit while the editor is open
        let action = app.update(Input::Char('q'));
        assert_eq!(action, Action::None);
        let ed = app.editor.as_ref().unwrap();
        assert!(ed.state.dirty);
        assert!(!ed.diagnostics.is_empty(), "diags: {:?}", ed.diagnostics);
        assert!(ed.state.text().starts_with('q'));
    }

    #[test]
    fn ctrl_s_saves_only_a_dirty_buffer() {
        let mut app = App::new(fixture_projects());
        load_editor(&mut app);
        assert_eq!(app.update(Input::Ctrl('s')), Action::None);
        app.update(Input::Char('x'));
        match app.update(Input::Ctrl('s')) {
            Action::SaveSpec { file, text } => {
                assert_eq!(file, "specs/0001.spec");
                assert!(text.starts_with('x'));
            }
            other => panic!("expected SaveSpec, got {other:?}"),
        }
    }

    #[test]
    fn esc_confirms_before_discarding_dirty_buffer() {
        let mut app = App::new(fixture_projects());
        load_editor(&mut app);
        // clean buffer: Esc closes immediately
        app.update(Input::Esc);
        assert!(app.editor.is_none());
        // dirty buffer: first Esc arms the confirmation, second discards
        load_editor(&mut app);
        app.update(Input::Char('x'));
        app.update(Input::Esc);
        assert!(app.editor.is_some(), "first Esc only confirms");
        assert!(app.editor.as_ref().unwrap().confirm_discard);
        app.update(Input::Esc);
        assert!(app.editor.is_none(), "second Esc discards");
    }

    #[test]
    fn editing_after_confirm_disarms_it() {
        let mut app = App::new(fixture_projects());
        load_editor(&mut app);
        app.update(Input::Char('x'));
        app.update(Input::Esc);
        assert!(app.editor.as_ref().unwrap().confirm_discard);
        // a real change re-arms the safety: the next Esc must NOT discard
        app.update(Input::Char('y'));
        assert!(!app.editor.as_ref().unwrap().confirm_discard);
        app.update(Input::Esc);
        assert!(
            app.editor.is_some(),
            "editor must survive one Esc after the edit"
        );
    }

    #[test]
    fn mark_saved_clears_dirty() {
        let mut app = App::new(fixture_projects());
        load_editor(&mut app);
        app.update(Input::Char('x'));
        assert!(app.editor.as_ref().unwrap().state.dirty);
        app.mark_saved();
        let ed = app.editor.as_ref().unwrap();
        assert!(!ed.state.dirty);
        assert!(!ed.confirm_discard);
    }

    #[test]
    fn p_requests_a_pr_for_a_loaded_project() {
        let mut app = App::new(fixture_projects());
        assert_eq!(app.update(Input::Char('p')), Action::CreatePr);
        app.update(Input::Right); // broken project: no workspace
        let action = app.update(Input::Char('p'));
        assert_eq!(action, Action::None);
        assert!(app.status.contains("PR needs"), "status: {}", app.status);
    }

    #[test]
    fn new_app_starts_on_first_project_in_specs_mode() {
        let app = App::new(fixture_projects());
        assert_eq!(app.projects.len(), 2);
        assert_eq!(app.project, 0);
        assert_eq!(app.mode, Mode::Specs);
        assert!(app.status.contains("2 project(s)"));
    }

    #[test]
    fn empty_registry_says_so_in_status() {
        let app = App::new(vec![]);
        assert!(app.status.contains("no projects registered"));
    }

    #[test]
    fn right_and_left_switch_projects_and_clamp() {
        let mut app = App::new(fixture_projects());
        assert_eq!(app.update(Input::Right), Action::None);
        assert_eq!(app.project, 1);
        // clamp at the end
        assert_eq!(app.update(Input::Right), Action::None);
        assert_eq!(app.project, 1);
        assert_eq!(app.update(Input::Left), Action::None);
        assert_eq!(app.project, 0);
        // clamp at the start
        assert_eq!(app.update(Input::Left), Action::None);
        assert_eq!(app.project, 0);
    }

    #[test]
    fn switching_project_resets_selection_and_scroll() {
        let mut app = App::new(fixture_projects());
        app.decision = 1;
        app.scroll = 5;
        app.update(Input::Right);
        assert_eq!((app.decision, app.scroll), (0, 0));
    }

    #[test]
    fn tab_cycles_modes() {
        let mut app = App::new(fixture_projects());
        app.update(Input::Tab);
        assert_eq!(app.mode, Mode::Artifacts);
        app.update(Input::Tab);
        assert_eq!(app.mode, Mode::Dashboard);
        app.update(Input::Tab);
        assert_eq!(app.mode, Mode::Specs);
    }

    #[test]
    fn down_clamps_to_decision_count() {
        let mut app = App::new(fixture_projects());
        app.update(Input::Down);
        assert_eq!(app.decision, 1);
        app.update(Input::Down); // only 2 decisions -> clamps
        assert_eq!(app.decision, 1);
        app.update(Input::Up);
        assert_eq!(app.decision, 0);
    }

    #[test]
    fn artifact_files_render_in_memory_with_index_last() {
        let app = App::new(fixture_projects());
        let files = app.artifact_files();
        assert!(!files.is_empty());
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&".decispec/diagrams/model.mmd"), "paths: {paths:?}");
        assert!(
            paths
                .iter()
                .any(|p| p.starts_with("tests/generated/unit/test_AUTH_001_ok")),
            "paths: {paths:?}"
        );
        // the on-demand decision index rides along as the final entry
        assert_eq!(paths.last(), Some(&".decispec/docs/decisions.md"));
        // in-memory only: content is real codegen output
        let mmd = files
            .iter()
            .find(|f| f.path == ".decispec/diagrams/model.mmd")
            .unwrap();
        assert!(mmd.content.contains("flowchart"));
    }

    #[test]
    fn artifact_files_empty_for_broken_project() {
        let mut app = App::new(fixture_projects());
        app.update(Input::Right);
        assert!(app.artifact_files().is_empty());
    }

    #[test]
    fn q_quits_r_reloads_other_chars_do_nothing() {
        let mut app = App::new(fixture_projects());
        assert_eq!(app.update(Input::Char('q')), Action::Quit);
        assert_eq!(app.update(Input::Char('r')), Action::Reload);
        assert_eq!(app.update(Input::Char('x')), Action::None);
        assert_eq!(app.update(Input::Enter), Action::None);
        assert_eq!(app.update(Input::Esc), Action::None);
        assert_eq!(app.scroll, 0);
    }

    #[test]
    fn enter_pages_scroll_and_esc_resets() {
        let mut app = App::new(fixture_projects());
        app.mode = Mode::Artifacts;
        app.update(Input::Enter);
        assert!(app.scroll > 0);
        app.update(Input::Esc);
        assert_eq!(app.scroll, 0);
    }

    #[test]
    fn status_counts_by_decision_level() {
        let ws = fixture_workspace();
        assert_eq!(decision_status_counts(&ws), [1, 0, 0, 1]);
    }

    #[test]
    fn layer_counts_requirements_per_layer() {
        let ws = fixture_workspace();
        let counts = layer_counts(&ws);
        assert_eq!(
            counts,
            vec![("unit", 1), ("contract", 1), ("e2e", 0), ("infra", 0), ("fitness", 0)]
        );
    }
}
