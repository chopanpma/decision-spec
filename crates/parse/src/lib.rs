//! Recursive-descent parser for the `.spec` DSL (no parser generator).
//!
//! `parse_file` turns one file into a [`FileUnit`]; `merge` combines units;
//! `validate` checks cross-references and global ID uniqueness, returning
//! human-friendly `file:line: message` diagnostics.

use decispec_ir::*;

#[derive(Debug, Clone, thiserror::Error)]
#[error("{file}:{line}: {message}")]
pub struct ParseError {
    pub file: String,
    pub line: usize,
    pub message: String,
}

impl ParseError {
    fn new(file: &str, line: usize, message: String) -> Self {
        ParseError {
            file: file.to_string(),
            line,
            message,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{file}:{line}: {message}")]
pub struct Diagnostic {
    pub file: String,
    pub line: usize,
    pub message: String,
}

impl Diagnostic {
    fn new(src: &Src, message: String) -> Self {
        Diagnostic {
            file: src.file.clone(),
            line: src.line,
            message,
        }
    }
}

// ---------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Ident(String),
    Str(String),
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    LParen,
    RParen,
    Colon,
    Comma,
    Arrow,
    DashedArrow,
}

impl Tok {
    fn describe(&self) -> String {
        match self {
            Tok::Ident(s) => format!("'{s}'"),
            Tok::Str(_) => "a quoted string".to_string(),
            Tok::LBrace => "'{'".to_string(),
            Tok::RBrace => "'}'".to_string(),
            Tok::LBracket => "'['".to_string(),
            Tok::RBracket => "']'".to_string(),
            Tok::LParen => "'('".to_string(),
            Tok::RParen => "')'".to_string(),
            Tok::Colon => "':'".to_string(),
            Tok::Comma => "','".to_string(),
            Tok::Arrow => "'->'".to_string(),
            Tok::DashedArrow => "'-->'".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
struct Spanned {
    tok: Tok,
    line: usize,
}

fn lex(file: &str, src: &str) -> Result<Vec<Spanned>, ParseError> {
    let chars: Vec<char> = src.chars().collect();
    let mut toks = Vec::new();
    let mut i = 0;
    let mut line = 1usize;

    while i < chars.len() {
        let c = chars[i];
        match c {
            '\n' => {
                line += 1;
                i += 1;
            }
            c if c.is_whitespace() => {
                i += 1;
            }
            '#' => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '"' => {
                let start_line = line;
                i += 1;
                let mut s = String::new();
                loop {
                    if i >= chars.len() {
                        return Err(ParseError::new(
                            file,
                            start_line,
                            "unterminated string literal (missing closing '\"')".to_string(),
                        ));
                    }
                    match chars[i] {
                        '"' => {
                            i += 1;
                            break;
                        }
                        '\\' if i + 1 < chars.len()
                            && (chars[i + 1] == '"' || chars[i + 1] == '\\') =>
                        {
                            s.push(chars[i + 1]);
                            i += 2;
                        }
                        '\n' => {
                            return Err(ParseError::new(
                                file,
                                start_line,
                                "unterminated string literal (newline before closing '\"')"
                                    .to_string(),
                            ));
                        }
                        ch => {
                            s.push(ch);
                            i += 1;
                        }
                    }
                }
                toks.push(Spanned {
                    tok: Tok::Str(s),
                    line: start_line,
                });
            }
            '{' => {
                toks.push(Spanned {
                    tok: Tok::LBrace,
                    line,
                });
                i += 1;
            }
            '}' => {
                toks.push(Spanned {
                    tok: Tok::RBrace,
                    line,
                });
                i += 1;
            }
            '[' => {
                toks.push(Spanned {
                    tok: Tok::LBracket,
                    line,
                });
                i += 1;
            }
            ']' => {
                toks.push(Spanned {
                    tok: Tok::RBracket,
                    line,
                });
                i += 1;
            }
            '(' => {
                toks.push(Spanned {
                    tok: Tok::LParen,
                    line,
                });
                i += 1;
            }
            ')' => {
                toks.push(Spanned {
                    tok: Tok::RParen,
                    line,
                });
                i += 1;
            }
            ':' => {
                toks.push(Spanned {
                    tok: Tok::Colon,
                    line,
                });
                i += 1;
            }
            ',' => {
                toks.push(Spanned {
                    tok: Tok::Comma,
                    line,
                });
                i += 1;
            }
            '-' => {
                if i + 1 < chars.len() && chars[i + 1] == '>' {
                    toks.push(Spanned {
                        tok: Tok::Arrow,
                        line,
                    });
                    i += 2;
                } else if i + 2 < chars.len() && chars[i + 1] == '-' && chars[i + 2] == '>' {
                    toks.push(Spanned {
                        tok: Tok::DashedArrow,
                        line,
                    });
                    i += 3;
                } else {
                    return Err(ParseError::new(
                        file,
                        line,
                        "unexpected '-', expected '->' or '-->'".to_string(),
                    ));
                }
            }
            c if c.is_alphanumeric() || c == '_' => {
                let start = i;
                while i < chars.len() {
                    let ch = chars[i];
                    // `-` continues an identifier only when it is not the start of `->`
                    // or `-->`, which the caller dispatches on.
                    let arrow_start = ch == '-'
                        && ((i + 1 < chars.len() && chars[i + 1] == '>')
                            || (i + 2 < chars.len() && chars[i + 1] == '-' && chars[i + 2] == '>'));
                    if ch.is_alphanumeric() || ch == '_' || (ch == '-' && !arrow_start) {
                        i += 1;
                    } else {
                        break;
                    }
                }
                toks.push(Spanned {
                    tok: Tok::Ident(chars[start..i].iter().collect()),
                    line,
                });
            }
            _ => {
                return Err(ParseError::new(
                    file,
                    line,
                    format!("unexpected character '{c}'"),
                ));
            }
        }
    }
    Ok(toks)
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// The IR fragments contributed by a single `.spec` file.
#[derive(Debug, Clone, Default)]
pub struct FileUnit {
    pub decisions: Vec<Decision>,
    pub models: Vec<Model>,
    pub specs: Vec<Spec>,
}

struct Parser {
    toks: Vec<Spanned>,
    pos: usize,
    file: String,
}

impl Parser {
    fn cur(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|s| &s.tok)
    }

    fn cur_line(&self) -> usize {
        self.toks
            .get(self.pos)
            .map(|s| s.line)
            .unwrap_or_else(|| self.toks.last().map(|s| s.line).unwrap_or(1))
    }

    fn err(&self, message: impl Into<String>) -> ParseError {
        let found = self
            .cur()
            .map(|t| t.describe())
            .unwrap_or_else(|| "end of file".to_string());
        ParseError::new(
            &self.file,
            self.cur_line(),
            format!("{}, found {found}", message.into()),
        )
    }

    fn bump(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).map(|s| s.tok.clone());
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn expect(&mut self, want: &Tok, what: &str) -> Result<(), ParseError> {
        if self.cur() == Some(want) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.err(format!("expected {what}")))
        }
    }

    fn keyword(&mut self, kw: &str) -> Result<(), ParseError> {
        match self.cur() {
            Some(Tok::Ident(s)) if s == kw => {
                self.pos += 1;
                Ok(())
            }
            _ => Err(self.err(format!("expected '{kw}'"))),
        }
    }

    fn ident(&mut self) -> Result<(String, usize), ParseError> {
        match self.cur() {
            Some(Tok::Ident(s)) => {
                let line = self.cur_line();
                let s = s.clone();
                self.pos += 1;
                Ok((s, line))
            }
            _ => Err(self.err("expected an identifier")),
        }
    }

    fn string(&mut self) -> Result<String, ParseError> {
        match self.cur() {
            Some(Tok::Str(_)) => match self.bump() {
                Some(Tok::Str(s)) => Ok(s),
                _ => unreachable!(),
            },
            _ => Err(self.err("expected a quoted string")),
        }
    }

    fn ident_list(&mut self) -> Result<Vec<String>, ParseError> {
        self.expect(&Tok::LBracket, "'['")?;
        let mut out = Vec::new();
        loop {
            let (id, _) = self.ident()?;
            out.push(id);
            if self.cur() == Some(&Tok::Comma) {
                self.pos += 1;
            } else {
                break;
            }
        }
        self.expect(&Tok::RBracket, "']'")?;
        Ok(out)
    }

    fn layer_list(&mut self) -> Result<Vec<Layer>, ParseError> {
        self.expect(&Tok::LBracket, "'['")?;
        let mut out = Vec::new();
        loop {
            let (id, line) = self.ident()?;
            match Layer::parse(&id) {
                Some(l) => out.push(l),
                None => {
                    return Err(ParseError::new(
                        &self.file,
                        line,
                        format!(
                            "unknown layer '{id}' (expected one of: unit, contract, e2e, infra, fitness)"
                        ),
                    ));
                }
            }
            if self.cur() == Some(&Tok::Comma) {
                self.pos += 1;
            } else {
                break;
            }
        }
        self.expect(&Tok::RBracket, "']'")?;
        Ok(out)
    }

    fn file_unit(&mut self) -> Result<FileUnit, ParseError> {
        let mut unit = FileUnit::default();
        while self.cur().is_some() {
            match self.cur() {
                Some(Tok::Ident(s)) if s == "decision" => {
                    unit.decisions.push(self.decision()?);
                }
                Some(Tok::Ident(s)) if s == "model" => {
                    unit.models.push(self.model()?);
                }
                Some(Tok::Ident(s)) if s == "spec" => {
                    unit.specs.push(self.spec()?);
                }
                _ => {
                    return Err(self.err("expected 'decision', 'model' or 'spec' at top level"));
                }
            }
        }
        Ok(unit)
    }

    fn decision(&mut self) -> Result<Decision, ParseError> {
        self.keyword("decision")?;
        let (id, line) = self.ident()?;
        let title = self.string()?;
        self.expect(&Tok::LBrace, "'{'")?;
        let mut status = None;
        let mut context = None;
        let mut consequences = None;
        let mut superseded_by = None;
        while self.cur() != Some(&Tok::RBrace) {
            if self.cur().is_none() {
                return Err(self.err("expected '}' to close decision block"));
            }
            let (field, fline) = self.ident()?;
            self.expect(&Tok::Colon, "':'")?;
            match field.as_str() {
                "status" => {
                    let (v, vline) = self.ident()?;
                    status = Some(
                        Status::parse(&v).ok_or_else(|| {
                            ParseError::new(
                                &self.file,
                                vline,
                                format!(
                                    "unknown status '{v}', expected one of: accepted, proposed, rejected, superseded"
                                ),
                            )
                        })?,
                    );
                }
                "context" => context = Some(self.string()?),
                "consequences" => consequences = Some(self.string()?),
                "superseded_by" => {
                    let (v, _) = self.ident()?;
                    superseded_by = Some(v);
                }
                other => {
                    return Err(ParseError::new(
                        &self.file,
                        fline,
                        format!("unknown decision field '{other}'"),
                    ));
                }
            }
        }
        self.expect(&Tok::RBrace, "'}'")?;
        Ok(Decision {
            id,
            title,
            status: status.unwrap_or(Status::Proposed),
            context,
            consequences,
            superseded_by,
            src: Src {
                file: self.file.clone(),
                line,
            },
        })
    }

    fn model(&mut self) -> Result<Model, ParseError> {
        self.keyword("model")?;
        self.expect(&Tok::LBrace, "'{'")?;
        let mut m = Model {
            file: self.file.clone(),
            ..Model::default()
        };
        while self.cur() != Some(&Tok::RBrace) {
            if self.cur().is_none() {
                return Err(self.err("expected '}' to close model block"));
            }
            match self.cur() {
                Some(Tok::Ident(s)) if s == "container" => {
                    self.pos += 1;
                    let (id, _) = self.ident()?;
                    let label = self.string()?;
                    m.containers.push(Container { id, label });
                }
                Some(Tok::Ident(s)) if s == "rel" => {
                    self.pos += 1;
                    let (from, _) = self.ident()?;
                    self.expect(&Tok::Arrow, "'->'")?;
                    let (to, _) = self.ident()?;
                    self.expect(&Tok::Colon, "':'")?;
                    let label = self.string()?;
                    m.rels.push(Rel { from, to, label });
                }
                Some(Tok::Ident(s)) if s == "flow" => {
                    self.pos += 1;
                    let (id, line) = self.ident()?;
                    self.expect(&Tok::LBrace, "'{'")?;
                    let mut steps = Vec::new();
                    while self.cur() != Some(&Tok::RBrace) {
                        if self.cur().is_none() {
                            return Err(self.err("expected '}' to close flow block"));
                        }
                        steps.push(self.flow_step()?);
                    }
                    self.expect(&Tok::RBrace, "'}'")?;
                    m.flows.push(Flow {
                        id,
                        steps,
                        src: Src {
                            file: self.file.clone(),
                            line,
                        },
                    });
                }
                _ => {
                    return Err(
                        self.err("expected 'container', 'rel' or 'flow' inside model block")
                    );
                }
            }
        }
        self.expect(&Tok::RBrace, "'}'")?;
        Ok(m)
    }

    fn flow_step(&mut self) -> Result<FlowStep, ParseError> {
        if let Some(Tok::Ident(s)) = self.cur()
            && s == "alt"
        {
            self.pos += 1;
            let (name, _) = self.ident()?;
            self.expect(&Tok::LBrace, "'{'")?;
            let then_branch = self.flow_steps_until_rbrace()?;
            self.keyword("else")?;
            self.expect(&Tok::LBrace, "'{'")?;
            let else_branch = self.flow_steps_until_rbrace()?;
            return Ok(FlowStep::Alt {
                name,
                then_branch,
                else_branch,
            });
        }
        let (from, line) = self.ident()?;
        let dashed = match self.cur() {
            Some(Tok::Arrow) => {
                self.pos += 1;
                false
            }
            Some(Tok::DashedArrow) => {
                self.pos += 1;
                true
            }
            _ => return Err(self.err("expected '->' or '-->' in flow step")),
        };
        let (to, _) = self.ident()?;
        self.expect(&Tok::Colon, "':'")?;
        let label = self.string()?;
        let _ = line;
        Ok(FlowStep::Message {
            from,
            to,
            label,
            dashed,
        })
    }

    fn flow_steps_until_rbrace(&mut self) -> Result<Vec<FlowStep>, ParseError> {
        let mut steps = Vec::new();
        while self.cur() != Some(&Tok::RBrace) {
            if self.cur().is_none() {
                return Err(self.err("expected '}' to close flow branch"));
            }
            steps.push(self.flow_step()?);
        }
        self.expect(&Tok::RBrace, "'}'")?;
        Ok(steps)
    }

    fn spec(&mut self) -> Result<Spec, ParseError> {
        self.keyword("spec")?;
        let (id, line) = self.ident()?;
        self.expect(&Tok::LBrace, "'{'")?;
        let mut sp = Spec {
            id,
            requirements: Vec::new(),
            scenarios: Vec::new(),
            invariants: Vec::new(),
            infra_policies: Vec::new(),
            src: Src {
                file: self.file.clone(),
                line,
            },
        };
        while self.cur() != Some(&Tok::RBrace) {
            if self.cur().is_none() {
                return Err(self.err("expected '}' to close spec block"));
            }
            match self.cur() {
                Some(Tok::Ident(s)) if s == "requirement" => {
                    self.pos += 1;
                    let (rid, rline) = self.ident()?;
                    let style = if self.cur() == Some(&Tok::LParen) {
                        self.pos += 1;
                        let (st, _) = self.ident()?;
                        self.expect(&Tok::RParen, "')'")?;
                        Some(st)
                    } else {
                        None
                    };
                    self.expect(&Tok::LBrace, "'{'")?;
                    let mut text = None;
                    let mut layers = None;
                    let mut scenarios = None;
                    while self.cur() != Some(&Tok::RBrace) {
                        if self.cur().is_none() {
                            return Err(self.err("expected '}' to close requirement block"));
                        }
                        let (field, fline) = self.ident()?;
                        self.expect(&Tok::Colon, "':'")?;
                        match field.as_str() {
                            "text" => text = Some(self.string()?),
                            "layers" => layers = Some(self.layer_list()?),
                            "scenarios" => scenarios = Some(self.ident_list()?),
                            other => {
                                return Err(ParseError::new(
                                    &self.file,
                                    fline,
                                    format!("unknown requirement field '{other}'"),
                                ));
                            }
                        }
                    }
                    self.expect(&Tok::RBrace, "'}'")?;
                    sp.requirements.push(Requirement {
                        id: rid,
                        style,
                        text: text.unwrap_or_default(),
                        layers: layers.unwrap_or_default(),
                        scenarios: scenarios.unwrap_or_default(),
                        src: Src {
                            file: self.file.clone(),
                            line: rline,
                        },
                    });
                }
                Some(Tok::Ident(s)) if s == "scenario" => {
                    self.pos += 1;
                    let (sid, sline) = self.ident()?;
                    self.expect(&Tok::LBrace, "'{'")?;
                    let mut given = None;
                    let mut when = None;
                    let mut then = None;
                    while self.cur() != Some(&Tok::RBrace) {
                        if self.cur().is_none() {
                            return Err(self.err("expected '}' to close scenario block"));
                        }
                        let (field, _) = self.ident()?;
                        self.expect(&Tok::Colon, "':'")?;
                        match field.as_str() {
                            "given" => given = Some(self.string()?),
                            "when" => when = Some(self.string()?),
                            "then" => then = Some(self.string()?),
                            other => {
                                return Err(ParseError::new(
                                    &self.file,
                                    sline,
                                    format!("unknown scenario field '{other}'"),
                                ));
                            }
                        }
                    }
                    self.expect(&Tok::RBrace, "'}'")?;
                    let missing = ["given", "when", "then"]
                        .iter()
                        .zip([given.is_none(), when.is_none(), then.is_none()].iter())
                        .filter(|(_, m)| **m)
                        .map(|(f, _)| f.to_string())
                        .collect::<Vec<_>>();
                    if !missing.is_empty() {
                        return Err(ParseError::new(
                            &self.file,
                            sline,
                            format!(
                                "scenario '{sid}' is missing field(s): {}",
                                missing.join(", ")
                            ),
                        ));
                    }
                    sp.scenarios.push(Scenario {
                        id: sid,
                        given: given.unwrap(),
                        when: when.unwrap(),
                        then: then.unwrap(),
                        src: Src {
                            file: self.file.clone(),
                            line: sline,
                        },
                    });
                }
                Some(Tok::Ident(s)) if s == "invariant" => {
                    self.pos += 1;
                    let (iid, iline) = self.ident()?;
                    self.expect(&Tok::LBrace, "'{'")?;
                    let mut expr = None;
                    let mut layers = None;
                    while self.cur() != Some(&Tok::RBrace) {
                        if self.cur().is_none() {
                            return Err(self.err("expected '}' to close invariant block"));
                        }
                        let (field, fline) = self.ident()?;
                        self.expect(&Tok::Colon, "':'")?;
                        match field.as_str() {
                            "expr" => expr = Some(self.string()?),
                            "layers" => layers = Some(self.layer_list()?),
                            other => {
                                return Err(ParseError::new(
                                    &self.file,
                                    fline,
                                    format!("unknown invariant field '{other}'"),
                                ));
                            }
                        }
                    }
                    self.expect(&Tok::RBrace, "'}'")?;
                    if expr.is_none() {
                        return Err(ParseError::new(
                            &self.file,
                            iline,
                            format!("invariant '{iid}' is missing field: expr"),
                        ));
                    }
                    sp.invariants.push(Invariant {
                        id: iid,
                        expr: expr.unwrap(),
                        layers: layers.unwrap_or_default(),
                        src: Src {
                            file: self.file.clone(),
                            line: iline,
                        },
                    });
                }
                Some(Tok::Ident(s)) if s == "infra_policy" => {
                    self.pos += 1;
                    let (pid, pline) = self.ident()?;
                    self.expect(&Tok::LBrace, "'{'")?;
                    let mut description = None;
                    let mut layers = None;
                    while self.cur() != Some(&Tok::RBrace) {
                        if self.cur().is_none() {
                            return Err(self.err("expected '}' to close infra_policy block"));
                        }
                        let (field, fline) = self.ident()?;
                        self.expect(&Tok::Colon, "':'")?;
                        match field.as_str() {
                            "description" => description = Some(self.string()?),
                            "layers" => layers = Some(self.layer_list()?),
                            other => {
                                return Err(ParseError::new(
                                    &self.file,
                                    fline,
                                    format!("unknown infra_policy field '{other}'"),
                                ));
                            }
                        }
                    }
                    self.expect(&Tok::RBrace, "'}'")?;
                    if description.is_none() {
                        return Err(ParseError::new(
                            &self.file,
                            pline,
                            format!("infra_policy '{pid}' is missing field: description"),
                        ));
                    }
                    sp.infra_policies.push(InfraPolicy {
                        id: pid,
                        description: description.unwrap(),
                        layers: layers.unwrap_or_default(),
                        src: Src {
                            file: self.file.clone(),
                            line: pline,
                        },
                    });
                }
                _ => {
                    return Err(self.err(
                        "expected 'requirement', 'scenario', 'invariant' or 'infra_policy' inside spec block",
                    ));
                }
            }
        }
        self.expect(&Tok::RBrace, "'}'")?;
        Ok(sp)
    }
}

pub fn parse_file(file: &str, src: &str) -> Result<FileUnit, ParseError> {
    let toks = lex(file, src)?;
    let mut p = Parser {
        toks,
        pos: 0,
        file: file.to_string(),
    };
    p.file_unit()
}

pub fn merge(units: Vec<FileUnit>) -> Workspace {
    let mut ws = Workspace::default();
    for u in units {
        ws.decisions.extend(u.decisions);
        ws.models.extend(u.models);
        ws.specs.extend(u.specs);
    }
    ws
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Cross-reference and uniqueness checks. Returns diagnostics with file:line.
pub fn validate(ws: &Workspace) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    // Global ID uniqueness across decisions, specs, requirements, scenarios,
    // invariants, infra policies, flows and containers.
    let mut seen: std::collections::BTreeMap<String, Src> = std::collections::BTreeMap::new();
    let mut check = |id: &str, src: &Src, diags: &mut Vec<Diagnostic>| {
        if let Some(first) = seen.get(id) {
            diags.push(Diagnostic::new(
                src,
                format!(
                    "duplicate id '{id}' (first defined at {}:{})",
                    first.file, first.line
                ),
            ));
        } else {
            seen.insert(id.to_string(), src.clone());
        }
    };
    for d in &ws.decisions {
        check(&d.id, &d.src, &mut diags);
    }
    for m in &ws.models {
        for c in &m.containers {
            check(
                &c.id,
                &Src {
                    file: m.file.clone(),
                    line: 0,
                },
                &mut diags,
            );
        }
        for f in &m.flows {
            check(&f.id, &f.src, &mut diags);
        }
    }
    for sp in &ws.specs {
        // NB: the spec id deliberately equals its decision id (spec <ID>
        // references decision <ID>), so it is not checked for uniqueness here.
        for r in &sp.requirements {
            check(&r.id, &r.src, &mut diags);
        }
        for s in &sp.scenarios {
            check(&s.id, &s.src, &mut diags);
        }
        for i in &sp.invariants {
            check(&i.id, &i.src, &mut diags);
        }
        for p in &sp.infra_policies {
            check(&p.id, &p.src, &mut diags);
        }
    }

    let decision_ids: std::collections::BTreeSet<&str> =
        ws.decisions.iter().map(|d| d.id.as_str()).collect();

    // spec <ID> must reference an existing decision ID (spec id == decision id).
    for sp in &ws.specs {
        if !decision_ids.contains(sp.id.as_str()) {
            diags.push(Diagnostic::new(
                &sp.src,
                format!("spec '{}' references unknown decision '{}'", sp.id, sp.id),
            ));
        }
    }

    // Decision-level refs.
    for d in &ws.decisions {
        if let Some(target) = &d.superseded_by
            && !decision_ids.contains(target.as_str())
        {
            diags.push(Diagnostic::new(
                &d.src,
                format!(
                    "decision '{}' has unknown superseded_by target '{target}'",
                    d.id
                ),
            ));
        }
        match (d.status, &d.superseded_by) {
            (Status::Superseded, None) => diags.push(Diagnostic::new(
                &d.src,
                format!("decision '{}' has status superseded but no superseded_by field", d.id),
            )),
            (status, Some(_)) if status != Status::Superseded => diags.push(Diagnostic::new(
                &d.src,
                format!(
                    "decision '{}' sets superseded_by but has status '{}' (superseded_by is only valid with status: superseded)",
                    d.id,
                    status.as_str()
                ),
            )),
            _ => {}
        }
    }

    // Requirement -> scenario refs, scoped to the same spec block.
    for sp in &ws.specs {
        let scen_ids: std::collections::BTreeSet<&str> =
            sp.scenarios.iter().map(|s| s.id.as_str()).collect();
        for r in &sp.requirements {
            for sref in &r.scenarios {
                if !scen_ids.contains(sref.as_str()) {
                    diags.push(Diagnostic::new(
                        &r.src,
                        format!(
                            "requirement '{}' references unknown scenario '{sref}' (must be defined in spec '{}')",
                            r.id, sp.id
                        ),
                    ));
                }
            }
        }
    }

    // Flow participants must be declared containers in the same model block.
    for m in &ws.models {
        let container_ids: std::collections::BTreeSet<&str> =
            m.containers.iter().map(|c| c.id.as_str()).collect();
        for f in &m.flows {
            fn check_participant(
                p: &str,
                container_ids: &std::collections::BTreeSet<&str>,
                f: &Flow,
                diags: &mut Vec<Diagnostic>,
            ) {
                if !container_ids.contains(p) {
                    diags.push(Diagnostic::new(
                        &f.src,
                        format!("flow '{}' uses undeclared container '{p}'", f.id),
                    ));
                }
            }
            fn walk(
                steps: &[FlowStep],
                container_ids: &std::collections::BTreeSet<&str>,
                f: &Flow,
                diags: &mut Vec<Diagnostic>,
            ) {
                for step in steps {
                    match step {
                        FlowStep::Message { from, to, .. } => {
                            check_participant(from, container_ids, f, diags);
                            check_participant(to, container_ids, f, diags);
                        }
                        FlowStep::Alt {
                            then_branch,
                            else_branch,
                            ..
                        } => {
                            walk(then_branch, container_ids, f, diags);
                            walk(else_branch, container_ids, f, diags);
                        }
                    }
                }
            }
            walk(&f.steps, &container_ids, f, &mut diags);
        }
    }

    diags
}

#[cfg(test)]
mod tests {
    use super::*;

    pub const DEMO_SPEC: &str = r#"
# AUTH-001 demo spec
decision AUTH-001 "Use OAuth2 client credentials for service-to-service auth" {
  status: accepted                # accepted | proposed | rejected | superseded
  context: "Internal services need machine-to-machine auth without shared secrets"
  consequences: "+ no shared secrets in code; - token latency on cold start"
}

model {
  container api "Orders API"
  container auth "Auth Service"
  rel api -> auth: "requests token"
  flow svc_auth {
    api -> auth: "POST /token"
    alt valid {
      auth --> api: "200 + access token"
    } else {
      auth --> api: "401 unauthorized"
    }
  }
}

spec AUTH-001 {
  requirement AUTH-001-R1 (EARS) {
    text: "WHEN a service presents valid client credentials THEN the auth service SHALL return an access token"
    layers: [unit, contract, e2e]
    scenarios: [valid_token]
  }
  requirement AUTH-001-R2 (EARS) {
    text: "WHEN credentials are invalid THEN the auth service SHALL reject with 401"
    layers: [unit]
    scenarios: [invalid_token]
  }
  scenario valid_token {
    given: "valid client credentials"
    when: "a token request is made"
    then: "the response status is 200 and a token is returned"
  }
  scenario invalid_token {
    given: "invalid client credentials"
    when: "a token request is made"
    then: "the response status is 401"
  }
  invariant token_ttl {
    expr: "access_token.ttl_seconds <= 3600"
    layers: [unit]
  }
  infra_policy no_hardcoded_secrets {
    description: "No secrets committed in source or config"
    layers: [infra]
  }
}
"#;

    #[test]
    fn parses_demo_spec() {
        let unit = parse_file("specs/0001-auth.spec", DEMO_SPEC).expect("parse failed");
        assert_eq!(unit.decisions.len(), 1);
        let d = &unit.decisions[0];
        assert_eq!(d.id, "AUTH-001");
        assert_eq!(
            d.title,
            "Use OAuth2 client credentials for service-to-service auth"
        );
        assert_eq!(d.status, Status::Accepted);
        assert_eq!(
            d.context.as_deref(),
            Some("Internal services need machine-to-machine auth without shared secrets")
        );
        assert_eq!(
            d.consequences.as_deref(),
            Some("+ no shared secrets in code; - token latency on cold start")
        );

        assert_eq!(unit.models.len(), 1);
        let m = &unit.models[0];
        assert_eq!(m.containers.len(), 2);
        assert_eq!(m.containers[0].id, "api");
        assert_eq!(m.containers[0].label, "Orders API");
        assert_eq!(m.rels.len(), 1);
        assert_eq!(m.rels[0].from, "api");
        assert_eq!(m.rels[0].to, "auth");
        assert_eq!(m.rels[0].label, "requests token");
        assert_eq!(m.flows.len(), 1);
        let flow = &m.flows[0];
        assert_eq!(flow.id, "svc_auth");
        assert_eq!(flow.steps.len(), 2);
        match &flow.steps[0] {
            FlowStep::Message {
                from,
                to,
                label,
                dashed,
            } => {
                assert_eq!(
                    (from.as_str(), to.as_str(), label.as_str()),
                    ("api", "auth", "POST /token")
                );
                assert!(!dashed);
            }
            other => panic!("expected message, got {other:?}"),
        }
        match &flow.steps[1] {
            FlowStep::Alt {
                name,
                then_branch,
                else_branch,
            } => {
                assert_eq!(name, "valid");
                assert_eq!(then_branch.len(), 1);
                assert_eq!(else_branch.len(), 1);
                match &then_branch[0] {
                    FlowStep::Message { dashed, label, .. } => {
                        assert!(dashed);
                        assert_eq!(label, "200 + access token");
                    }
                    other => panic!("expected message, got {other:?}"),
                }
                match &else_branch[0] {
                    FlowStep::Message { label, .. } => assert_eq!(label, "401 unauthorized"),
                    other => panic!("expected message, got {other:?}"),
                }
            }
            other => panic!("expected alt, got {other:?}"),
        }

        assert_eq!(unit.specs.len(), 1);
        let sp = &unit.specs[0];
        assert_eq!(sp.id, "AUTH-001");
        assert_eq!(sp.requirements.len(), 2);
        let r1 = &sp.requirements[0];
        assert_eq!(r1.id, "AUTH-001-R1");
        assert_eq!(r1.style.as_deref(), Some("EARS"));
        assert!(
            r1.text
                .starts_with("WHEN a service presents valid client credentials")
        );
        assert_eq!(r1.layers, vec![Layer::Unit, Layer::Contract, Layer::E2e]);
        assert_eq!(r1.scenarios, vec!["valid_token"]);
        let r2 = &sp.requirements[1];
        assert_eq!(r2.layers, vec![Layer::Unit]);
        assert_eq!(r2.scenarios, vec!["invalid_token"]);

        assert_eq!(sp.scenarios.len(), 2);
        let sc = sp.scenario("valid_token").unwrap();
        assert_eq!(sc.given, "valid client credentials");
        assert_eq!(sc.when, "a token request is made");
        assert_eq!(
            sc.then,
            "the response status is 200 and a token is returned"
        );
        assert_eq!(sp.scenarios[1].id, "invalid_token");
        assert_eq!(sp.scenarios[1].then, "the response status is 401");

        assert_eq!(sp.invariants.len(), 1);
        assert_eq!(sp.invariants[0].id, "token_ttl");
        assert_eq!(sp.invariants[0].expr, "access_token.ttl_seconds <= 3600");
        assert_eq!(sp.invariants[0].layers, vec![Layer::Unit]);

        assert_eq!(sp.infra_policies.len(), 1);
        assert_eq!(sp.infra_policies[0].id, "no_hardcoded_secrets");
        assert_eq!(
            sp.infra_policies[0].description,
            "No secrets committed in source or config"
        );
        assert_eq!(sp.infra_policies[0].layers, vec![Layer::Infra]);
    }

    #[test]
    fn demo_spec_validates_clean() {
        let unit = parse_file("specs/0001-auth.spec", DEMO_SPEC).unwrap();
        let ws = merge(vec![unit]);
        let diags = validate(&ws);
        assert_eq!(diags, Vec::new(), "unexpected diagnostics: {diags:?}");
    }

    #[test]
    fn unknown_layer_reports_file_and_line() {
        let src =
            "spec S-1 {\n  requirement S-1-R1 {\n    text: \"x\"\n    layers: [unittest]\n  }\n}\n";
        let err = parse_file("specs/bad.spec", src).unwrap_err();
        assert_eq!(err.file, "specs/bad.spec");
        assert_eq!(err.line, 4);
        assert!(
            err.message.contains("unknown layer 'unittest'"),
            "{}",
            err.message
        );
        assert!(
            err.message
                .contains("expected one of: unit, contract, e2e, infra, fitness"),
            "{}",
            err.message
        );
        assert!(err.to_string().starts_with("specs/bad.spec:4:"));
    }

    #[test]
    fn duplicate_id_reports_second_definition_line() {
        let src = concat!(
            "decision DUP-1 \"first\" {\n",
            "  status: accepted\n",
            "}\n",
            "decision DUP-1 \"second\" {\n",
            "  status: accepted\n",
            "}\n",
        );
        let unit = parse_file("specs/dup.spec", src).unwrap();
        assert_eq!(unit.decisions.len(), 2);
        let ws = merge(vec![unit]);
        let diags = validate(&ws);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 4);
        assert!(
            diags[0].message.contains("duplicate id 'DUP-1'"),
            "{}",
            diags[0].message
        );
        assert!(
            diags[0].message.contains("specs/dup.spec:1"),
            "{}",
            diags[0].message
        );
    }

    #[test]
    fn unterminated_string_reports_line() {
        let src = "decision U-1 \"never closed {\n  status: accepted\n}\n";
        let err = parse_file("specs/unterm.spec", src).unwrap_err();
        assert_eq!(err.line, 1);
        assert!(
            err.message.contains("unterminated string"),
            "{}",
            err.message
        );
    }

    #[test]
    fn unknown_scenario_ref_reports_requirement_line() {
        let src = concat!(
            "decision D-1 \"t\" {\n  status: accepted\n}\n",
            "spec D-1 {\n",
            "  requirement D-1-R1 {\n",
            "    text: \"t\"\n",
            "    layers: [unit]\n",
            "    scenarios: [ghost]\n",
            "  }\n",
            "}\n",
        );
        let unit = parse_file("specs/ref.spec", src).unwrap();
        let ws = merge(vec![unit]);
        let diags = validate(&ws);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 5);
        assert!(
            diags[0].message.contains("unknown scenario 'ghost'"),
            "{}",
            diags[0].message
        );
    }

    #[test]
    fn unknown_decision_ref_for_spec() {
        let src = "spec MISSING-9 {\n}\n";
        let unit = parse_file("specs/orphan.spec", src).unwrap();
        let ws = merge(vec![unit]);
        let diags = validate(&ws);
        assert_eq!(diags.len(), 1);
        assert!(
            diags[0].message.contains("unknown decision 'MISSING-9'"),
            "{}",
            diags[0].message
        );
    }

    #[test]
    fn undeclared_flow_participant() {
        let src = concat!(
            "model {\n",
            "  container api \"API\"\n",
            "  flow f1 {\n",
            "    api -> db: \"query\"\n",
            "  }\n",
            "}\n",
        );
        let unit = parse_file("specs/flow.spec", src).unwrap();
        let ws = merge(vec![unit]);
        let diags = validate(&ws);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 3);
        assert!(
            diags[0].message.contains("undeclared container 'db'"),
            "{}",
            diags[0].message
        );
    }

    #[test]
    fn bad_status_reports_expected_values() {
        let src = "decision S-1 \"t\" {\n  status: acceptd\n}\n";
        let err = parse_file("specs/status.spec", src).unwrap_err();
        assert_eq!(err.line, 2);
        assert!(
            err.message.contains("unknown status 'acceptd'"),
            "{}",
            err.message
        );
        assert!(
            err.message
                .contains("accepted, proposed, rejected, superseded"),
            "{}",
            err.message
        );
    }

    #[test]
    fn unknown_superseded_by_target() {
        let src = concat!(
            "decision OLD-1 \"t\" {\n",
            "  status: superseded\n",
            "  superseded_by: NEW-9\n",
            "}\n",
        );
        let unit = parse_file("specs/sup.spec", src).unwrap();
        let ws = merge(vec![unit]);
        let diags = validate(&ws);
        assert_eq!(diags.len(), 1);
        assert!(
            diags[0]
                .message
                .contains("unknown superseded_by target 'NEW-9'"),
            "{}",
            diags[0].message
        );
    }

    #[test]
    fn parses_nested_alt() {
        let src = concat!(
            "model {\n",
            "  container api \"API\"\n",
            "  container auth \"Auth\"\n",
            "  flow f1 {\n",
            "    api -> auth: \"POST /token\"\n",
            "    alt valid {\n",
            "      alt fresh {\n",
            "        auth --> api: \"200 + token\"\n",
            "      } else {\n",
            "        auth --> api: \"200 + refreshed\"\n",
            "      }\n",
            "    } else {\n",
            "      alt expired {\n",
            "        auth --> api: \"200 + refreshed\"\n",
            "      } else {\n",
            "        auth --> api: \"401 unauthorized\"\n",
            "      }\n",
            "    }\n",
            "  }\n",
            "}\n",
        );
        let unit = parse_file("specs/nested.spec", src).expect("parse failed");
        let ws = merge(vec![unit]);
        assert_eq!(validate(&ws), vec![]);

        let flow = &ws.models[0].flows[0];
        assert_eq!(flow.steps.len(), 2);
        let FlowStep::Alt {
            name,
            then_branch,
            else_branch,
        } = &flow.steps[1]
        else {
            panic!("expected alt, got {:?}", flow.steps[1]);
        };
        assert_eq!(name, "valid");

        assert_eq!(then_branch.len(), 1);
        let FlowStep::Alt {
            name,
            then_branch: inner_then,
            else_branch: inner_else,
        } = &then_branch[0]
        else {
            panic!(
                "expected nested alt in then-branch, got {:?}",
                then_branch[0]
            );
        };
        assert_eq!(name, "fresh");
        assert_eq!(inner_then.len(), 1);
        assert_eq!(inner_else.len(), 1);
        match &inner_then[0] {
            FlowStep::Message {
                from,
                to,
                label,
                dashed,
            } => {
                assert_eq!(
                    (from.as_str(), to.as_str(), label.as_str()),
                    ("auth", "api", "200 + token")
                );
                assert!(*dashed);
            }
            other => panic!("expected message, got {other:?}"),
        }
        match &inner_else[0] {
            FlowStep::Message { label, .. } => assert_eq!(label, "200 + refreshed"),
            other => panic!("expected message, got {other:?}"),
        }

        assert_eq!(else_branch.len(), 1);
        let FlowStep::Alt {
            name,
            then_branch: inner_then,
            else_branch: inner_else,
        } = &else_branch[0]
        else {
            panic!(
                "expected nested alt in else-branch, got {:?}",
                else_branch[0]
            );
        };
        assert_eq!(name, "expired");
        assert_eq!(inner_then.len(), 1);
        assert_eq!(inner_else.len(), 1);
        match &inner_else[0] {
            FlowStep::Message { label, .. } => assert_eq!(label, "401 unauthorized"),
            other => panic!("expected message, got {other:?}"),
        }
    }

    #[test]
    fn nested_alt_uses_nested_container() {
        let src = concat!(
            "model {\n",
            "  container api \"API\"\n",
            "  flow f1 {\n",
            "    api -> api: \"start\"\n",
            "    alt outer {\n",
            "      alt middle {\n",
            "        api -> db: \"write\"\n",
            "      } else {\n",
            "        api -> api: \"noop\"\n",
            "      }\n",
            "    } else {\n",
            "      api -> api: \"noop\"\n",
            "    }\n",
            "  }\n",
            "}\n",
        );
        let unit = parse_file("specs/nested.spec", src).expect("parse failed");
        let ws = merge(vec![unit]);
        let diags = validate(&ws);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].file, "specs/nested.spec");
        assert_eq!(diags[0].line, 3);
        assert_eq!(diags[0].message, "flow 'f1' uses undeclared container 'db'");
    }

    #[test]
    fn rejects_unclosed_nested_alt() {
        let missing_brace = concat!(
            "model {\n",
            "  container api \"API\"\n",
            "  flow f1 {\n",
            "    alt outer {\n",
            "      alt inner {\n",
            "        api -> api: \"x\"\n",
            "      } else {\n",
            "        api -> api: \"y\"\n",
            "      }\n",
            "    } else {\n",
            "      api -> api: \"z\"\n",
            "  }\n",
            "}\n",
        );
        let err = parse_file("specs/nested.spec", missing_brace)
            .expect_err("unclosed nested alt must not parse");
        assert!(err.message.contains('}'), "{}", err.message);

        let missing_else = concat!(
            "model {\n",
            "  container api \"API\"\n",
            "  flow f1 {\n",
            "    alt outer {\n",
            "      alt inner {\n",
            "        api -> api: \"x\"\n",
            "      }\n",
            "      api -> api: \"y\"\n",
            "    } else {\n",
            "      api -> api: \"z\"\n",
            "    }\n",
            "  }\n",
            "}\n",
        );
        let err = parse_file("specs/nested.spec", missing_else)
            .expect_err("nested alt without else must not parse");
        assert!(err.message.contains("else"), "{}", err.message);
    }
}
