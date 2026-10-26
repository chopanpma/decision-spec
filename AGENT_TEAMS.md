# DecisionSpec agent team — plan and process

**2026-10-20 · Phase 0: this document exists; the first mission (F31, the TUI) is
queued and unclaimed.** Modeled on the tvgo agent-team plan
(`~/Projects/FPF/fpf-systems/tvgo/AGENT_TEAMS_PLAN.md`), adapted to what this
repository already enforces. Owner of this document: Francisco (lead /
main-orchestrator). Every claim below is dated; correct with a dated line when
it stops being true.

This file is the operating agreement for the agent teams working this repo. It
does **not** replace `AGENTS.md` — that is the rulebook, and its two hard rules
(TDD, and the `.coord/` claim/review protocol) bind every agent exactly as
written. This document adds what `AGENTS.md` does not have: role perimeters by
crate, the first mission broken into claimable tasks, and the guardrails
specific to working against *other projects'* spec repositories.

Sources: `AGENTS.md`, `ARCHITECTURE.md`, `README.md`, `.coord/BOARD.md`,
`docs/REVIEW.md`, `docs/SPEC_AUTHORING.md`, and read-only inspection of the two
first registered projects (`~/Projects/FPF/tvgo`, `~/Projects/FPF/verydian-app`).

---

## 1. Short answer

**A team of 6 crate-owning agents + 1 docs-only role + 2 read-only support
roles, coordinated by the lead through the existing `.coord/` board.** No new
coordination machinery is invented: crate exclusivity, root-file claims, TDD,
and independent review already exist here (`AGENTS.md` §1–§2) and are the
whole interface the team needs. The first mission is the **spec cockpit**: a
ratatui TUI that connects the `.spec` files of registered projects, renders
their decisions and generated artifacts on screen (never into the projects),
and shows which spec levels and gate rules are active per project. Phase 2 of
the mission adds in-TUI spec authoring with live syntax validation and PR
creation into each registered project's own repository.

---

## 2. How this repository works today

Measured 2026-10-20; numbers change, the shape matters.

- **Rust workspace, 8 crates** (`crates/*`): `ir` (typed IR + `ir::names`
  naming registry), `parse` (DSL parser + `file:line` diagnostics),
  `codegen` (pure `&Workspace -> Vec<GeneratedFile>`), `gate` (JUnit/manifest
  → matrix + verdict, pure), `store` (rusqlite run history), `extract`
  (bootstrap draft specs from existing py/js/ts projects), `mcp` (stdio MCP
  server: `validate_spec`, `check_workspace`, `workspace_overview`,
  `gate_status`, …), `cli` (clap binary — **all real I/O lives here**).
- **Exit-code contract:** `0` clean / `1` completed-with-findings / `2`
  could-not-complete. Load-bearing for CI; never add a code.
- **Libraries are pure:** filesystem-in, report-out. Writes, process spawns,
  terminal I/O, exit mapping: `cli` only.
- **TDD is mandatory** from F7 onward (`AGENTS.md` §1): red → green →
  refactor, no `#[ignore]`, no tautologies. F1–F6's two real bugs are the
  documented reason.
- **Coordination is `.coord/`** (`AGENTS.md` §2): one row per task on
  `BOARD.md` (`open → claimed → review → done`), one claim file per task
  naming exact files in scope and must-not-touch, crate exclusivity for
  `claimed` rows, root files claimed explicitly, finishing = `cargo build &&
  cargo test` green + change summary + a **different** agent's review verdict
  (`docs/REVIEW.md`).
- **The repo has git.** `AGENTS.md` §2 still says "no git — this repo is not a
  VCS checkout"; that stopped being true (branch `f28-f29-dogfood-spec`
  tracking `origin`, per `git status` 2026-10-20). Recorded as drift to
  correct (§11) — the claim/review protocol stays regardless, because it is
  how this team coordinates, not a git substitute.
- **Dogfooded:** this repo carries its own spec (`specs/`, 29 decisions /
  131 scenarios as of F29) and its own gate runs green (131/131, pytest
  adapter RAN via `tests/glue`).

### The two first registered projects (read-only inspection, 2026-10-20)

| Project | Path | Shape |
| --- | --- | --- |
| `tvgo` | `~/Projects/FPF/tvgo` | 4 decisions (`001-decisions.spec` ×3, `SMS-001.spec` ×1), `000-model.spec`, python stack, `[runners] pytest = "backend/.venv/bin/python -m pytest"`, glue in `tests/glue/`. Its `AGENTS.md` enforces PR-base-`main`-only and a human merge. |
| `verydian` | `~/Projects/FPF/verydian-app` (registered under the alias `verydian`) | 400 decisions: `010-adrs.spec` ×18, `020-agdrs.spec` ×105, `030-feature-specs.spec` ×75, `040-decision-log.spec` ×202, plus `000-model.spec` and `110-produccion.spec`. Python stack; glue shells out to each app's `vitest`. |

Neither project commits generated artifacts as source: `docs/diagrams/` and
`tests/generated/` are produced by `decispec gen` locally. The TUI mission
extends that rule — generated output is rendered to **screen only**.

---

## 3. Coexistence — the team vs. the house rules

The house rules win every conflict. On top of them, four agent-side rules:

1. **Perimeter by crate.** An agent implements only inside the crates its role
   owns; cross-crate needs go through the lead as a separate claim, never by
   reaching into another crate.
2. **The DSL is a published interface.** `tvgo` and `verydian` already carry
   committed `.spec` files. Grammar, ID-charset (see open F13), and validation
   changes must not break them — the reviewer re-parses both registered
   projects' specs as part of any `parse`/`ir` change's evidence.
3. **Registered projects are read-only until an explicit human action.** The
   TUI (and every agent) reads their specs, reports, and stores freely; it
   never writes into them. Writes happen only in mission phase 2, only via the
   registered project's own PR interface, only as draft PRs a human merges.
4. **Green before it asks for eyes.** Full `cargo build && cargo test` (plus
   the dogfood gate for spec/glue changes) before a row flips to `review`.

The real bottleneck is the lead's review bandwidth — every task costs one
independent agent review plus the lead's pass. Mitigations: small claims,
self-evidenced summaries, and never claiming a second item while a first sits
in `review`.

---

## 4. The team

Perimeters follow `AGENTS.md` §3's crate table — two agents never own the same
crate. "Danger zone" must stay in the role's description: it is what routes
work correctly.

| Agent | Owns | Read-only | Danger zone |
| --- | --- | --- | --- |
| `team-language` | `crates/ir`, `crates/parse` | specs/ (dogfood examples) | Grammar/validation changes can break the registered projects' committed specs; every IR change ripples into all six downstream crates; `ir::names` is the single naming authority — never duplicate a format string codegen emits and gate matches on |
| `team-gen` | `crates/codegen` | `gate` (the matching half) | Generated names are matched back by `gate` through `ir::names` — a rename without the paired gate side = silent `UNCOVERED` rows; `gen`'s stale sweep **deletes** header-marked files that no longer render — never run it where specs aren't the authority |
| `team-gate` | `crates/gate`, `crates/store` | `codegen` (the naming registry) | The exit-code contract is CI-load-bearing; running adapters executes foreign toolchains (pytest, playwright, import-linter…) inside a registered project — nothing auto-runs them |
| `team-bootstrap` | `crates/extract` | — | Extraction proposes drafts only — it never infers decisions or requirements (F15 rule); every draft must round-trip through `parse` |
| `team-surface` | `crates/cli`, `crates/tui` (new, F31) | — | Owns **all real I/O**: writes, process spawns, terminal setup, exit codes. The TUI renders registered projects read-only; save and PR actions are explicit, human-initiated, and go through each repo's own rules (tvgo: base `main` only, human merge) |
| `team-mcp` | `crates/mcp` | — | Agents are configured against today's tool schemas; `validate_spec` results are JSON inside the content envelope — shape changes break live agent setups |
| `team-spec` (docs-only) | `specs/`, `tests/glue/`, `docs/decisions.md`, `.coord/reviews/` | everything | Specs are this repo's truth; generated files carry no-edit headers; a task is done only with a review file; authoring rules live in `docs/SPEC_AUTHORING.md` |
| **lead** (Francisco / main-orchestrator) | `.coord/`, splits work, integrates reviewable units, `README.md`/`ARCHITECTURE.md`/`AGENTS.md` only when a task's acceptance criteria say so | — | Never merges own work without the normal review; one approving review per task, by a different agent |

Support roles — `reviewer`, `security-auditor` — are read-only and always a
*different* agent than the author (house rule, `docs/REVIEW.md`).

*Status 2026-10-20:* no role definition files exist yet (no `.claude/agents/`,
no launcher). Like the tvgo plan's Phase 1, writing the per-role definition
files is itself a claimable docs-only task once the first mission proves the
shapes. Until then, the table above + `AGENTS.md` is the definition.

---

## 5. The process — one item, end to end

Exactly `AGENTS.md` §2 plus evidence discipline:

1. **Pick** a row from `.coord/BOARD.md` whose scope (crates, root files) does
   not overlap any `claimed` row. The lead may pre-split larger missions into
   sequenced rows (F31 → F32 below).
2. **Claim.** Write `.coord/claims/<ID>.md`: summary, agent name, exact
   files/dirs in scope, must-not-touch, test plan (the failing test first),
   timestamp. Flip the board row to `claimed`.
3. **TDD.** Red → green → refactor (`AGENTS.md` §1). If TDD is genuinely
   impossible for the change, justify it in the claim; the reviewer rules on
   the excuse.
4. **Verify.** `cargo build && cargo test` green; for `parse`/`ir` changes,
   also re-parse both registered projects' specs clean; for spec/glue changes,
   the dogfood gate green. Paste the tail into the claim as build evidence.
5. **Summarize.** Change summary in the claim: files touched, behavior added,
   exit-code/DSL surface changes, proposed doc updates. Flip the row to
   `review`.
6. **Review.** A different agent reviews per `docs/REVIEW.md` (correctness,
   exit codes, TDD evidence, architecture fit), re-runs the suite themselves,
   and writes `.coord/reviews/<ID>.md`. Findings loop back to step 3.
7. **Done.** Only the reviewer-approved row may flip to `done`.

Definition of done: claim + review files exist, suite green, no scope creep,
docs (`README.md`/`ARCHITECTURE.md`) updated when the acceptance criteria say
so — otherwise the proposed doc updates sit in the claim for the lead to fold
in.

---

## 6. The first mission — the spec cockpit (ratatui TUI)

**Goal (user requirement, 2026-10-20):** a TUI that connects the decispec
specs of different projects — starting with `tvgo` and `verydian` — to view
formatted specs; the generated decisions/artifacts are rendered for the TUI on
the fly, so **no repository commits generated files**; and the levels of specs
and rules active on any registered project are visible at a glance. The
mission's phase 2: author specs from the TUI with live syntax revision so the
spec stays consistent and testable, and open the PR for it in each project.

### Design constraints (decided up front, so claims don't relitigate them)

- **New crate `crates/tui`, launched as `decispec tui` from `cli`.** The `tui`
  crate holds pure UI logic: app state, `update(state, event) -> state`,
  widget trees as functions into ratatui `Buffer`. **`cli` owns the terminal**:
  crossterm raw-mode setup, the event pump, and teardown — the same
  Template-Method split `cmd_*` already uses. Rationale: keeps libraries pure
  per `AGENTS.md` §3 and makes the whole UI TDD-able with ratatui's
  `TestBackend` (buffer-assertion tests, no terminal needed).
- **In-memory generation.** The "auto-generated for this TUI" requirement is
  met by calling `codegen`'s existing pure render functions
  (`&Workspace -> Vec<GeneratedFile>`) and showing the buffer contents in a
  viewer pane — byte-identical to what `gen` would write, never written.
  Satisfies "no repository has generated files committed" by construction.
- **Project registry, cli-owned.** `~/.config/decispec/registry.toml`
  (`$XDG_CONFIG_HOME` respected): `[[project]] name = "tvgo", path = "…"`.
  `decispec tui` reads it; registration is a cli command (e.g.
  `decispec tui --register <path>`), not a TUI write to foreign repos. First
  two entries seeded: `tvgo`, `verydian` (→ `~/Projects/FPF/verydian-app`).
- **"Levels of specs and rules that are active"** renders as three bands per
  registered project: (a) decision status levels — proposed / accepted /
  superseded (with `superseded_by` link) / rejected, active = accepted +
  proposed, superseded dimmed, rejected hidden by default; (b) requirement
  layers — unit / contract / e2e / infra / fitness with per-layer counts; (c)
  gate rule state — the last verdict from `.decispec/report.json` + `store`,
  rows as pass / fail / uncovered / skipped, colored. Re-running a gate from
  the TUI is **not** phase 1: adapters execute foreign toolchains inside
  registered projects, so refresh is an explicit later decision (§11).
- **Live syntax revision reuses `parse`.** `parse::parse_file(file, src)` +
  `merge` + `validate` already run on strings without touching disk (the mcp
  crate's `validate_spec` is the worked example); the phase-2 editor validates
  on every change and shows `file:line` diagnostics in a pane. F26
  (`check --strict` authoring lint) stays its own task — the TUI must not
  absorb it.
- **PR creation is phase 2, per-repo rules.** Draft PRs only, base `main`,
  branch `spec/<id>-<slug>`, through `gh` from `cli`; tvgo's `AGENTS.md`
  (base-`main`-only, human merge, no agent merge) applies to anything the TUI
  opens there.

### Task breakdown (board rows)

| Row | Task | Scope | Depends |
| --- | --- | --- | --- |
| **F31** | TUI phase 1 — read-only cockpit: registry, project browser, formatted spec/decision views, in-memory generated-artifact viewer, levels + gate-state dashboard | crates: `tui` (new), `cli`; root: `Cargo.toml`, `Cargo.lock`, `README.md`, `docs/USAGE.md`, `ARCHITECTURE.md` | — |
| **F32** | TUI phase 2 — in-TUI spec authoring: editor pane with live `parse`-validation, explicit save (writes only the project's `specs/*.spec`), consistency check, then branch + draft PR per registered repo's rules | crates: `tui`, `cli`, `parse` (string-entry only — grammar work, if any, is `team-language`'s separate claim); root: `README.md`, `docs/USAGE.md` | F31 |

Adjacent open rows the mission must **not** silently absorb: F21 (decision→
element linkage / DSL `touches` field), F22 (diff views), F24 (spec
federation/ownership), F26 (authoring lint), F13 (ID charset). If the cockpit
needs one, the lead sequences it as its own claim first.

---

## 7. Guardrails

- **Registered projects are read-only** (F31 hard rule; F32 writes only via
  explicit save → the project's own PR flow, drafts only).
- **Nothing auto-runs foreign toolchains.** No `decispec test`/`gate` adapter
  execution inside a registered project from the TUI without an explicit
  keypress decision recorded in the F31 claim — default reads the last stored
  report.
- **Registered projects' rulebooks bind the TUI's writes.** tvgo: PR base
  `main` only, agent never merges, screenshots/notes per its `AGENTS.md`.
- **Never commit generated artifacts anywhere.** Not in this repo, not in
  registered projects; the TUI renders them in memory.
- **Exit-code contract untouched by the TUI** — `decispec tui` is interactive;
  it returns `0` on clean quit and `2` on terminal setup failure, never `1`
  (findings are for batch commands; decided 2026-10-20, §12).
- Secrets never enter specs, glue, claims, or PRs — same as every FPF repo.

## 8. Isolation

This repo now has git (§2), but the team's isolation mechanism stays the
`.coord/` claim protocol — it predates git here and is what makes crate
exclusivity enforceable. One agent works in the main checkout; parallel
builders coordinate through disjoint claims, and if two builders ever need the
same crate, the lead serializes rather than worktrees (small workspace, cheap
rebase). Registered projects are never build targets for this team — only read
sources and, in F32, PR destinations.

## 9. Where coordination state lives

Unchanged from `AGENTS.md` §2 — this repo, in the open:

```
.coord/BOARD.md          one row per task: open → claimed → review → done
.coord/claims/<ID>.md    scope exclusivity: files to touch / must not touch
.coord/reviews/<ID>.md   verdict by a different agent
```

This document (`AGENT_TEAMS.md`) is team configuration, not task state; the
board stays the single source of truth for what is actually in flight.

---

## 10. Rollout

- **Phase 0 — done 2026-10-20.** This plan (F30) written and queued F31/F32.
  Cost: one session.
- **Phase 1 — next claim.** F31: the read-only cockpit against `tvgo` +
  `verydian`. TDD via `TestBackend` buffer assertions; acceptance = both
  registered projects browsable, decisions rendered formatted with status
  levels, generated artifacts viewable in memory, gate state per project
  visible, zero writes into either project.
- **Phase 2 — only with F31 reviewed.** F32: authoring + live validation +
  save + draft PR flow, first end-to-end exercise being one real spec edit on
  one registered project, merged by a human.
- **Phase 3 — only if 1–2 prove it.** Role definition files installed per
  harness; gate-refresh and federation (F24) tie-ins; publish the CLI so
  registered projects can gate in CI (unblocks tvgo's disabled decispec CI
  job).

## 11. Risks and drift to correct

- **Review bandwidth.** Every row costs an independent agent review + lead
  pass. Keep claims small; the F31→F32 split is deliberate.
- **TUI test rot.** Terminal UIs regress silently. Discipline: every widget
  and every `update` transition has a `TestBackend`/state test that could
  fail; no "manually checked" rows in the claim.
- **Purity creep.** Crossterm calls inside `tui` would break the house
  invariant; the F31 reviewer checks for it explicitly (architecture-fit item
  in `docs/REVIEW.md`).
- **Registered-project drift.** tvgo/verydian specs grow; the F31 acceptance
  re-parse check keeps `parse` honest against real files, not fixtures only.
- **Scope creep into F13/F21/F22/F24/F26.** The cockpit visualizes levels; it
  does not redesign the DSL to get nicer visuals.
- **Drift to correct (proposed doc update, lead commits):** `AGENTS.md` §2's
  "no git" preamble is stale — the repo is a git checkout on
  `f28-f29-dogfood-spec` (verified 2026-10-20). Correct the sentence, keep the
  protocol. Also: `AGENTS.md` does not yet point at this file; add one line
  under §2 naming `AGENT_TEAMS.md` as the team layer.

## 12. Decisions

Dated, newest last. "By" is who took it.

| Date | Decision | By |
| --- | --- | --- |
| 2026-10-20 | **Team shape:** 6 crate-owning agents + docs-only `team-spec` + read-only support roles, coordinated through the existing `.coord/` protocol; `AGENTS.md` remains the rulebook | lead |
| 2026-10-20 | **First mission:** spec cockpit TUI; registered projects `tvgo` + `verydian` (alias for `~/Projects/FPF/verydian-app`); phased F31 (read-only viewer) → F32 (authoring + PRs) | lead |
| 2026-10-20 | **No generated files committed anywhere:** the TUI renders `codegen` output in memory (pure render fns); registered projects stay write-free until F32's explicit save/PR path | lead |
| 2026-10-20 | **`crates/tui` pure, `cli` owns the terminal:** state + widget fns + `TestBackend` tests in `tui`; crossterm setup/pump/teardown in `cli`; launched as `decispec tui` | lead |
| 2026-10-20 | **Registry:** `~/.config/decispec/registry.toml`, cli-owned I/O, seeded with tvgo + verydian | lead |
| 2026-10-20 | **No auto-run of adapters from the TUI** (phase 1 reads last stored gate report; refresh is a later explicit decision) | lead |
| 2026-10-20 | **`decispec tui` exit codes:** `0` clean quit, `2` terminal/setup failure; never `1` — the findings code is for batch commands | lead |
| 2026-10-20 | **PRs from the TUI are draft PRs** through each registered repo's own interface and rulebook; agents never merge | lead |

## 13. Documentation map for agents

Read in order before claiming:

1. `AGENTS.md` — the rulebook (TDD, `.coord/` protocol, crate invariants).
2. `ARCHITECTURE.md` + `README.md` — design and behavior, both normative.
3. This file — roles, mission phasing, guardrails.
4. `.coord/BOARD.md` + your claim file — what is actually in flight.
5. `docs/REVIEW.md` — the review checklist your work will face.
6. `docs/SPEC_AUTHORING.md` — the authoring rules (F32 enforces these in-UI).
7. The tvgo plan `~/Projects/FPF/fpf-systems/tvgo/AGENT_TEAMS_PLAN.md` — the
   method source this document adapts.
8. Registered projects' specs + rulebooks (`tvgo/AGENTS.md`, verydian-app's)
   when a task touches them.
