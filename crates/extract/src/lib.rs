//! Bootstrap a draft `.spec` from an already-started project.
//!
//! Pure filesystem-in/report-out: `analyze` reads a project tree and reports
//! structure (containers from the directory layout, relationships from the
//! import graph) plus suggested `[stack.fitness.packages]` mappings. It does
//! NOT recover decisions/requirements — the rendered draft header says so.
//! All writes stay in `cli`.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

/// Language family detected in the analyzed tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Python,
    TypeScript,
    JavaScript,
}

/// A container candidate: one top-level directory (or the project root, when
/// loose source files live there) holding source files of the winning family.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerInfo {
    pub id: String,
    pub title: String,
    /// Python dotted path, or JS/TS relative directory path (`/` separated).
    pub module: String,
}

/// Result of analyzing a project tree.
#[derive(Debug, Clone)]
pub struct Extracted {
    pub lang: Lang,
    pub project_name: String,
    pub containers: Vec<ContainerInfo>,
    /// (from_id, to_id) import edges, sorted + deduped, no self-edges.
    pub relationships: Vec<(String, String)>,
    /// Decisions recovered from ADR markdown docs, sorted by source path.
    /// Requirements/scenarios are never inferred.
    pub decisions: Vec<ExtractedDecision>,
    pub warnings: Vec<String>,
}

/// A decision recovered from an ADR/markdown decision doc. Only explicitly
/// written facts are extracted (title/status/context/consequences);
/// requirements and `superseded_by` targets are never inferred.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedDecision {
    pub id: String,
    pub title: String,
    /// DSL status word: proposed | accepted | superseded | rejected
    /// (an ADR `deprecated` maps to `superseded`).
    pub status: String,
    /// Flattened single-line text of the `## Context` section (<= 500 chars
    /// + ellipsis), if the section exists.
    pub context: Option<String>,
    /// Same for `## Consequences`.
    pub consequences: Option<String>,
    /// Root-relative path of the ADR file, for the `# Source:` comment.
    pub source: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ExtractError {
    #[error("{}", .0.display())]
    NotADirectory(PathBuf),
    #[error("no source files (.py, .ts, .tsx, .js, .jsx, .mjs, .cjs) found under {}", .0.display())]
    NoSourceFiles(PathBuf),
    #[error("cannot read {}: {_0}", .1.display())]
    Io(#[source] std::io::Error, PathBuf),
}

/// Directories never descended into, matched by name at any depth.
const NOISE_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    "venv",
    ".venv",
    "__pycache__",
    "target",
    "dist",
    "build",
    "out",
    ".next",
    ".cache",
    "coverage",
    ".tox",
    ".mypy_cache",
    ".pytest_cache",
    "site-packages",
    ".decispec",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    Python,
    JsTs,
}

fn is_noise(name: &OsStr) -> bool {
    name.to_str().is_some_and(|s| NOISE_DIRS.contains(&s))
}

fn family_of(path: &Path) -> Option<Family> {
    match path.extension()?.to_str()? {
        "py" => Some(Family::Python),
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => Some(Family::JsTs),
        _ => None,
    }
}

fn is_ts(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e == "ts" || e == "tsx")
}

/// Recursively collect source files under `dir_abs` (as paths relative to the
/// analysis root), skipping noise directories by name and any root-relative
/// path in `exclude` (or beneath one).
fn walk(
    dir_abs: &Path,
    dir_rel: &Path,
    exclude: &[PathBuf],
    out: &mut Vec<(PathBuf, Family)>,
) -> Result<(), ExtractError> {
    let entries =
        std::fs::read_dir(dir_abs).map_err(|e| ExtractError::Io(e, dir_abs.to_path_buf()))?;
    for entry in entries {
        let entry = entry.map_err(|e| ExtractError::Io(e, dir_abs.to_path_buf()))?;
        let path = entry.path();
        let rel = dir_rel.join(entry.file_name());
        if exclude.iter().any(|e| !e.as_os_str().is_empty() && rel.starts_with(e)) {
            continue;
        }
        let ft = entry
            .file_type()
            .map_err(|e| ExtractError::Io(e, path.clone()))?;
        if ft.is_dir() {
            if is_noise(&entry.file_name()) {
                continue;
            }
            walk(&path, &rel, exclude, out)?;
        } else if ft.is_file()
            && let Some(fam) = family_of(&path)
        {
            out.push((rel, fam));
        }
    }
    Ok(())
}

/// Analyze `root`: containers from top-level directories — or, when a
/// single umbrella directory holds the code, from its children —
/// relationships from the import graph of the winning language family.
/// `exclude` lists root-relative paths (dirs or files) to skip — e.g.
/// DecisionSpec's own test scaffold — matched the same way noise dirs are.
pub fn analyze(root: &Path, exclude: &[PathBuf]) -> Result<Extracted, ExtractError> {
    if !root.is_dir() {
        return Err(ExtractError::NotADirectory(root.to_path_buf()));
    }
    let mut files: Vec<(PathBuf, Family)> = Vec::new();
    walk(root, Path::new(""), exclude, &mut files)?;

    let mut py = 0usize;
    let mut js = 0usize;
    let mut ts_seen = false;
    for (rel, fam) in &files {
        match fam {
            Family::Python => py += 1,
            Family::JsTs => {
                js += 1;
                ts_seen |= is_ts(rel);
            }
        }
    }
    if py + js == 0 {
        return Err(ExtractError::NoSourceFiles(root.to_path_buf()));
    }
    // More files wins; a tie breaks to Python. Within the JS family the
    // presence of any .ts/.tsx makes the project TypeScript.
    let lang = if py >= js {
        Lang::Python
    } else if ts_seen {
        Lang::TypeScript
    } else {
        Lang::JavaScript
    };
    let winning = if lang == Lang::Python {
        Family::Python
    } else {
        Family::JsTs
    };

    let project_name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "my-project".to_string());

    // Containers: every top-level directory holding >=1 winning-family file,
    // plus one project-named container for loose files directly in root.
    //
    // Single-umbrella transparency: when exactly ONE depth-1 dir holds the
    // code and it has >=2 child dirs with winning files (src/{pages,lib},
    // shop/{auth,payments}), the umbrella itself says nothing — promote its
    // children to containers and keep files sitting directly under it in an
    // umbrella-named container. Two or more depth-1 dirs stay as-is; the
    // exclusions applied during the walk already kept the scaffold out.
    let mut dirs: BTreeSet<String> = BTreeSet::new();
    let mut has_loose = false;
    for (rel, fam) in &files {
        if *fam != winning {
            continue;
        }
        let mut comps = rel.components();
        match (comps.next(), comps.next()) {
            (Some(Component::Normal(d)), Some(_)) => {
                dirs.insert(d.to_string_lossy().into_owned());
            }
            _ => has_loose = true,
        }
    }

    struct Pending {
        title: String,
        module: String,
        /// Root-relative directory this container owns (empty = loose root).
        dir: PathBuf,
        loose: bool,
    }
    let mut pending: Vec<Pending> = Vec::new();
    let umbrella: Option<(String, BTreeSet<String>, bool)> = (|| {
        if dirs.len() != 1 {
            return None;
        }
        let u = dirs.iter().next().expect("one dir").clone();
        let mut children: BTreeSet<String> = BTreeSet::new();
        let mut u_direct = false;
        for (rel, fam) in &files {
            if *fam != winning {
                continue;
            }
            let comps: Vec<Component> = rel.components().collect();
            match comps.as_slice() {
                [Component::Normal(a), Component::Normal(child), _, ..]
                    if *a == std::ffi::OsStr::new(&u) =>
                {
                    children.insert(child.to_string_lossy().into_owned());
                }
                [Component::Normal(a), Component::Normal(_file)] if *a == std::ffi::OsStr::new(&u) => {
                    u_direct = true;
                }
                _ => {}
            }
        }
        (children.len() >= 2).then_some((u, children, u_direct))
    })();
    match &umbrella {
        Some((u, children, u_direct)) => {
            for child in children {
                pending.push(Pending {
                    title: child.clone(),
                    module: if lang == Lang::Python {
                        format!("{u}.{child}")
                    } else {
                        format!("{u}/{child}")
                    },
                    dir: PathBuf::from(u).join(child),
                    loose: false,
                });
            }
            if *u_direct {
                pending.push(Pending {
                    title: u.clone(),
                    module: u.clone(),
                    dir: PathBuf::from(u),
                    loose: false,
                });
            }
        }
        None => {
            // A depth-1 dir path doubles as the module: dotted (python) or
            // slash-separated (js/ts) — for depth 1 both are the bare name.
            for d in &dirs {
                pending.push(Pending {
                    title: d.clone(),
                    module: d.clone(),
                    dir: PathBuf::from(d),
                    loose: false,
                });
            }
        }
    }
    if has_loose {
        pending.push(Pending {
            title: project_name.clone(),
            module: project_name.clone(),
            dir: PathBuf::new(),
            loose: true,
        });
    }
    pending.sort_by(|a, b| a.title.cmp(&b.title));

    let mut containers: Vec<ContainerInfo> = Vec::new();
    let mut by_dir: HashMap<String, String> = HashMap::new();
    let mut by_path: Vec<(PathBuf, String)> = Vec::new();
    let mut loose_id: Option<String> = None;
    let mut used: HashSet<String> = HashSet::new();
    for p in pending {
        let base = container_id_base(&p.title);
        let mut id = base.clone();
        let mut n = 2;
        while !used.insert(id.clone()) {
            id = format!("{base}_{n}");
            n += 1;
        }
        if p.loose {
            loose_id = Some(id.clone());
        } else {
            by_dir.insert(p.title.clone(), id.clone());
            by_path.push((p.dir, id.clone()));
        }
        containers.push(ContainerInfo {
            id,
            title: p.title,
            module: p.module,
        });
    }

    // The importing file's container is the one owning its parent dir
    // (deepest prefix), same resolution js relative imports use.
    let importer_of = |rel: &Path| -> String {
        let parent = rel.parent().unwrap_or(Path::new(""));
        js_owner(parent, &by_path, loose_id.as_deref())
            .expect("every winning file lives in a container")
    };

    let mut edges: BTreeSet<(String, String)> = BTreeSet::new();
    let mut unmapped = 0usize;
    for (rel, fam) in &files {
        if *fam != winning {
            continue;
        }
        let importer = importer_of(rel);
        let abs = root.join(rel);
        let src =
            std::fs::read_to_string(&abs).map_err(|e| ExtractError::Io(e, abs.clone()))?;
        for line in src.lines() {
            if lang == Lang::Python {
                for u in python_uses(line) {
                    match u {
                        PyUse::Relative => {}
                        PyUse::Absolute(first) => match by_dir.get(&first) {
                            Some(to) => {
                                edges.insert((importer.clone(), to.clone()));
                            }
                            None => unmapped += 1,
                        },
                    }
                }
            } else {
                for spec in js_specifiers(line) {
                    // Bare specifiers (react, lodash) and aliases (@/...) are
                    // dropped; relative ones resolve to the owning container.
                    if !spec.starts_with('.') {
                        unmapped += 1;
                        continue;
                    }
                    let parent = rel.parent().unwrap_or(Path::new(""));
                    let Some(target) = normalize_path(&parent.join(&spec)) else {
                        unmapped += 1;
                        continue;
                    };
                    match js_owner(&target, &by_path, loose_id.as_deref()) {
                        Some(to) => {
                            edges.insert((importer.clone(), to));
                        }
                        None => unmapped += 1,
                    }
                }
            }
        }
    }

    // -- ADR ingestion -------------------------------------------------------

    let mut decisions: Vec<ExtractedDecision> = Vec::new();
    let mut skipped_no_title = 0usize;
    let mut missing_status = 0usize;
    for (rel, text) in scan_adrs(root, exclude)? {
        let Some(title) = adr_title(&text) else {
            skipped_no_title += 1;
            continue;
        };
        let base = adr_id_base(&rel);
        let mut id = base.clone();
        let mut n = 2;
        while !used.insert(id.clone()) {
            id = format!("{base}_{n}");
            n += 1;
        }
        let status = match adr_status(&text) {
            Some("deprecated") => "superseded".to_string(),
            Some(s) => s.to_string(),
            None => {
                missing_status += 1;
                "proposed".to_string()
            }
        };
        decisions.push(ExtractedDecision {
            id,
            title,
            status,
            context: adr_section(&text, "context"),
            consequences: adr_section(&text, "consequences"),
            source: rel.to_string_lossy().replace('\\', "/"),
        });
    }

    let mut warnings = Vec::new();
    if unmapped > 0 {
        warnings.push(format!("{unmapped} imports did not map to any container"));
    }
    if containers.len() > 30 {
        warnings.push(format!(
            "{} containers found — consider grouping related directories into fewer, higher-level containers",
            containers.len()
        ));
    }
    if skipped_no_title > 0 {
        warnings.push(format!("{skipped_no_title} ADR file(s) skipped (no # title)"));
    }
    if missing_status > 0 {
        warnings.push(format!(
            "{missing_status} ADR(s) missing status (defaulted to proposed)"
        ));
    }

    let relationships: Vec<(String, String)> =
        edges.into_iter().filter(|(from, to)| from != to).collect();
    Ok(Extracted {
        lang,
        project_name,
        containers,
        relationships,
        decisions,
        warnings,
    })
}

/// Render the draft `.spec` text: recovered decisions first (each with its
/// `# Source:` comment), then the `model` block, then the review header.
pub fn render_spec(extracted: &Extracted) -> String {
    let mut out = String::new();
    out.push_str(
        "# DRAFT from `decispec extract` — decisions from ADR docs, containers = top-level dirs, rels = import graph.\n",
    );
    out.push_str("# Review names, then add requirements (never inferred). Delete this header.\n");
    for d in &extracted.decisions {
        out.push_str(&format!("# Source: {}\n", d.source));
        out.push_str(&format!("decision {} \"{}\" {{\n", d.id, escape_title(&d.title)));
        out.push_str(&format!("  status: {}\n", d.status));
        // The DSL requires a superseded_by target for status superseded; the
        // real target is not written in the ADR, so point at the decision
        // itself to keep the draft valid and flag the gap for review.
        if d.status == "superseded" {
            out.push_str(
                "  # TODO: status was \"superseded\"/\"deprecated\" in the ADR; no successor determinable.\n",
            );
            out.push_str(
                "  # Point superseded_by at the real successor ID, then delete these comment lines.\n",
            );
            out.push_str(&format!("  superseded_by: {}\n", d.id));
        }
        if let Some(ctx) = &d.context {
            out.push_str(&format!("  context: \"{}\"\n", escape_title(ctx)));
        }
        if let Some(cons) = &d.consequences {
            out.push_str(&format!("  consequences: \"{}\"\n", escape_title(cons)));
        }
        out.push_str("}\n");
    }
    out.push_str("model {\n");
    for c in &extracted.containers {
        out.push_str(&format!(
            "  container {} \"{}\"\n",
            c.id,
            escape_title(&c.title)
        ));
    }
    for (from, to) in &extracted.relationships {
        out.push_str(&format!("  rel {from} -> {to}: \"{from} imports {to}\"\n"));
    }
    out.push_str("}\n");
    out
}

/// Render a suggested `[stack.fitness]` TOML snippet to merge into
/// `decispec.toml`.
pub fn render_fitness_toml(extracted: &Extracted) -> String {
    let mut out = String::new();
    out.push_str("# Suggested — merge into decispec.toml\n");
    out.push_str("[stack.fitness]\n");
    match extracted.lang {
        Lang::Python => {
            // Project dirs may contain hyphens etc.; a python package name
            // must be importable, so scrub to [A-Za-z0-9_.] (dots stay:
            // these are dotted paths).
            out.push_str(&format!(
                "root_package = \"{}\"\n",
                python_dotted(&extracted.project_name)
            ));
        }
        _ => {
            out.push_str(
                "# root_package is python-only; for a js/ts project point dependency-cruiser at your source dir.\n",
            );
            out.push_str(
                "# note: lang = \"js\" glue codegen is not yet supported — fitness (dependency-cruiser) is the useful part today.\n",
            );
        }
    }
    out.push('\n');
    out.push_str("[stack.fitness.packages]\n");
    for c in &extracted.containers {
        let value = match extracted.lang {
            // The root-level (loose files) container IS the package root.
            Lang::Python if c.module != extracted.project_name => python_dotted(&format!(
                "{}.{}",
                extracted.project_name, c.module
            )),
            Lang::Python => python_dotted(&c.module),
            _ => c.module.clone(),
        };
        out.push_str(&format!("{} = \"{value}\"\n", c.id));
    }
    out
}

/// Scrub a dotted python path to what an interpreter can import: anything
/// outside `[A-Za-z0-9_.]` becomes `_` (project dirs like `my-shop` are
/// legal on disk but not importable).
fn python_dotted(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Container ids must be valid DSL ids ([A-Za-z0-9_-]+, see README) or the
/// rendered draft fails `decispec check`. `ir::names::sanitize` only maps
/// `-` → `_` (it is the test-name registry, shared with codegen/gate), so
/// extract scrubs any other out-of-charset character to `_` first, then
/// applies `sanitize` on top. Titles are untouched — they are quoted and
/// escaped in the render.
fn container_id_base(title: &str) -> String {
    let scrubbed: String = title
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    decispec_ir::names::sanitize(&scrubbed)
}

/// Textually normalize `.`/`..` out of a relative path; `None` when the path
/// would escape the root.
fn normalize_path(p: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir | Component::RootDir | Component::Prefix(_) => {}
            Component::Normal(s) => out.push(s),
            Component::ParentDir => {
                if !out.pop() {
                    return None;
                }
            }
        }
    }
    Some(out)
}

/// The container that owns `target` (a root-relative path): the deepest
/// container directory that is an ancestor of (or equal to) it, else the
/// loose root container.
fn js_owner(target: &Path, by_path: &[(PathBuf, String)], loose: Option<&str>) -> Option<String> {
    let mut best: Option<&(PathBuf, String)> = None;
    for c in by_path {
        if target == c.0 || target.starts_with(&c.0) {
            best = match best {
                Some(b) if b.0.components().count() >= c.0.components().count() => best,
                _ => Some(c),
            };
        }
    }
    match best {
        Some(b) => Some(b.1.clone()),
        None => loose.map(str::to_string),
    }
}

// ---------------------------------------------------------------------------
// ADR ingestion (F15): markdown decision docs -> decision blocks
// ---------------------------------------------------------------------------

/// Canonical ADR locations, resolved root-relative, case-insensitively.
const ADR_PATTERNS: [&[&str]; 4] = [
    &["adr"],
    &["decisions"],
    &["docs", "adr"],
    &["docs", "decisions"],
];

/// Read every `*.md` under the canonical ADR dirs (recursively, skipping
/// noise + excluded paths), as (root-relative path, text) sorted by path.
fn scan_adrs(root: &Path, exclude: &[PathBuf]) -> Result<Vec<(PathBuf, String)>, ExtractError> {
    let mut found: Vec<PathBuf> = Vec::new();
    for pattern in ADR_PATTERNS {
        let mut abs = root.to_path_buf();
        let mut ok = true;
        for comp in pattern {
            match resolve_dir_ci(&abs, comp) {
                Some(a) => abs = a,
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            collect_adrs(&abs, root, exclude, &mut found)?;
        }
    }
    found.sort();
    let mut out = Vec::new();
    for rel in found {
        let abs = root.join(&rel);
        let text = std::fs::read_to_string(&abs).map_err(|e| ExtractError::Io(e, abs))?;
        out.push((rel, text));
    }
    Ok(out)
}

/// Find `wanted` (case-insensitive) among `dir`'s subdirectories.
fn resolve_dir_ci(dir: &Path, wanted: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let name_match = entry.file_name().to_str().is_some_and(|n| n.eq_ignore_ascii_case(wanted));
        if name_match && entry.file_type().is_ok_and(|t| t.is_dir()) {
            return Some(entry.path());
        }
    }
    None
}

/// Recursively collect `*.md` files under `dir_abs` as root-relative paths.
fn collect_adrs(
    dir_abs: &Path,
    root: &Path,
    exclude: &[PathBuf],
    out: &mut Vec<PathBuf>,
) -> Result<(), ExtractError> {
    let entries =
        std::fs::read_dir(dir_abs).map_err(|e| ExtractError::Io(e, dir_abs.to_path_buf()))?;
    for entry in entries {
        let entry = entry.map_err(|e| ExtractError::Io(e, dir_abs.to_path_buf()))?;
        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_path_buf();
        if exclude
            .iter()
            .any(|e| !e.as_os_str().is_empty() && rel.starts_with(e))
        {
            continue;
        }
        let ft = entry
            .file_type()
            .map_err(|e| ExtractError::Io(e, path.clone()))?;
        if ft.is_dir() {
            if is_noise(&entry.file_name()) {
                continue;
            }
            collect_adrs(&path, root, exclude, out)?;
        } else if ft.is_file() && path.extension().is_some_and(|e| e == "md") {
            out.push(rel);
        }
    }
    Ok(())
}

/// Title = text of the first `#` heading; `None` when there is none.
fn adr_title(text: &str) -> Option<String> {
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("# ") {
            let title = rest.trim();
            return if title.is_empty() { None } else { Some(title.to_string()) };
        }
        if t.starts_with("##") {
            break; // reached section headings without ever seeing a title
        }
    }
    None
}

/// ID base: `ADR-<n>` for a numeric filename prefix (`0001-foo.md`,
/// `0001.md`), else an uppercase, charset-safe slug of the stem.
fn adr_id_base(rel: &Path) -> String {
    let stem = rel.file_stem().map(|s| s.to_string_lossy()).unwrap_or_default();
    let digits: String = stem.chars().take_while(|c| c.is_ascii_digit()).collect();
    if !digits.is_empty() {
        format!("ADR-{digits}")
    } else {
        stem.split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-")
            .to_uppercase()
    }
}

const ADR_STATUSES: [&str; 5] = ["proposed", "accepted", "deprecated", "superseded", "rejected"];

/// First whitespace token of `s`, stripped of surrounding punctuation,
/// matched against the DSL status words.
fn first_status_word(s: &str) -> Option<&'static str> {
    let token: String = s.chars().take_while(|c| !c.is_whitespace()).collect();
    let token = token.trim_matches(|c: char| c.is_ascii_punctuation());
    ADR_STATUSES
        .iter()
        .find(|w| token.eq_ignore_ascii_case(w))
        .copied()
}

/// Status from a MADR `## Status` section (first status word on the next
/// non-empty line) or an inline `Status:` / `**Status:**` line.
fn adr_status(text: &str) -> Option<&'static str> {
    let lines: Vec<&str> = text.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if line.trim().eq_ignore_ascii_case("## status") {
            for next in &lines[i + 1..] {
                let n = next.trim();
                if n.is_empty() {
                    continue;
                }
                if let Some(w) = first_status_word(n) {
                    return Some(w);
                }
                break;
            }
            break;
        }
    }
    for line in &lines {
        let t = line.trim();
        let rest = t
            .strip_prefix("**Status:**")
            .or_else(|| t.strip_prefix("**status:**"))
            .or_else(|| t.strip_prefix("Status:"))
            .or_else(|| t.strip_prefix("status:"));
        if let Some(rest) = rest
            && let Some(w) = first_status_word(rest.trim())
        {
            return Some(w);
        }
    }
    None
}

/// Flattened single-line text of a `## <name>` section (stops at the next
/// heading), capped at 500 chars + an ellipsis. `None` if the section is
/// absent or empty.
fn adr_section(text: &str, name: &str) -> Option<String> {
    let mut in_section = false;
    let mut parts: Vec<&str> = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('#') {
            let heading = t.trim_start_matches('#').trim();
            if in_section {
                break; // any next heading ends the section
            }
            if heading.eq_ignore_ascii_case(name) {
                in_section = true;
            }
            continue;
        }
        if in_section && !t.is_empty() {
            parts.push(t);
        }
    }
    if parts.is_empty() {
        return None;
    }
    let flat = parts.join(" ");
    let flat: String = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > 500 {
        Some(format!("{}…", flat.chars().take(500).collect::<String>()))
    } else {
        Some(flat)
    }
}

// ---------------------------------------------------------------------------
// Import-line parsing (anchored, comment lines naturally excluded)
// ---------------------------------------------------------------------------

/// One import reference parsed from a Python source line.
#[derive(Debug, Clone, PartialEq, Eq)]
enum PyUse {
    /// `from . import x` / `from .mod import x` — resolves to the importing
    /// file's own container (a self-edge, which is dropped).
    Relative,
    /// Absolute module; only the first dotted segment is kept for mapping.
    Absolute(String),
}

fn python_uses(line: &str) -> Vec<PyUse> {
    // `#` comments: a comment line does not start with import/from after the
    // cut, and trailing comments are dropped before parsing.
    let line = match line.find('#') {
        Some(i) => &line[..i],
        None => line,
    };
    let t = line.trim_start();
    if let Some(rest) = t.strip_prefix("from ") {
        let Some((module, _names)) = rest.split_once(" import ") else {
            return vec![];
        };
        let module = module.trim();
        if module.starts_with('.') {
            return vec![PyUse::Relative];
        }
        let first = module.split('.').next().unwrap_or(module);
        return if first.is_empty() {
            vec![]
        } else {
            vec![PyUse::Absolute(first.to_string())]
        };
    }
    if let Some(rest) = t.strip_prefix("import ") {
        return rest
            .split(',')
            .filter_map(|m| {
                let name = m.split_whitespace().next()?;
                let first = name.split('.').next()?;
                if first.is_empty() {
                    None
                } else {
                    Some(PyUse::Absolute(first.to_string()))
                }
            })
            .collect();
    }
    vec![]
}

/// Specifiers parsed from a JS/TS source line (`import ... from 'x'`,
/// `import 'x'`, `require('x')`, `import('x')`).
fn js_specifiers(line: &str) -> Vec<String> {
    let t = line.trim_start();
    let mut out = Vec::new();
    let is_static_import = t.starts_with("import") && !t["import".len()..].starts_with('(');
    if is_static_import {
        if let Some(idx) = t.find(" from ") {
            if let Some(spec) = read_quoted(&t[idx + " from ".len()..]) {
                out.push(spec);
            }
        } else if let Some(spec) = read_quoted(&t["import".len()..]) {
            out.push(spec);
        }
        return out;
    }
    for needle in ["require(", "import("] {
        let mut from = 0;
        while let Some(idx) = t[from..].find(needle) {
            let after = &t[from + idx + needle.len()..];
            if let Some(spec) = read_quoted(after) {
                out.push(spec);
            }
            from += idx + needle.len();
        }
    }
    out
}

/// Read a single/double-quoted string at the start of `s`.
fn read_quoted(s: &str) -> Option<String> {
    let s = s.trim_start();
    let q = s.chars().next()?;
    if q != '\'' && q != '"' {
        return None;
    }
    let rest = &s[q.len_utf8()..];
    let end = rest.find(q)?;
    Some(rest[..end].to_string())
}

/// Escape a container title for the `.spec` string literal (`\"` / `\\`).
fn escape_title(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    for c in title.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_project(name: &str) -> PathBuf {
        // Uniqueness comes from the counter: nanosecond clocks can collide
        // between parallel tests, the counter cannot.
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let base = std::env::temp_dir().join(format!(
            "decispec-extract-test-{}-{n}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join(name);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn write(root: &Path, rel: &str, content: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    fn ci(id: &str, title: &str, module: &str) -> ContainerInfo {
        ContainerInfo {
            id: id.to_string(),
            title: title.to_string(),
            module: module.to_string(),
        }
    }

    // -- 1. Python fixture: containers + rels + stdlib/comment handling ------

    #[test]
    fn analyze_python_project_containers_and_rels() {
        let root = temp_project("myapp");
        write(&root, "api/__init__.py", "from . import views\n");
        write(
            &root,
            "api/views.py",
            "import auth\nimport os\n# import fake\nfrom util import helpers\nfrom . import middleware\n",
        );
        write(&root, "auth/__init__.py", "");
        write(&root, "auth/service.py", "from api.views import handler\n");
        write(&root, "util/__init__.py", "");
        write(&root, "util/helpers.py", "import json\n");

        let ex = analyze(&root, &[]).unwrap();
        assert_eq!(ex.lang, Lang::Python);
        assert_eq!(ex.project_name, "myapp");
        assert_eq!(
            ex.containers,
            vec![ci("api", "api", "api"), ci("auth", "auth", "auth"), ci("util", "util", "util")]
        );
        assert_eq!(
            ex.relationships,
            vec![
                ("api".to_string(), "auth".to_string()),
                ("api".to_string(), "util".to_string()),
                ("auth".to_string(), "api".to_string()),
            ]
        );
        // `os` and `json` are stdlib: they map to no container and land in
        // the single unmapped-imports summary (with a fixed count, not noise).
        assert_eq!(
            ex.warnings,
            vec!["2 imports did not map to any container".to_string()]
        );

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    // -- 2a. JS/TS umbrella fixture: src/ is transparent --------------------

    #[test]
    fn analyze_ts_umbrella_project_promotes_src_children() {
        // src/ is the classic single umbrella: it holds all the code and has
        // >=2 child dirs with files, so its children become the containers
        // and the relative imports turn into real rels. Loose files directly
        // under src/ stay in an `src` container.
        let root = temp_project("tsapp");
        write(&root, "src/pages/index.ts", "import { helper } from '../lib/helper';\n");
        write(&root, "src/lib/helper.ts", "import '../pages';\n");
        write(&root, "src/index.ts", "import './lib/helper';\n");
        write(&root, "node_modules/dep/index.js", "import '../../src/lib/util';\n");

        let ex = analyze(&root, &[]).unwrap();
        assert_eq!(ex.lang, Lang::TypeScript);
        assert_eq!(
            ex.containers,
            vec![
                ci("lib", "lib", "src/lib"),
                ci("pages", "pages", "src/pages"),
                ci("src", "src", "src"),
            ]
        );
        assert_eq!(
            ex.relationships,
            vec![
                ("lib".to_string(), "pages".to_string()),
                ("pages".to_string(), "lib".to_string()),
                ("src".to_string(), "lib".to_string()),
            ]
        );
        assert!(ex.warnings.is_empty(), "warnings: {:?}", ex.warnings);
        // node_modules must stay invisible even in umbrella mode.
        assert!(
            ex.containers.iter().all(|c| c.title != "dep"),
            "node_modules leaked into containers: {:?}",
            ex.containers
        );
        let text = render_spec(&ex);
        let unit = decispec_parse::parse_file("extracted.spec", &text).unwrap();
        let ws = decispec_parse::merge(vec![unit]);
        assert!(
            decispec_parse::validate(&ws).is_empty(),
            "{:?}",
            decispec_parse::validate(&ws)
        );

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    // -- 2b. JS/TS fixture, top-level dirs: relative imports map both ways ---

    #[test]
    fn analyze_ts_project_maps_relative_imports_between_top_level_dirs() {
        let root = temp_project("tsapp2");
        write(
            &root,
            "pages/index.ts",
            "import { helper } from '../lib/helper';\nconst _ = require('react');\n",
        );
        write(
            &root,
            "lib/helper.ts",
            "import '../pages';\nimport('./lazy');\nimport x from '@/components/x';\n",
        );
        write(&root, "lib/util.js", "require('lodash');\n");
        write(&root, "node_modules/dep/index.js", "import '../lib/util';\n");

        let ex = analyze(&root, &[]).unwrap();
        assert_eq!(ex.lang, Lang::TypeScript);
        assert_eq!(ex.containers, vec![ci("lib", "lib", "lib"), ci("pages", "pages", "pages")]);
        assert_eq!(
            ex.relationships,
            vec![
                ("lib".to_string(), "pages".to_string()),
                ("pages".to_string(), "lib".to_string()),
            ]
        );
        // react + lodash (bare) and @/components/x (alias) are dropped; the
        // dynamic import('./lazy') resolves to lib itself (self-edge, gone).
        assert_eq!(
            ex.warnings,
            vec!["3 imports did not map to any container".to_string()]
        );

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    // -- 3. ID sanitization + collision suffixing ----------------------------

    #[test]
    fn container_ids_sanitize_and_dedupe_with_numeric_suffixes() {
        let root = temp_project("collide");
        write(&root, "a-b/x.py", "");
        write(&root, "a_b/y.py", "");

        let ex = analyze(&root, &[]).unwrap();
        // '-' (0x2D) sorts before '_' (0x5F), so `a-b` wins the bare id.
        assert_eq!(
            ex.containers,
            vec![ci("a_b", "a-b", "a-b"), ci("a_b_2", "a_b", "a_b")]
        );
        assert!(ex.relationships.is_empty());
        assert!(ex.warnings.is_empty());

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    // -- Review round 1 (F8): ids must stay inside the DSL id charset --------

    #[test]
    fn container_ids_scrub_chars_outside_the_id_charset() {
        // A directory named `my pages` must not render `container my pages`,
        // which the lexer rejects (ids match [A-Za-z0-9_-]+). Titles keep the
        // raw dir name; only ids are scrubbed.
        let root = temp_project("scrubby");
        write(&root, "my pages/index.ts", "import { h } from '../lib/h';\n");
        write(&root, "lib/h.ts", "");

        let ex = analyze(&root, &[]).unwrap();
        assert_eq!(
            ex.containers,
            vec![
                ci("lib", "lib", "lib"),
                ci("my_pages", "my pages", "my pages"),
            ]
        );
        // The scrubbed id flows into rels and the rendered draft.
        assert_eq!(
            ex.relationships,
            vec![("my_pages".to_string(), "lib".to_string())]
        );
        let text = render_spec(&ex);
        assert!(text.contains("  container my_pages \"my pages\"\n"), "{text}");
        assert!(text.contains("  rel my_pages -> lib: \"my_pages imports lib\"\n"), "{text}");
        let unit = decispec_parse::parse_file("extracted.spec", &text)
            .expect("draft with scrubbed ids must parse");
        let ws = decispec_parse::merge(vec![unit]);
        assert!(
            decispec_parse::validate(&ws).is_empty(),
            "draft with scrubbed ids must validate: {:?}",
            decispec_parse::validate(&ws)
        );

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    #[test]
    fn scrubbed_id_collisions_still_get_distinct_suffixes() {
        // `a b`, `a-b`, `a.b` and `a_b` all collapse onto base `a_b`; the
        // collision pass must still disambiguate them (sorted by raw title).
        let root = temp_project("collide4");
        write(&root, "a b/w.py", "");
        write(&root, "a-b/x.py", "");
        write(&root, "a.b/y.py", "");
        write(&root, "a_b/z.py", "");

        let ex = analyze(&root, &[]).unwrap();
        let ids: Vec<&str> = ex.containers.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, vec!["a_b", "a_b_2", "a_b_3", "a_b_4"]);
        assert_eq!(
            ex.containers
                .iter()
                .map(|c| c.title.as_str())
                .collect::<Vec<_>>(),
            vec!["a b", "a-b", "a.b", "a_b"]
        );

        let text = render_spec(&ex);
        let unit = decispec_parse::parse_file("extracted.spec", &text).unwrap();
        let ws = decispec_parse::merge(vec![unit]);
        assert!(
            decispec_parse::validate(&ws).is_empty(),
            "colliding scrubbed ids must stay unique and valid: {:?}",
            decispec_parse::validate(&ws)
        );

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    // -- 4. Import-line parser unit tests ------------------------------------

    #[test]
    fn python_import_forms() {
        use PyUse::*;
        assert_eq!(python_uses("import a"), vec![Absolute("a".into())]);
        assert_eq!(python_uses("import a.b"), vec![Absolute("a".into())]);
        assert_eq!(
            python_uses("import a, b.c"),
            vec![Absolute("a".into()), Absolute("b".into())]
        );
        assert_eq!(python_uses("import a as alias"), vec![Absolute("a".into())]);
        assert_eq!(python_uses("from a import x"), vec![Absolute("a".into())]);
        assert_eq!(python_uses("from a.b import x, y"), vec![Absolute("a".into())]);
        assert_eq!(python_uses("    from a import x"), vec![Absolute("a".into())]);
        assert_eq!(python_uses("import os  # noqa"), vec![Absolute("os".into())]);
        assert_eq!(python_uses("# import fake"), vec![]);
        assert_eq!(python_uses("text = 'import a'"), vec![]);
    }

    #[test]
    fn python_relative_imports_resolve_to_own_container() {
        use PyUse::*;
        assert_eq!(python_uses("from . import x"), vec![Relative]);
        assert_eq!(python_uses("from .mod import x"), vec![Relative]);
        assert_eq!(python_uses("from .mod.sub import x"), vec![Relative]);
    }

    #[test]
    fn js_import_forms() {
        assert_eq!(js_specifiers("import x from 'mod'"), vec!["mod".to_string()]);
        assert_eq!(
            js_specifiers("import { a, b } from \"mod\""),
            vec!["mod".to_string()]
        );
        assert_eq!(js_specifiers("import './side'"), vec!["./side".to_string()]);
        assert_eq!(js_specifiers("  import './side';"), vec!["./side".to_string()]);
        assert_eq!(
            js_specifiers("const m = require('mod')"),
            vec!["mod".to_string()]
        );
        assert_eq!(
            js_specifiers("let m = require(\"mod\");"),
            vec!["mod".to_string()]
        );
        assert_eq!(
            js_specifiers("const lazy = await import('./lazy')"),
            vec!["./lazy".to_string()]
        );
        assert_eq!(js_specifiers("const x = 1;"), Vec::<String>::new());
        assert_eq!(js_specifiers("// import x from 'mod'"), Vec::<String>::new());
    }

    // -- 5. Noise-dir exclusion ----------------------------------------------

    #[test]
    fn noise_dirs_are_never_descended_into() {
        let root = temp_project("noisy");
        // Each noise dir holds a .ts file; if any were counted, the js family
        // would beat python (1 file) and flip the detected language.
        for d in [
            "node_modules",
            ".git",
            "venv",
            ".venv",
            "__pycache__",
            "target",
            "dist",
            "build",
            "out",
            ".next",
            ".cache",
            "coverage",
            ".tox",
            ".mypy_cache",
            ".pytest_cache",
            "site-packages",
            ".decispec",
        ] {
            write(&root, &format!("{d}/pkg/mod.ts"), "export {};\n");
        }
        // Noise nested inside a real package must not count either.
        write(&root, "real/vendor/venv/inner.py", "import sys\n");
        write(&root, "real/main.py", "import real\n");
        write(&root, "real/mod.py", "");

        let ex = analyze(&root, &[]).unwrap();
        assert_eq!(ex.lang, Lang::Python);
        assert_eq!(ex.containers, vec![ci("real", "real", "real")]);
        assert!(ex.relationships.is_empty(), "rels: {:?}", ex.relationships);
        assert!(ex.warnings.is_empty(), "warnings: {:?}", ex.warnings);

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    // -- Review round 1 (F8): caller-supplied exclusions (DecisionSpec scaffold) --

    #[test]
    fn analyze_with_exclusions_skips_scaffold_dirs() {
        // What `decispec init` + `gen` write into a project: without
        // exclusions the 2 glue .py files tie the 2 .ts files and the
        // tie-break flips the whole draft to a lone `tests` container.
        let root = temp_project("excl");
        write(&root, "pages/index.ts", "import { h } from '../lib/h';\n");
        write(&root, "lib/h.ts", "");
        write(&root, "tests/glue/__init__.py", "");
        write(&root, "tests/glue/conftest.py", "");
        write(&root, "tests/generated/unit/test_x.py", "");

        let ex = analyze(
            &root,
            &[
                PathBuf::from("tests/glue"),
                PathBuf::from("tests/generated"),
            ],
        )
        .unwrap();
        assert_eq!(ex.lang, Lang::TypeScript);
        assert_eq!(
            ex.containers,
            vec![ci("lib", "lib", "lib"), ci("pages", "pages", "pages")]
        );
        assert_eq!(
            ex.relationships,
            vec![("pages".to_string(), "lib".to_string())]
        );
        assert!(
            ex.containers.iter().all(|c| c.title != "tests"),
            "scaffold dirs must not become containers: {:?}",
            ex.containers
        );

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    // -- 6. Round-trip through the real parser -------------------------------

    #[test]
    fn rendered_draft_parses_and_validates_with_the_real_parser() {
        let root = temp_project("roundtrip");
        write(&root, "api/__init__.py", "");
        write(&root, "api/views.py", "import auth\n");
        write(&root, "auth/__init__.py", "");
        write(&root, "auth/service.py", "");

        let ex = analyze(&root, &[]).unwrap();
        let text = render_spec(&ex);
        // parse_file is pure (file name is only used for diagnostics), so the
        // round-trip goes straight through the real parser + validator.
        let unit = decispec_parse::parse_file("extracted.spec", &text)
            .expect("rendered draft must parse");
        let ws = decispec_parse::merge(vec![unit]);
        let diags = decispec_parse::validate(&ws);
        assert!(diags.is_empty(), "rendered draft must validate: {diags:?}");
        assert_eq!(ws.models.len(), 1);
        assert_eq!(ws.models[0].containers.len(), 2);
        assert_eq!(ws.models[0].rels.len(), 1);

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    #[test]
    fn render_spec_shows_header_containers_and_rels() {
        let root = temp_project("renderme");
        write(&root, "api/x.py", "import auth\n");
        write(&root, "auth/y.py", "");

        let ex = analyze(&root, &[]).unwrap();
        let text = render_spec(&ex);
        assert!(text.starts_with("# DRAFT from `decispec extract`"));
        assert!(text.contains("model {\n"));
        assert!(text.contains("  container api \"api\"\n"));
        assert!(text.contains("  container auth \"auth\"\n"));
        assert!(text.contains("  rel api -> auth: \"api imports auth\"\n"));
        assert!(text.trim_end().ends_with('}'));

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    #[test]
    fn title_escaping_handles_quotes_and_backslashes() {
        assert_eq!(escape_title("auth"), "auth");
        assert_eq!(escape_title("a\"b"), "a\\\"b");
        assert_eq!(escape_title("a\\b"), "a\\\\b");
    }

    // -- 7. Loose root files land in a project-named container ---------------

    #[test]
    fn loose_root_py_files_land_in_a_project_named_container() {
        let root = temp_project("looseproj");
        write(&root, "main.py", "import pkg\n");
        write(&root, "core.py", "");
        write(&root, "pkg/__init__.py", "");
        write(&root, "pkg/mod.py", "import core\n");

        let ex = analyze(&root, &[]).unwrap();
        assert_eq!(
            ex.containers,
            vec![
                ci("looseproj", "looseproj", "looseproj"),
                ci("pkg", "pkg", "pkg"),
            ]
        );
        assert_eq!(
            ex.relationships,
            vec![("looseproj".to_string(), "pkg".to_string())]
        );
        // Loose modules do not map by directory name, so `import core` from
        // pkg/mod.py is unmapped (containers map top-level dirs only).
        assert_eq!(
            ex.warnings,
            vec!["1 imports did not map to any container".to_string()]
        );

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    // -- Review round 2 (F8): single-umbrella transparency -------------------

    #[test]
    fn analyze_python_umbrella_project_promotes_children() {
        // shop/ holds every file and has 3 child packages: the umbrella is
        // transparent — children become containers (modules dotted shop.*),
        // files directly under shop/ stay in a `shop` container, and the
        // cross-package imports become real rels instead of self-edges.
        let root = temp_project("myshop");
        write(&root, "shop/auth/__init__.py", "");
        write(&root, "shop/auth/service.py", "import catalog\n# import fake\n");
        write(&root, "shop/payments/__init__.py", "");
        write(&root, "shop/payments/stripe.py", "import auth\n");
        write(&root, "shop/catalog/__init__.py", "");
        write(&root, "shop/catalog/views.py", "def handler():\n    import auth\n");
        write(&root, "shop/util.py", "");
        write(&root, "shop/main.py", "import auth\nimport payments\n");

        let ex = analyze(&root, &[]).unwrap();
        assert_eq!(ex.lang, Lang::Python);
        assert_eq!(
            ex.containers,
            vec![
                ci("auth", "auth", "shop.auth"),
                ci("catalog", "catalog", "shop.catalog"),
                ci("payments", "payments", "shop.payments"),
                ci("shop", "shop", "shop"),
            ]
        );
        assert_eq!(
            ex.relationships,
            vec![
                ("auth".to_string(), "catalog".to_string()),
                ("catalog".to_string(), "auth".to_string()),
                ("payments".to_string(), "auth".to_string()),
                ("shop".to_string(), "auth".to_string()),
                ("shop".to_string(), "payments".to_string()),
            ]
        );
        assert!(ex.warnings.is_empty(), "warnings: {:?}", ex.warnings);

        let text = render_spec(&ex);
        let unit =
            decispec_parse::parse_file("extracted.spec", &text).expect("draft must parse");
        let ws = decispec_parse::merge(vec![unit]);
        assert!(
            decispec_parse::validate(&ws).is_empty(),
            "{:?}",
            decispec_parse::validate(&ws)
        );

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    #[test]
    fn two_top_level_dirs_with_children_are_not_promoted() {
        // Two depth-1 dirs with code: ambiguous — the dirs themselves stay
        // the containers, children or not.
        let root = temp_project("fullstack");
        write(&root, "frontend/app/ui.py", "");
        write(&root, "backend/api/views.py", "import frontend\n");

        let ex = analyze(&root, &[]).unwrap();
        assert_eq!(
            ex.containers,
            vec![
                ci("backend", "backend", "backend"),
                ci("frontend", "frontend", "frontend"),
            ]
        );
        assert_eq!(
            ex.relationships,
            vec![("backend".to_string(), "frontend".to_string())]
        );

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    #[test]
    fn single_dir_with_only_loose_files_stays_one_container() {
        // One top-level dir but no child dirs with files: not an umbrella —
        // the dir itself remains the container (pre-umbrella behavior).
        let root = temp_project("onedir");
        write(&root, "pkg/a.py", "");
        write(&root, "pkg/b.py", "import a\n");

        let ex = analyze(&root, &[]).unwrap();
        assert_eq!(ex.containers, vec![ci("pkg", "pkg", "pkg")]);
        // `a` is a module inside pkg, not a container: unmapped.
        assert_eq!(
            ex.warnings,
            vec!["1 imports did not map to any container".to_string()]
        );

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    // -- 8. Error cases -------------------------------------------------------

    #[test]
    fn empty_tree_reports_no_source_files() {
        let root = temp_project("empty");
        let err = analyze(&root, &[]).unwrap_err();
        assert!(matches!(err, ExtractError::NoSourceFiles(_)), "{err}");
        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    #[test]
    fn non_directory_reports_not_a_directory() {
        let root = temp_project("afile");
        let f = root.join("main.py");
        std::fs::write(&f, "").unwrap();
        let err = analyze(&f, &[]).unwrap_err();
        assert!(matches!(err, ExtractError::NotADirectory(_)), "{err}");
        let err = analyze(&root.join("missing"), &[]).unwrap_err();
        assert!(matches!(err, ExtractError::NotADirectory(_)), "{err}");
        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    // -- Language detection edges ---------------------------------------------

    #[test]
    fn js_family_without_ts_files_is_javascript() {
        let root = temp_project("plainjs");
        write(&root, "a/x.js", "import b from './b';\n");
        write(&root, "b/y.jsx", "");
        write(&root, "c/z.mjs", "");
        write(&root, "d/w.cjs", "");

        let ex = analyze(&root, &[]).unwrap();
        assert_eq!(ex.lang, Lang::JavaScript);
        assert_eq!(ex.containers.len(), 4);
        // ./b resolves inside the importing file's own container: self-edge.
        assert!(ex.relationships.is_empty());

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    #[test]
    fn language_tie_breaks_to_python() {
        let root = temp_project("tie");
        write(&root, "a/x.py", "");
        write(&root, "b/y.ts", "");

        let ex = analyze(&root, &[]).unwrap();
        assert_eq!(ex.lang, Lang::Python);
        // Only the winning family's directory becomes a container.
        assert_eq!(ex.containers, vec![ci("a", "a", "a")]);

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    // -- Grouping warning ------------------------------------------------------

    #[test]
    fn more_than_30_containers_warn_about_grouping() {
        let root = temp_project("wide");
        for i in 0..31 {
            write(&root, &format!("pkg{i:02}/mod.py"), "");
        }

        let ex = analyze(&root, &[]).unwrap();
        assert_eq!(ex.containers.len(), 31, "all containers are kept");
        assert!(
            ex.warnings
                .iter()
                .any(|w| w.contains("consider grouping")),
            "warnings: {:?}",
            ex.warnings
        );

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    // -- render_fitness_toml ----------------------------------------------------

    #[test]
    fn fitness_toml_suggests_python_root_package_and_dotted_modules() {
        let root = temp_project("fitpy");
        write(&root, "api/x.py", "");
        write(&root, "auth/y.py", "");

        let ex = analyze(&root, &[]).unwrap();
        let toml = render_fitness_toml(&ex);
        assert!(toml.starts_with("# Suggested — merge into decispec.toml\n"));
        assert!(toml.contains("[stack.fitness]\n"));
        assert!(toml.contains("root_package = \"fitpy\"\n"));
        assert!(toml.contains("[stack.fitness.packages]\n"));
        assert!(toml.contains("api = \"fitpy.api\"\n"));
        assert!(toml.contains("auth = \"fitpy.auth\"\n"));

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    #[test]
    fn fitness_toml_for_js_uses_dir_paths_and_warns_about_glue() {
        let root = temp_project("fitjs");
        write(&root, "pages/index.ts", "");
        write(&root, "lib/helper.ts", "");

        let ex = analyze(&root, &[]).unwrap();
        let toml = render_fitness_toml(&ex);
        assert!(
            !toml.contains("root_package = "),
            "js/ts gets no live root_package:\n{toml}"
        );
        assert!(toml.contains("pages = \"pages\"\n"));
        assert!(toml.contains("lib = \"lib\"\n"));
        assert!(
            toml.contains("lang = \"js\" glue codegen is not yet supported"),
            "js glue limitation must be called out:\n{toml}"
        );

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    #[test]
    fn fitness_toml_sanitizes_python_package_names() {
        // Hyphenated project dirs are legal on disk but not importable:
        // the PYTHON suggestion scrubs to [A-Za-z0-9_.] (js/ts values are
        // filesystem paths and stay untouched).
        let root = temp_project("my-shop");
        write(&root, "api/x.py", "");

        let ex = analyze(&root, &[]).unwrap();
        let toml = render_fitness_toml(&ex);
        assert!(
            toml.contains("root_package = \"my_shop\"\n"),
            "hyphenated root_package must be sanitized:\n{toml}"
        );
        assert!(
            toml.contains("api = \"my_shop.api\"\n"),
            "hyphenated package value must be sanitized:\n{toml}"
        );

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    // -- ADR ingestion (F15) ---------------------------------------------------

    fn adr_project(name: &str) -> PathBuf {
        // Every fixture needs at least one source file or analyze bails with
        // NoSourceFiles; ADR-only projects are not a supported input.
        let root = temp_project(name);
        write(&root, "pkg/mod.py", "");
        root
    }

    // 1. MADR fixture: full field mapping.
    #[test]
    fn adr_madr_fixture_maps_fields() {
        let root = adr_project("adrproj");
        write(
            &root,
            "docs/adr/0001-use-postgres.md",
            "# Use Postgres for persistence\n\n## Status\n\nAccepted\n\n## Context\nWe need a durable store.\nIt must survive restarts.\n\n## Consequences\nOperational overhead of a new service.\n",
        );
        write(
            &root,
            "docs/adr/0002-cache.md",
            "# Add Redis caching\n\n## Status\nProposed\n",
        );

        let ex = analyze(&root, &[]).unwrap();
        assert_eq!(ex.decisions.len(), 2);
        let d1 = &ex.decisions[0];
        assert_eq!(d1.id, "ADR-0001");
        assert_eq!(d1.title, "Use Postgres for persistence");
        assert_eq!(d1.status, "accepted");
        assert_eq!(
            d1.context.as_deref(),
            Some("We need a durable store. It must survive restarts.")
        );
        assert_eq!(
            d1.consequences.as_deref(),
            Some("Operational overhead of a new service.")
        );
        assert_eq!(d1.source, "docs/adr/0001-use-postgres.md");
        let d2 = &ex.decisions[1];
        assert_eq!(d2.id, "ADR-0002");
        assert_eq!(d2.status, "proposed");
        assert_eq!(d2.context, None);
        assert_eq!(d2.consequences, None);

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    // 2. adr-tools style (`## Status` + blank line) and inline status lines;
    //    deprecated maps to superseded.
    #[test]
    fn adr_status_from_heading_and_inline_line() {
        let root = adr_project("adrstatus");
        write(&root, "docs/adr/0003.md", "# Plain numbered ADR\n\n## Status\n\nAccepted\n");
        write(
            &root,
            "docs/adr/0004.md",
            "# Inline status ADR\n\nSome prose first.\n\n**Status:** deprecated\n",
        );

        let ex = analyze(&root, &[]).unwrap();
        assert_eq!(ex.decisions.len(), 2);
        assert_eq!(ex.decisions[0].id, "ADR-0003");
        assert_eq!(ex.decisions[0].status, "accepted");
        assert_eq!(ex.decisions[1].id, "ADR-0004");
        assert_eq!(ex.decisions[1].status, "superseded");
        assert!(ex.warnings.is_empty(), "warnings: {:?}", ex.warnings);

        // A deprecated->superseded ADR renders a self superseded_by: the DSL
        // requires a target for status superseded and the real target is not
        // written in the ADR, so the draft stays valid while flagging the
        // gap for human review. The self-reference must be marked as a
        // placeholder by TODO comments directly above it, and only there.
        let text = render_spec(&ex);
        assert!(
            text.contains("  superseded_by: ADR-0004\n"),
            "superseded needs a target to stay valid:\n{text}"
        );
        let todo_pos = text
            .find("# TODO: status was \"superseded\"/\"deprecated\"")
            .expect("placeholder TODO comment missing:\n{text}");
        let sup_pos = text.find("  superseded_by: ADR-0004\n").unwrap();
        assert!(
            todo_pos < sup_pos,
            "TODO comment must precede the superseded_by line:\n{text}"
        );
        assert_eq!(
            text.matches("# TODO: status was \"superseded\"/\"deprecated\"").count(),
            1,
            "TODO comment only for the self-reference case:\n{text}"
        );
        let unit = decispec_parse::parse_file("extracted.spec", &text).unwrap();
        let ws = decispec_parse::merge(vec![unit]);
        assert!(
            decispec_parse::validate(&ws).is_empty(),
            "{:?}",
            decispec_parse::validate(&ws)
        );

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    // 3. Non-numeric filenames become scrubbed uppercase slugs; slugs that
    //    clash (even after scrubbing) get collision suffixes.
    #[test]
    fn adr_slug_ids_scrubbed_and_collision_suffixed() {
        let root = adr_project("adrslug");
        write(&root, "docs/decisions/use-uuid-pks.md", "# Use UUID PKs\n");
        write(&root, "docs/decisions/use.uuid.pks.md", "# Also UUID PKs\n");
        write(&root, "docs/decisions/weird name!.md", "# Weird name\n");

        let ex = analyze(&root, &[]).unwrap();
        let ids: Vec<&str> = ex.decisions.iter().map(|d| d.id.as_str()).collect();
        // Sorted by source path: use-uuid-pks.md < use.uuid.pks.md < weird name!.md
        assert_eq!(ids, vec!["USE-UUID-PKS", "USE-UUID-PKS_2", "WEIRD-NAME"]);

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    // 4. Missing title -> file skipped + warning; missing status -> proposed
    //    + warning.
    #[test]
    fn adr_missing_title_skipped_and_missing_status_warns() {
        let root = adr_project("adrskip");
        write(&root, "docs/adr/0005-no-title.md", "## Status\nAccepted\n");
        write(&root, "docs/adr/0006-no-status.md", "# Has a title only\n\nSome prose.\n");

        let ex = analyze(&root, &[]).unwrap();
        assert_eq!(ex.decisions.len(), 1, "0005 must be skipped");
        assert_eq!(ex.decisions[0].id, "ADR-0006");
        assert_eq!(ex.decisions[0].status, "proposed");
        assert!(
            ex.warnings
                .iter()
                .any(|w| w == "1 ADR file(s) skipped (no # title)"),
            "warnings: {:?}",
            ex.warnings
        );
        assert!(
            ex.warnings
                .iter()
                .any(|w| w == "1 ADR(s) missing status (defaulted to proposed)"),
            "warnings: {:?}",
            ex.warnings
        );

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    // 5. Long/multi-line context flattened to <=500 chars + ellipsis; ADR ids
    //    share the used set with container ids (a single-word slug collides
    //    with a same-named dir — numeric `ADR-000n` ids keep their dash and
    //    cannot collide because container ids are dash-free); draft
    //    round-trips.
    #[test]
    fn adr_long_context_flattened_and_id_collision_with_container() {
        let root = adr_project("adrcollision");
        write(&root, "POSTGRES/x.py", "");
        write(&root, "lib/y.py", "");
        let long_context = format!(
            "{} {}",
            "lorem ipsum dolor sit amet".repeat(20),
            "consectetur adipiscing elit".repeat(20),
        );
        write(
            &root,
            "docs/decisions/postgres.md",
            &format!("# Postgres\n\n## Status\nAccepted\n\n## Context\n{long_context}\n"),
        );

        let ex = analyze(&root, &[]).unwrap();
        // The container dir `postgres` was assigned first and keeps the bare
        // id; the decision slug collides and gets the suffix.
        assert!(ex.containers.iter().any(|c| c.id == "POSTGRES"));
        assert_eq!(ex.decisions.len(), 1);
        assert_eq!(ex.decisions[0].id, "POSTGRES_2");
        let ctx = ex.decisions[0].context.clone().unwrap();
        assert!(!ctx.contains('\n'), "context must be single-line");
        assert!(ctx.ends_with('…'), "context must be capped: {ctx:?}");
        assert_eq!(ctx.chars().count(), 501, "500 chars + ellipsis");

        let text = render_spec(&ex);
        let unit =
            decispec_parse::parse_file("extracted.spec", &text).expect("draft must parse");
        let ws = decispec_parse::merge(vec![unit]);
        assert!(
            decispec_parse::validate(&ws).is_empty(),
            "{:?}",
            decispec_parse::validate(&ws)
        );
        assert_eq!(ws.decisions.len(), 1);
        assert_eq!(ws.models[0].containers.len(), 3);

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    // 6. Excluded dirs must not contribute ADRs either.
    #[test]
    fn adr_excluded_dir_contributes_nothing() {
        let root = adr_project("adrexcl");
        write(&root, "docs/adr/0001-visible.md", "# Visible\n\n## Status\nAccepted\n");
        write(
            &root,
            "docs/adr/archive/0002-old.md",
            "# Old\n\n## Status\nAccepted\n",
        );

        let ex = analyze(&root, &[PathBuf::from("docs/adr/archive")]).unwrap();
        assert_eq!(ex.decisions.len(), 1);
        assert_eq!(ex.decisions[0].id, "ADR-0001");

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    // 7. Case-insensitive ADR dir names; decisions render before the model
    //    block with a `# Source:` comment.
    #[test]
    fn adr_case_insensitive_dirs_and_render_order() {
        let root = adr_project("adrcase");
        write(&root, "ADR/0007.md", "# Root-level adr dir\n\n## Status\nRejected\n");

        let ex = analyze(&root, &[]).unwrap();
        assert_eq!(ex.decisions.len(), 1);
        assert_eq!(ex.decisions[0].id, "ADR-0007");
        assert_eq!(ex.decisions[0].status, "rejected");

        let text = render_spec(&ex);
        let source_pos = text.find("# Source: ADR/0007.md").expect("source comment");
        let decision_pos = text.find("decision ADR-0007 \"Root-level adr dir\" {").unwrap();
        let model_pos = text.find("model {").unwrap();
        assert!(source_pos < decision_pos && decision_pos < model_pos);
        assert!(text.contains("  status: rejected\n"));

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }
}
