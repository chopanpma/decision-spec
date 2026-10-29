//! Widgets. Every function renders from `&App` into the frame — no I/O, no
//! globals; tested with ratatui's `TestBackend` via buffer assertions.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};

use decispec_gate::coverage::{Coverage, ProofState};
use decispec_ir::{Decision, Status, Workspace};

use crate::app::{App, Mode, ProjectData, decision_status_counts, layer_counts};
use crate::snapshot::{GateSnapshot, Verdict};

fn status_style(status: Status) -> Style {
    match status {
        Status::Accepted => Style::default().fg(Color::Green),
        Status::Proposed => Style::default().fg(Color::Yellow),
        Status::Superseded => Style::default().fg(Color::DarkGray),
        Status::Rejected => Style::default().fg(Color::Red),
    }
}

fn proof_style(state: ProofState) -> Style {
    match state {
        ProofState::Proven => Style::default().fg(Color::Green),
        ProofState::Partial => Style::default().fg(Color::Yellow),
        ProofState::Unproven => Style::default().fg(Color::DarkGray),
    }
}

/// The per-decision mark: glyph + label in the buffer text, never color-only.
fn proof_mark_span(state: ProofState) -> Span<'static> {
    Span::styled(
        format!("{} {} ", state.symbol(), state.as_str()),
        proof_style(state),
    )
}

/// The mark column for one spec-list row. Gatable decisions join to the
/// report's rollup by id; a gatable decision absent from the report has no
/// passing rows on record, so it is honestly unproven. Superseded/rejected
/// decisions are outside coverage and get a dim placeholder for alignment.
fn proof_span(coverage: Option<&Coverage>, id: &str, status: Status) -> Span<'static> {
    match coverage.and_then(|cov| cov.decisions.iter().find(|d| d.id == id)) {
        Some(d) => proof_mark_span(d.state),
        None if matches!(status, Status::Accepted | Status::Proposed) => {
            proof_mark_span(ProofState::Unproven)
        }
        None => Span::styled("— ", Style::default().fg(Color::DarkGray)),
    }
}

fn status_str(status: Status) -> &'static str {
    status.as_str()
}

fn verdict_style(verdict: Verdict) -> Style {
    match verdict {
        Verdict::Pass => Style::default().fg(Color::Green),
        Verdict::Fail => Style::default().fg(Color::Red),
    }
}

/// Headline emphasis: fully proven reads green, otherwise yellow; the numbers
/// themselves are the message, so it is always bold.
fn headline_style(cov: &Coverage) -> Style {
    let style = Style::default().add_modifier(Modifier::BOLD);
    if cov.gatable > 0 && cov.proven == cov.gatable {
        style.fg(Color::Green)
    } else {
        style.fg(Color::Yellow)
    }
}

/// `N/M decisions proven (P%)`, with P rounded to the nearest percent.
fn headline_text(cov: &Coverage) -> String {
    format!("{}/{} decisions proven ({}%)", cov.proven, cov.gatable, cov.percent_nearest())
}

fn truncate(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        s.to_string()
    } else {
        let mut out: String = chars[..max.saturating_sub(1)].iter().collect();
        out.push('…');
        out
    }
}

const KEY_HINTS: &str = "←→ project  ⇥ view  ↑↓ select  ⏎ page  esc top  r reload  e edit  p PR  q quit";
const EDITOR_HINTS: &str = "Ctrl+S save  esc close  ↑↓←→ move  home/end  pgup/pgdn  type to edit";
const PICKER_HINTS: &str = "↑↓ choose  ⏎ open  esc close";

pub fn render(app: &App, frame: &mut Frame) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(22), Constraint::Min(0)])
        .split(frame.area());
    render_sidebar(app, frame, cols[0]);
    let main = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(cols[1]);
    if app.editor.is_some() {
        render_editor(app, frame, main[0]);
    } else {
        match app.mode {
            Mode::Specs => render_specs(app, frame, main[0]),
            Mode::Artifacts => render_artifacts(app, frame, main[0]),
            Mode::Dashboard => render_dashboard(app, frame, main[0]),
        }
    }
    render_status_line(app, frame, main[1]);
}

fn render_no_projects(app: &App, frame: &mut Frame, area: Rect) {
    let para = Paragraph::new(Line::from(Span::styled(
        app.status.clone(),
        Style::default().fg(Color::Yellow),
    )))
    .block(Block::default().borders(Borders::ALL).title("decispec tui"));
    frame.render_widget(para, area);
}

fn render_sidebar(app: &App, frame: &mut Frame, area: Rect) {
    let items: Vec<ListItem> = app
        .projects
        .iter()
        .map(|p| {
            let badge = match &p.gate {
                Some(g) => Span::styled(" ●", verdict_style(g.verdict)),
                None => Span::styled(" ●", Style::default().fg(Color::DarkGray)),
            };
            let style = if p.load_error.is_some() {
                Style::default().fg(Color::Red)
            } else {
                Style::default()
            };
            let percent = match &p.coverage {
                Some(cov) => Span::styled(
                    format!(" {}%", cov.percent_nearest()),
                    headline_style(cov),
                ),
                None => Span::raw(""),
            };
            ListItem::new(Line::from(vec![
                Span::styled(truncate(&p.name, 12), style),
                percent,
                badge,
            ]))
        })
        .collect();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("projects"))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD).bg(Color::DarkGray));
    let mut state = ListState::default();
    if !app.projects.is_empty() {
        state.select(Some(app.project.min(app.projects.len() - 1)));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

fn error_pane(p: &ProjectData) -> Paragraph<'_> {
    let message = p
        .load_error
        .clone()
        .unwrap_or_else(|| "workspace unavailable".to_string());
    Paragraph::new(vec![
        Line::from(vec![Span::styled(
            format!("{} failed to load", p.name),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        Line::from(Span::styled(message, Style::default().fg(Color::Red))),
    ])
    .block(Block::default().borders(Borders::ALL).title("load error"))
    .wrap(Wrap { trim: true })
}

fn decision_detail_lines(ws: &Workspace, d: &Decision) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                format!("{} — ", d.id),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(d.title.clone(), Style::default().add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::raw("status: "),
            Span::styled(status_str(d.status), status_style(d.status)),
        ]),
    ];
    if let Some(ctx) = &d.context {
        lines.push(Line::from(format!("context: {ctx}")));
    }
    if let Some(cons) = &d.consequences {
        lines.push(Line::from(format!("consequences: {cons}")));
    }
    if let Some(by) = &d.superseded_by {
        lines.push(Line::from(format!("superseded_by: {by}")));
    }
    lines.push(Line::from(format!("defined at: {}:{}", d.src.file, d.src.line)));
    lines.push(Line::from(""));

    if let Some(sp) = ws.spec_for_decision(&d.id) {
        if !sp.requirements.is_empty() {
            lines.push(Line::from(Span::styled(
                "requirements:",
                Style::default().add_modifier(Modifier::BOLD),
            )));
        }
        for req in &sp.requirements {
            let layers: Vec<&str> = req.layers.iter().map(|l| l.as_str()).collect();
            let style = req
                .style
                .as_ref()
                .map(|s| format!(" ({s})"))
                .unwrap_or_default();
            lines.push(Line::from(vec![
                Span::styled(format!("  {}{}", req.id, style), Style::default().fg(Color::Cyan)),
                Span::raw(format!(" [{}]", layers.join(", "))),
            ]));
            lines.push(Line::from(format!("    {}", req.text)));
            for scen_id in &req.scenarios {
                let Some(scen) = sp.scenario(scen_id) else { continue };
                let scen_layers: Vec<&str> =
                    sp.layers_for_scenario(scen_id).iter().map(|l| l.as_str()).collect();
                lines.push(Line::from(vec![
                    Span::styled(format!("    scenario {}", scen.id), Style::default().fg(Color::Blue)),
                    Span::raw(format!(" [{}]", scen_layers.join(", "))),
                ]));
                lines.push(Line::from(format!(
                    "      given {} | when {} | then {}",
                    scen.given, scen.when, scen.then
                )));
            }
        }
        for inv in &sp.invariants {
            let layers: Vec<&str> = inv.layers.iter().map(|l| l.as_str()).collect();
            lines.push(Line::from(vec![
                Span::raw("invariant "),
                Span::styled(inv.id.clone(), Style::default().fg(Color::Cyan)),
                Span::raw(format!(" [{}] {}", layers.join(", "), inv.expr)),
            ]));
        }
        for pol in &sp.infra_policies {
            let layers: Vec<&str> = pol.layers.iter().map(|l| l.as_str()).collect();
            lines.push(Line::from(vec![
                Span::raw("infra_policy "),
                Span::styled(pol.id.clone(), Style::default().fg(Color::Cyan)),
                Span::raw(format!(" [{}] {}", layers.join(", "), pol.description)),
            ]));
        }
    }
    lines
}

fn render_specs(app: &App, frame: &mut Frame, area: Rect) {
    let Some(p) = app.current() else {
        render_no_projects(app, frame, area);
        return;
    };
    let Some(ws) = p.workspace.as_ref() else {
        frame.render_widget(error_pane(p), area);
        return;
    };
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(32), Constraint::Min(0)])
        .split(area);
    let visible = app.visible_decisions();
    // filter line rides above the list while a filter is engaged or kept;
    // with no filter at all the layout is untouched (F43)
    let list_area = if app.filtering || !app.filter.is_empty() {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(cols[0]);
        let line = Line::from(Span::styled(
            format!("filter: {}   {}/{}", app.filter, visible.len(), ws.decisions.len()),
            Style::default().fg(Color::Cyan),
        ));
        frame.render_widget(Paragraph::new(line), rows[0]);
        rows[1]
    } else {
        cols[0]
    };
    let items: Vec<ListItem> = visible
        .iter()
        .map(|d| {
            ListItem::new(Line::from(vec![
                Span::styled(format!("{} ", d.id), status_style(d.status)),
                proof_span(p.coverage.as_ref(), &d.id, d.status),
                Span::raw(truncate(&d.title, 26)),
            ]))
        })
        .collect();
    let list_title = if p.coverage.is_some() {
        "decisions"
    } else {
        // No report: say so in the chrome instead of inventing marks.
        "decisions — no gate run: no proof marks"
    };
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(list_title))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD).bg(Color::DarkGray));
    let mut state = ListState::default();
    if !visible.is_empty() {
        state.select(Some(app.decision.min(visible.len() - 1)));
    }
    frame.render_stateful_widget(list, list_area, &mut state);

    let detail = match visible.get(app.decision.min(visible.len().saturating_sub(1))) {
        Some(d) => decision_detail_lines(ws, d),
        None => vec![Line::from("no decisions")],
    };
    let para = Paragraph::new(detail)
        .block(Block::default().borders(Borders::ALL).title("decision"))
        .wrap(Wrap { trim: true })
        .scroll((app.scroll, 0));
    frame.render_widget(para, cols[1]);
}

fn render_artifacts(app: &App, frame: &mut Frame, area: Rect) {
    let Some(p) = app.current() else {
        render_no_projects(app, frame, area);
        return;
    };
    if p.workspace.is_none() {
        frame.render_widget(error_pane(p), area);
        return;
    }
    let files = app.artifact_files();
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(34), Constraint::Min(0)])
        .split(area);
    let items: Vec<ListItem> = files
        .iter()
        .map(|f| ListItem::new(f.path.clone()))
        .collect();
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("generated (in memory — never written)"),
        )
        .highlight_style(Style::default().add_modifier(Modifier::BOLD).bg(Color::DarkGray));
    let mut state = ListState::default();
    if !files.is_empty() {
        state.select(Some(app.artifact.min(files.len() - 1)));
    }
    frame.render_stateful_widget(list, cols[0], &mut state);

    let idx = app.artifact.min(files.len().saturating_sub(1));
    let (title, content): (String, String) = files
        .get(idx)
        .map(|f| (f.path.clone(), f.content.clone()))
        .unwrap_or_else(|| ("no artifacts".to_string(), String::new()));
    let para = Paragraph::new(content)
        .block(Block::default().borders(Borders::ALL).title(title))
        .wrap(Wrap { trim: false })
        .scroll((app.scroll, 0));
    frame.render_widget(para, cols[1]);
}

fn render_dashboard(app: &App, frame: &mut Frame, area: Rect) {
    let Some(p) = app.current() else {
        render_no_projects(app, frame, area);
        return;
    };
    let Some(ws) = p.workspace.as_ref() else {
        frame.render_widget(error_pane(p), area);
        return;
    };
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),
            Constraint::Length(10),
            Constraint::Min(8),
        ])
        .split(area);

    // Band 1: decision status levels — what is active on this project.
    let [accepted, proposed, rejected, superseded] = decision_status_counts(ws);
    let active = accepted + proposed;
    let levels = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("accepted ", status_style(Status::Accepted)),
            Span::raw(format!("{accepted}   ")),
            Span::styled("proposed ", status_style(Status::Proposed)),
            Span::raw(format!("{proposed}   ")),
            Span::styled("rejected ", status_style(Status::Rejected)),
            Span::raw(format!("{rejected}   ")),
            Span::styled("superseded ", status_style(Status::Superseded)),
            Span::raw(superseded.to_string()),
        ]),
        Line::from(vec![
            Span::styled(
                format!("active (accepted + proposed): {active}"),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!("  of {} decision(s)", ws.decisions.len())),
        ]),
    ])
    .block(Block::default().borders(Borders::ALL).title("decision levels"));
    frame.render_widget(levels, rows[0]);

    // Band 2: requirement layers.
    let mut layer_lines = Vec::new();
    for (name, count) in layer_counts(ws) {
        layer_lines.push(Line::from(vec![
            Span::styled(format!("{name:<10}"), Style::default().fg(Color::Cyan)),
            Span::raw(format!("{count:>3} requirement(s)")),
        ]));
    }
    let layers = Paragraph::new(layer_lines)
        .block(Block::default().borders(Borders::ALL).title("requirement layers"));
    frame.render_widget(layers, rows[1]);

    // Band 3: last gate run, with the proof-coverage headline beside the
    // verdict. No report -> an explicit no-gate-run marker, never a fake 0%.
    let gate_lines: Vec<Line> = match (&p.gate, &p.coverage) {
        (Some(g), Some(cov)) => gate_lines(g, cov),
        _ => vec![
            Line::from(Span::styled(
                "no gate report yet — run `decispec gate` in the project",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "coverage: no gate run",
                Style::default().fg(Color::DarkGray),
            )),
        ],
    };
    let gate = Paragraph::new(gate_lines)
        .block(Block::default().borders(Borders::ALL).title("last gate"))
        .wrap(Wrap { trim: true })
        .scroll((app.scroll, 0));
    frame.render_widget(gate, rows[2]);
}

fn gate_lines(g: &GateSnapshot, cov: &Coverage) -> Vec<Line<'static>> {
    let c = &g.counts;
    let mut lines = vec![
        Line::from(vec![
            Span::raw("verdict: "),
            Span::styled(
                if g.verdict == Verdict::Pass { "PASS" } else { "FAIL" },
                verdict_style(g.verdict).add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!("  — {} ({} rows: {} passed, {} failed, {} uncovered, {} skipped, {} no-artifacts)",
                g.project, c.rows, c.passed, c.failed, c.uncovered, c.skipped, c.no_artifacts)),
        ]),
        Line::from(vec![
            Span::raw("coverage: "),
            Span::styled(
                headline_text(cov),
                headline_style(cov),
            ),
        ]),
        Line::from(""),
    ];
    if g.strict_skipped {
        lines.push(Line::from(Span::styled(
            "strict-skipped: SKIPPED rows escalated to FAIL",
            Style::default().fg(Color::Yellow),
        )));
    }
    if !g.failures.is_empty() {
        lines.push(Line::from(Span::styled(
            "failures:",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )));
        for f in g.failures.iter().take(6) {
            lines.push(Line::from(Span::styled(
                format!("  ✗ {f}"),
                Style::default().fg(Color::Red),
            )));
        }
    }
    if !g.warnings.is_empty() {
        lines.push(Line::from(Span::styled(
            "warnings:",
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        )));
        for w in g.warnings.iter().take(6) {
            lines.push(Line::from(Span::styled(
                format!("  ! {w}"),
                Style::default().fg(Color::Yellow),
            )));
        }
    }
    lines
}

fn render_status_line(app: &App, frame: &mut Frame, area: Rect) {
    let hints = if app.editor.is_some() {
        if app.editor.as_ref().is_some_and(|e| e.picking) {
            PICKER_HINTS
        } else {
            EDITOR_HINTS
        }
    } else {
        KEY_HINTS
    };
    let mut spans = vec![
        Span::styled(
            format!(" [{}] ", app.mode.as_str()),
            Style::default().fg(Color::Black).bg(Color::Cyan),
        ),
        Span::raw(format!(" {hints} ")),
    ];
    if let Some(p) = app.current() {
        spans.push(Span::styled(
            format!(" {} ", p.path.display()),
            Style::default().fg(Color::DarkGray),
        ));
    }
    if !app.status.is_empty() {
        spans.push(Span::styled(
            format!(" {} ", app.status),
            Style::default().fg(Color::Yellow),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_editor(app: &App, frame: &mut Frame, area: Rect) {
    let Some(ed) = app.editor.as_ref() else {
        return;
    };
    let Some(p) = app.current() else {
        render_no_projects(app, frame, area);
        return;
    };
    if ed.picking {
        let visible = app.pickable();
        let items: Vec<ListItem> = visible.iter().map(|f| ListItem::new(f.clone())).collect();
        let title = if ed.filter.is_empty() {
            "edit which file? (n = new, / = filter)".to_string()
        } else {
            format!(
                "edit which file? (n = new, / = filter)  filter: /{} ({}/{})",
                ed.filter,
                visible.len(),
                p.sources.len()
            )
        };
        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title(title))
            .highlight_style(Style::default().add_modifier(Modifier::BOLD).bg(Color::DarkGray));
        let mut state = ListState::default();
        if !visible.is_empty() {
            state.select(Some(ed.pick_idx.min(visible.len() - 1)));
        }
        frame.render_stateful_widget(list, area, &mut state);
        return;
    }
    if ed.naming {
        let para = Paragraph::new(ed.name.clone()).block(
            Block::default()
                .borders(Borders::ALL)
                .title("new spec file (under specs/, e.g. 010-payments.spec)"),
        );
        frame.render_widget(para, area);
        let cursor_x = area.x + 1 + (ed.name.chars().count() as u16).min(area.width.saturating_sub(2));
        frame.set_cursor_position(ratatui::layout::Position::new(cursor_x, area.y + 1));
        return;
    }

    let diag_height = ((ed.diagnostics.len() + 2).min(8) as u16).min(area.height / 2);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(diag_height)])
        .split(area);

    let title = format!(
        "{}{}{}",
        ed.state.file,
        if ed.state.dirty { "  [modified]" } else { "" },
        if ed.confirm_discard {
            "  — esc again to discard"
        } else {
            ""
        }
    );
    let visible: Vec<Line> = ed
        .state
        .lines
        .iter()
        .skip(ed.state.top)
        .map(|l| Line::from(l.clone()))
        .collect();
    let para = Paragraph::new(visible)
        .block(Block::default().borders(Borders::ALL).title(title))
        .wrap(Wrap { trim: false });
    frame.render_widget(para, chunks[0]);

    // cursor inside the text block: account for borders + scroll offset
    let text_height = (chunks[0].height.saturating_sub(2)) as usize;
    let top = crate::editor::visible_top(ed.state.row, ed.state.top, text_height);
    let cursor_x = chunks[0].x
        + 1
        + ed
            .state
            .col
            .min(chunks[0].width.saturating_sub(2) as usize) as u16;
    let cursor_y = chunks[0].y + 1 + (ed.state.row - top) as u16;
    frame.set_cursor_position(ratatui::layout::Position::new(cursor_x, cursor_y));

    if diag_height > 0 {
        let lines: Vec<Line> = ed
            .diagnostics
            .iter()
            .map(|d| Line::from(Span::styled(d.clone(), Style::default().fg(Color::Red))))
            .collect();
        let diag = Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title("diagnostics"));
        frame.render_widget(diag, chunks[1]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Input, ProjectData};
    use crate::snapshot::{Counts, GateSnapshot, RowOutcome, RowSnapshot, Verdict};
    use decispec_codegen::CodegenConfig;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::path::PathBuf;

    const FIXTURE_SPEC: &str = r#"
decision AUTH-001 "Auth decision" {
  status: accepted
  context: "needs auth"
  consequences: "+ safe; - latency"
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

    fn workspace(src: &str) -> decispec_ir::Workspace {
        let unit = decispec_parse::parse_file("0001.spec", src).unwrap();
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

    fn gate() -> GateSnapshot {
        GateSnapshot {
            project: "demo".to_string(),
            verdict: Verdict::Pass,
            strict_skipped: false,
            counts: Counts {
                rows: 2,
                passed: 2,
                failed: 0,
                uncovered: 0,
                skipped: 0,
                no_artifacts: 0,
            },
            failures: vec![],
            warnings: vec!["drift".to_string()],
            decisions: vec![],
        }
    }

    fn projects() -> Vec<ProjectData> {
        vec![
            ProjectData::loaded(
                "demo".to_string(),
                PathBuf::from("/tmp/demo"),
                workspace(FIXTURE_SPEC),
                vec![("specs/0001.spec".to_string(), FIXTURE_SPEC.to_string())],
                codegen_cfg(),
                Some(gate()),
            ),
            ProjectData::error(
                "broken".to_string(),
                PathBuf::from("/tmp/broken"),
                "specs/000.spec:3: unknown ref NOPE".to_string(),
                vec![],
            ),
        ]
    }

    fn buffer_text(terminal: &Terminal<TestBackend>) -> String {
        let buf = terminal.backend().buffer();
        let area = buf.area;
        (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| buf.cell((x, y)).unwrap().symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn specs_mode_shows_projects_decisions_and_detail() {
        let app = App::new(projects());
        let backend = TestBackend::new(120, 36);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(&app, f)).unwrap();
        let text = buffer_text(&terminal);
        assert!(text.contains("demo"), "text:\n{text}");
        assert!(text.contains("broken"), "text:\n{text}");
        assert!(text.contains("AUTH-001"), "text:\n{text}");
        assert!(text.contains("Auth decision"), "text:\n{text}");
        assert!(text.contains("accepted"), "text:\n{text}");
        assert!(text.contains("unit"), "text:\n{text}");
        assert!(text.contains("contract"), "text:\n{text}");
        assert!(text.contains("q quit"), "text:\n{text}");
    }

    #[test]
    fn broken_project_shows_its_error_not_a_panic() {
        let mut app = App::new(projects());
        app.update(Input::Right);
        let backend = TestBackend::new(120, 36);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(&app, f)).unwrap();
        let text = buffer_text(&terminal);
        assert!(text.contains("unknown ref NOPE"), "text:\n{text}");
    }

    #[test]
    fn artifacts_mode_lists_generated_files_and_content() {
        let mut app = App::new(projects());
        app.update(Input::Tab);
        assert_eq!(app.mode, Mode::Artifacts);
        let backend = TestBackend::new(120, 36);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(&app, f)).unwrap();
        let text = buffer_text(&terminal);
        assert!(text.contains(".decispec/diagrams/model.mmd"), "text:\n{text}");
        assert!(text.contains(".decispec/docs/decisions.md"), "text:\n{text}");
        // content pane shows the selected artifact's rendered text
        assert!(text.contains("flowchart"), "text:\n{text}");
    }

    #[test]
    fn dashboard_shows_levels_layers_and_gate_verdict() {
        let mut app = App::new(projects());
        app.update(Input::Tab);
        app.update(Input::Tab);
        assert_eq!(app.mode, Mode::Dashboard);
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(&app, f)).unwrap();
        let text = buffer_text(&terminal);
        assert!(text.contains("PASS"), "text:\n{text}");
        assert!(text.contains("2 rows"), "text:\n{text}");
        // decision levels band
        assert!(text.contains("decision levels"), "text:\n{text}");
        // layers band
        assert!(text.contains("unit"), "text:\n{text}");
        // warnings surfaced
        assert!(text.contains("drift"), "text:\n{text}");
    }

    #[test]
    fn editor_picker_lists_spec_files() {
        let mut app = App::new(projects());
        app.update(Input::Char('e'));
        let backend = TestBackend::new(120, 36);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(&app, f)).unwrap();
        let text = buffer_text(&terminal);
        assert!(text.contains("edit which file?"), "text:\n{text}");
        assert!(text.contains("specs/0001.spec"), "text:\n{text}");
        assert!(text.contains("edit which file"), "hints:\n{text}");
    }

    // -- F42: picker type-to-filter -----------------------------------------

    #[test]
    fn picker_filter_line_shows_query_and_match_count() {
        let projects = vec![ProjectData::loaded(
            "demo".to_string(),
            PathBuf::from("/tmp/demo"),
            workspace(FIXTURE_SPEC),
            vec![
                (
                    "specs/0001-auth.spec".to_string(),
                    FIXTURE_SPEC.to_string(),
                ),
                (
                    "specs/0002-user.spec".to_string(),
                    FIXTURE_SPEC.to_string(),
                ),
            ],
            codegen_cfg(),
            None,
        )];
        let mut app = App::new(projects);
        app.update(Input::Char('e'));
        // base hint advertises the filter before it is used
        let base = render_text(&app, 120, 20);
        assert!(base.contains("/ = filter"), "text:\n{base}");
        // engaged filter: query, match count, and only the match listed
        app.update(Input::Char('/'));
        app.update(Input::Char('u'));
        app.update(Input::Char('s'));
        app.update(Input::Char('e'));
        app.update(Input::Char('r'));
        let text = render_text(&app, 120, 20);
        assert!(text.contains("filter: /user"), "text:\n{text}");
        assert!(text.contains("1/2"), "match count:\n{text}");
        assert!(text.contains("specs/0002-user.spec"), "text:\n{text}");
        assert!(
            !text.contains("specs/0001-auth.spec"),
            "non-match hidden:\n{text}"
        );
    }

    // -- F43: specs-view decision filter -------------------------------------

    #[test]
    fn specs_view_shows_filter_line_and_narrows_rows() {
        let mut app = App::new(proof_projects(None));
        // no filter engaged and no keyword: no filter line, all rows listed
        let base = render_text(&app, 120, 36);
        assert!(
            !base.contains("filter:"),
            "filter line hidden while unused:\n{base}"
        );
        assert!(base.contains("AUTH-001"), "text:\n{base}");
        assert!(base.contains("PAY-001"), "text:\n{base}");
        assert!(base.contains("AUD-001"), "text:\n{base}");
        // engaged filter: query + match count ride above the narrowed rows
        app.update(Input::Char('/'));
        for c in "pay".chars() {
            app.update(Input::Char(c));
        }
        let text = render_text(&app, 120, 36);
        assert!(text.contains("filter: pay"), "text:\n{text}");
        assert!(text.contains("1/4"), "match count:\n{text}");
        assert!(text.contains("PAY-001"), "text:\n{text}");
        assert!(
            !text.contains("AUD-001"),
            "non-match hidden from the rows:\n{text}"
        );
        // the detail pane follows the narrowed selection
        assert!(text.contains("needs payments"), "text:\n{text}");
        assert!(!text.contains("needs audit"), "text:\n{text}");
    }

    #[test]
    fn editor_shows_text_diagnostics_and_discard_warning() {
        let mut app = App::new(projects());
        app.update(Input::Char('e'));
        app.update(Input::Enter);
        // type 'x' on the first (empty) line -> parse error in live diagnostics
        app.update(Input::Char('x'));
        app.update(Input::Esc); // arms discard confirmation
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(&app, f)).unwrap();
        let text = buffer_text(&terminal);
        assert!(text.contains("[modified]"), "text:\n{text}");
        assert!(text.contains("diagnostics"), "text:\n{text}");
        assert!(
            text.contains("specs/0001.spec:1"),
            "text:\n{text}"
        );
        // the armed discard confirmation is visible, not just internal state
        assert!(text.contains("esc again to discard"), "text:\n{text}");
        assert!(text.contains("Ctrl+S save"), "text:\n{text}");
    }

    const PROOF_SPEC: &str = r#"
decision AUTH-001 "Auth decision" {
  status: accepted
  context: "needs auth"
  consequences: "+ safe; - latency"
}

decision PAY-001 "Payments decision" {
  status: proposed
  context: "needs payments"
  consequences: "+ money; - complexity"
}

decision AUD-001 "Audit decision" {
  status: accepted
  context: "needs audit"
  consequences: "+ traceability; - storage"
}

decision OLD-1 "Old decision" {
  status: superseded
  superseded_by: AUTH-001
}

spec AUTH-001 {
  requirement AUTH-001-R1 (EARS) {
    text: "WHEN a request arrives THEN it SHALL be authenticated"
    layers: [unit]
    scenarios: [ok]
  }
  scenario ok {
    given: "a request"
    when: "it arrives"
    then: "it is authenticated"
  }
}
"#;

    fn decision_snapshot(
        id: &str,
        status: Status,
        results: &[Option<RowOutcome>],
    ) -> crate::snapshot::DecisionSnapshot {
        crate::snapshot::DecisionSnapshot {
            id: id.to_string(),
            status,
            outcome: String::new(),
            rows: results
                .iter()
                .map(|result| RowSnapshot {
                    decision: id.to_string(),
                    requirement: "R".to_string(),
                    item: "i".to_string(),
                    layer: None,
                    result: *result,
                    detail: String::new(),
                })
                .collect(),
        }
    }

    /// A report whose rollup is: AUTH-001 proven, PAY-001 partial,
    /// AUD-001 unproven (all rows skipped), OLD-1 excluded (superseded).
    fn proof_gate() -> GateSnapshot {
        GateSnapshot {
            project: "demo".to_string(),
            verdict: Verdict::Pass,
            strict_skipped: false,
            counts: Counts {
                rows: 5,
                passed: 3,
                failed: 1,
                uncovered: 0,
                skipped: 1,
                no_artifacts: 0,
            },
            failures: vec![],
            warnings: vec![],
            decisions: vec![
                decision_snapshot(
                    "AUTH-001",
                    Status::Accepted,
                    &[Some(RowOutcome::Pass), Some(RowOutcome::Pass)],
                ),
                decision_snapshot(
                    "PAY-001",
                    Status::Proposed,
                    &[Some(RowOutcome::Pass), Some(RowOutcome::Fail)],
                ),
                decision_snapshot("AUD-001", Status::Accepted, &[Some(RowOutcome::Skipped)]),
                decision_snapshot("OLD-1", Status::Superseded, &[]),
            ],
        }
    }

    fn proof_projects(gate: Option<GateSnapshot>) -> Vec<ProjectData> {
        vec![ProjectData::loaded(
            "demo".to_string(),
            PathBuf::from("/tmp/demo"),
            workspace(PROOF_SPEC),
            vec![("specs/0001.spec".to_string(), PROOF_SPEC.to_string())],
            codegen_cfg(),
            gate,
        )]
    }

    fn render_text(app: &App, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(app, f)).unwrap();
        buffer_text(&terminal)
    }

    #[test]
    fn tui_marks_each_decision_with_its_proof_state() {
        let app = App::new(proof_projects(Some(proof_gate())));
        let text = render_text(&app, 140, 40);
        // The glyph + label ride in the buffer text itself, so the mark is
        // not color-only; each sits beside its decision id / status level.
        assert!(text.contains("✓ proven"), "text:\n{text}");
        assert!(text.contains("◐ partial"), "text:\n{text}");
        assert!(text.contains("○ unproven"), "text:\n{text}");
        // Spot-check the join: the proven mark is on AUTH-001's row.
        assert!(
            text.lines()
                .any(|l| l.contains("AUTH-001") && l.contains("✓ proven")),
            "text:\n{text}"
        );
    }

    #[test]
    fn tui_sidebar_shows_each_projects_proven_percent() {
        let app = App::new(proof_projects(Some(proof_gate())));
        let text = render_text(&app, 140, 40);
        // 1 of 3 decisions proven: the project row in the sidebar carries 33%.
        assert!(
            text.lines().any(|l| l.contains("demo") && l.contains("33%")),
            "text:\n{text}"
        );
    }

    #[test]
    fn tui_sidebar_shows_no_percent_without_a_gate_run() {
        let app = App::new(proof_projects(None));
        let text = render_text(&app, 140, 40);
        assert!(
            !text.lines().any(|l| l.contains("demo") && l.contains('%')),
            "text:\n{text}"
        );
    }

    #[test]
    fn tui_headline_uses_the_nearest_percent() {
        let cov = Coverage { proven: 54, gatable: 74, percent: 72, decisions: vec![] };
        assert_eq!(headline_text(&cov), "54/74 decisions proven (73%)");
    }

    #[test]
    fn tui_dashboard_shows_the_headline_percentage() {
        let mut app = App::new(proof_projects(Some(proof_gate())));
        app.update(Input::Tab);
        app.update(Input::Tab);
        assert_eq!(app.mode, Mode::Dashboard);
        let text = render_text(&app, 140, 40);
        // Headline beside the last gate verdict, same wording the
        // `decispec coverage` console prints.
        assert!(text.contains("PASS"), "text:\n{text}");
        assert!(
            text.contains("coverage: 1/3 decisions proven (33%)"),
            "text:\n{text}"
        );
    }

    #[test]
    fn tui_shows_no_gate_run_instead_of_a_fake_zero() {
        let mut app = App::new(proof_projects(None));
        // Spec list: no invented marks, the pane says why.
        let text = render_text(&app, 140, 40);
        assert!(text.contains("no gate run"), "specs text:\n{text}");
        assert!(!text.contains("0%"), "specs text:\n{text}");
        assert!(!text.contains("✓"), "specs text:\n{text}");
        // Dashboard: an explicit no-gate-run marker, never a fabricated zero.
        app.update(Input::Tab);
        app.update(Input::Tab);
        assert_eq!(app.mode, Mode::Dashboard);
        let text = render_text(&app, 140, 40);
        assert!(text.contains("coverage: no gate run"), "text:\n{text}");
        assert!(!text.contains("0%"), "text:\n{text}");
        assert!(!text.contains("coverage: 0/"), "text:\n{text}");
    }
}
