# Using DecisionSpec

End-user guide. Status: **draft for MVP** — refresh at MVP close (see
`.coord/BOARD.md`).

DecisionSpec is a spec-driven compliance CLI. You write architectural decisions
and behavior specs in `.spec` files (committed to git, next to your code);
DecisionSpec compiles them into diagrams and per-layer test skeletons, runs your
test toolchains, and gates the build on a traceability matrix:
decision → requirement → test → result.

## 1. Install

```sh
cargo build --release   # Rust 1.98+, from the DecisionSpec repo
# binary at target/release/decispec — put it on PATH
```

## 2. Start a project

Greenfield:

```sh
cd my-project
decispec init --demo    # writes decispec.toml, specs/, tests/glue/, .decispec/
```

Already-started Python/JavaScript/TypeScript project:

```sh
decispec init
decispec extract        # drafts specs/extracted.spec from layout + imports + ADRs
# edit the draft: name containers, add requirements (decisions may already be there)
```

`extract` recovers structure — containers from top-level directories (or the
children of a single umbrella dir that holds the code, like `src/` or your
main package), import relationships, and suggested fitness module mappings —
plus decisions already written as ADR markdown docs (`docs/adr/`,
`docs/decisions/`, `adr/`, `decisions/`), which become `decision` blocks
(each with a `# Source:` comment; title/status/context/consequences only).
It does not recover intent: requirements and scenarios are yours to write.

Specs can also live outside the repo, in a store directory under your
control. `decispec extract --to ~/specs` (in the analyzed project, or with an
explicit path) drafts straight into the store as `<project-dirname>.spec`
— never into the analyzed repo, never over an existing file. The store then
bootstraps new projects: in a fresh directory, `decispec init --from ~/specs`
copies every stored `.spec` (a directory argument contributes its direct
`.spec` children) into `specs/` under the original names, after validating
them as a workspace — a broken store file exits 2 with `file:line`
diagnostics and writes nothing, and an existing `specs/` tree is never
clobbered.

## 3. The daily loop

```sh
decispec check     # 1. parse + validate specs (file:line errors; exit 1 = findings)
decispec gen       # 2. render diagrams + test skeletons (idempotent, safe to re-run)
# 3. implement tests/glue/ — the only hand-written test code
decispec test      # 4. run pytest/cucumber/playwright/conftest/import-linter...
decispec gate      # 5. matrix + verdict; exit 0 = compliant, 1 = violation
decispec query AUTH-001   # why is this decision here, what covers it, last gate status
decispec index     # 6. GitHub-renderable decision index (diagrams + tables),
                     #    default .decispec/docs/decisions.md (gitignored); --out for a tracked copy
```

All commands work from any subdirectory; `decispec.toml` is found by walking
up. Regeneration is byte-identical, never touches `tests/glue/`, and deletes
artifacts whose spec item disappeared.

Writing specs well matters more than any command — decision-block rules, the
EARS pattern, and a worked example: see
[SPEC_AUTHORING.md](SPEC_AUTHORING.md).

## 4. Writing specs (the 20% you need)

```sdl
decision AUTH-001 "OAuth2 client credentials for service-to-service auth" {
  status: accepted               # accepted | proposed | rejected | superseded
  context: "machines talking to machines, no shared secrets"
  consequences: "+ no shared secrets; - token latency on cold start"
}

model {
  container api "Orders API"
  container auth "Auth Service"
  rel api -> auth: "requests token"
  flow svc_auth {
    api -> auth: "POST /token"
    alt valid { auth --> api: "200 + token" } else { auth --> api: "401" }
  }
}

spec AUTH-001 {                   # must reference the decision ID
  requirement AUTH-001-R1 (EARS) {
    text: "WHEN valid client credentials THEN the auth service SHALL return a token"
    layers: [unit, contract, e2e]     # unit | contract | e2e | infra | fitness
    scenarios: [valid_token]
  }
  scenario valid_token {
    given: "valid client credentials"
    when: "a token request is made"
    then: "status 200 and a token is returned"
  }
}
```

Rules that bite: IDs are unique workspace-wide; requirement layers drive which
test skeletons get generated; scenarios inherit the union of referencing
requirements' layers.

## 5. decispec.toml

```toml
[project]
name = "my-project"

[stack]
lang = "python"                 # python | rust (more later)
glue_module = "tests.glue"

[stack.fitness]
root_package = "myapp"          # import-linter root

[stack.fitness.packages]        # container id -> importable module
api = "myapp.api"
auth = "myapp.auth"
```

A `rel` is enforced only when **both** of its containers are mapped here
(import-linter is python-only); a rel with an unmapped endpoint gets a TODO
naming the container to add, and no contract.

## 6. Exit codes (CI contract)

| Code | Meaning |
| --- | --- |
| 0 | completed, nothing to report (gate PASS) |
| 1 | completed **with findings**: gate FAIL, check errors, missing query ID |
| 2 | could not complete: bad usage, unparseable specs for gen/gate, missing results dir, I/O errors |

CI: `1` = "there is work to do", `2` = "the run itself is broken".

```yaml
# GitHub Actions sketch
- run: decispec check && decispec gen
- run: decispec gate --strict-skipped --allow-skips specs/skips.toml
```

`--strict-skipped` turns `SKIPPED` rows into `FAIL` — a layer with no toolchain
installed, and equally a JUnit case the toolchain reported `<skipped>`
(`pytest.mark.skip`, Playwright `test.skip`/`test.fixme`): an intentionally
skipped test is never a silent pass. Use strict mode in CI, not while hacking
locally. `--allow-skips <file>` is the ratchet that pairs with it: a TOML list
of `[[skips]]` tables (decision, item, reason) naming the gaps a project
intentionally carries — listed rows stay `SKIPPED` under strict mode, every
other skip still fails, and entries that matched no skip warn as stale.

## 7. Reading the gate

- A row fails when a mapped layer has no artifacts, an executed test failed,
  or the toolchain ran but the item has no result (`UNCOVERED`).
- Contract/e2e matching is by generated test name; a naming mismatch degrades
  that row to suite-level judgment with a printed `WARNING`.
- Superseded decisions warn instead of failing (plus a coverage-drift warning
  if tests still map to them).
- `--json` emits `.decispec/report.json`; every run is recorded in
  `.decispec/decispec.db` (`decispec query` reads it).

## 7a. Proof coverage: what percent of the specs are proven

`decispec coverage` answers "how much of this spec suite do tests actually
prove?" without running anything — it reads the last `.decispec/report.json`:

```text
═══ DECISIONSPEC COVERAGE ═══ project: tvgo (from gate run 2026-10-20 12:00:00 UTC)
✓ MONEY-001 proven
◐ MONEY-002 partial
○ MONEY-003 unproven
coverage: 1/3 decisions proven (33%)
```

A decision is **proven** only when every gatable row mapped to it passed;
**partial** when at least one row passed and at least one did not (a `SKIPPED`
row is never proof); superseded and rejected decisions are excluded from the
headline. The gate's own summary prints the same line, and the TUI shows the
per-decision marks and the headline on the dashboard. The `from gate run …`
stamp matters: the numbers reflect that run, so re-run `decispec gate` after
editing specs. In CI, `decispec coverage --min 80` exits 1 while the headline
is below the floor; with no report at all the command exits 2 — it never
invents a number.

## 8. AI agents (MCP)

`decispec mcp` exposes DecisionSpec to AI agents as an MCP server over stdio
— typed tools instead of parsed CLI text. Setup and the full tool table live
in [README.md](README.md#using-with-ai-agents-mcp); the short version:

```json
{"mcpServers": {"decispec": {"command": "decispec", "args": ["mcp"]}}}
```

Point the agent's `cwd` at the project you are porting (or pass `path` per
call). Tools: `project_info`, `extract_project` (bootstrap a draft from an
existing project), `validate_spec` (file:line diagnostics for a spec text
before you write it), `check_workspace`, `workspace_overview` (the porting
work queue), `gate_status`.

## 9. The spec cockpit (TUI)

> First session? Walk through it in [docs/TUI.md](TUI.md) — register a
> project, browse, edit, open a draft PR.

`decispec tui` is a read-only terminal cockpit over **registered projects**
— any directory with a `decispec.toml`. It exists so a spec fleet can be
browsed from one place without generating files into any repository:

- **specs** (default view): decision list with status colors per project —
  accepted/proposed in full color, superseded dimmed, rejected red — a
  detail pane with context, consequences, requirements (layers + EARS text),
  scenarios, invariants, and infra policies, and a `/` keyword filter that
  narrows the list by id or title (Esc clears, Enter keeps for browsing).
- **artifacts** (`⇥`): what `decispec gen` **would** write, rendered in
  memory from the live workspace — diagrams, per-layer test skeletons, and
  the on-demand decision index — listed by path with a scrollable preview.
  Nothing is written; registered projects stay clean of generated files.
- **dashboard** (`⇥`): the levels that are *active* on the project —
  decision-status counts (with accepted+proposed as "active"), requirements
  per layer, and the last gate verdict with its row counts, failures, and
  warnings (read from the project's `.decispec/report.json`).

```sh
decispec tui                       # browse the registry
decispec tui --register ~/work/app # add a project, then exit
```

The registry lives at `~/.config/decispec/registry.toml`
(`$XDG_CONFIG_HOME` honored); on first run it is seeded with well-known
projects that exist on the machine and written so it stays editable. A
project whose specs fail to parse shows its `file:line` diagnostics as an
error card instead of breaking the session.

Keys: `←→` switch project · `⇥` cycle view · `↑↓` select · `⏎` page down ·
`esc` back to top · `r` reload all projects from disk · `e` edit or create a
spec (`n` in the picker = new file, `/` = type-to-filter the list) · `p` open a draft PR · `q` quit. The TUI never runs toolchains (adapters)
inside registered projects — it only reads what is on disk. Exit codes: `0`
on clean quit, `2` on terminal/registry setup failure; it never exits `1`
(findings are for batch commands).

### Editing and PRs

Pressing `e` opens the current project's spec files (picked with `↑↓`+`⏎`,
filterable with `/`)
in an inline editor. With no spec files — or with `n` in the picker — you
get a **name prompt** instead: a valid name (relative to `specs/`, ending
in `.spec`) opens the editor on a minimal, already-valid PREFIX-001
skeleton you rename and fill in; the file is created only when you `Ctrl+S`.
Every keystroke re-parses the buffer **in the context of
the whole workspace** — cross-file references included — and shows
`file:line` diagnostics in a pane under the text, so a spec stays consistent
as it is written (parse errors first, phase-by-phase). `Ctrl+S` saves the
file into the project and immediately re-checks the workspace, reporting
"ok: N decision(s)" or the exact `file:line` failures; a broken save is
reported, never hidden. `esc` closes a clean buffer, and asks once before
discarding a modified one. Broken projects are editable too — the file picker
works from their on-disk sources, which is how they get fixed.

With saved spec changes in a git-registered project, `p` creates a **draft
PR**: a `spec/tui-<timestamp>` branch, a commit containing only `specs/`,
push, then `gh pr create --draft --base main`. Nothing is ever merged from
the cockpit, and changes outside `specs/` are refused. Failures (not a git
repo, clean tree, `gh` missing, push rejected) land in the status bar as
plain messages.

## 10. Troubleshooting

- **gate exits 2 right after editing specs** → run `decispec check`; it prints
  `file:line` diagnostics. (check = findings/1; gen/gate = abort/2.)
- **everything SKIPPED** → toolchains not installed; that's a pass by default.
- **stale artifacts** → you edited a spec id; re-run `gen` — it sweeps files
  whose header says DecisionSpec but whose item no longer exists.
- **green locally, red in CI** → almost always `--strict-skipped` catching a
  toolchain CI doesn't have.
