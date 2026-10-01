# DecisionSpec agent board — single source of truth for in-flight work

> Protocol: see `AGENTS.md` §2. One row per task. A task may be picked up only
> when no other row with overlapping scope (crates, root files) is `claimed`.
> Status values: `open` → `claimed` → `review` → `done` (or back to `claimed`
> with findings).

| ID | Task | Scope (crates / root files) | Owner | Status | Notes |
| --- | --- | --- | --- | --- | --- |
| F8 | `decispec extract` — bootstrap specs from an existing Python/JS/TS project | crates: extract (new), cli; root: Cargo.toml, Cargo.lock, ARCHITECTURE.md, README.md, docs/USAGE.md, AGENTS.md | main-orchestrator | **done** | Landed after 2 review rounds (see `.coord/reviews/F8.md`). 98 tests green. Delivers: "read from a project (python, javascript, typescript) and extract the specs for projects already started". |
| F9 | Fix inverted dependency-cruiser rule direction | crates: codegen; root: README.md only if the documented direction changes (it should not — README:167 is already correct) | — | open | Evidence in ARCHITECTURE.md "Remaining work" §1. Regression/direction test first (TDD), then fix `depcruise`. Do not assert on rule name/shape only — assert which direction is forbidden. |
| F10 | Integration tests that run the built binary | crates: cli (tests/ dir + dev-deps); root: Cargo.toml, Cargo.lock | — | open | README quickstart flow over a fixture project: init --demo → check → gen → test → gate → query, asserting exit codes 0/1/2 end-to-end. No real toolchains available: use `--results` with hand-written JUnit XML. |
| F11 | Implement or cut `[runners]` per-tool overrides | crates: cli; root: README.md | — | open | Documented (README:227) and written by init as a comment, but parse_config ignores it and run_adapters hardcodes invocations. Pick one: wire it through, or delete the docs+comment. |
| F13 | Reconcile ID charset: lexer vs README | crates: parse; root: README.md | — | open | Lexer (parse/src/lib.rs:239) accepts Unicode alphanumerics in IDs; README documents ASCII `[A-Za-z0-9_-]+`; extract's scrubber emits ASCII. Tighten lexer to `is_ascii_alphanumeric` (aligns all three) or amend README. |
| F14 | Rename project → DecisionSpec / `decispec` | ALL crates; root: Cargo.lock, README.md, ARCHITECTURE.md, AGENTS.md, docs/USAGE.md, .coord/BOARD.md header | main-orchestrator (coder: agent-1) | **done** | Exclusive rename landed clean: 98 tests green, grep-zero leftovers outside the F8 audit trail, reviewer e2e smoke passed. See `.coord/reviews/F14.md`. |
| F15 | `decispec extract` ingests ADRs into `decision` blocks | crates: extract, cli; root: README.md, docs/USAGE.md, Cargo.lock | main-orchestrator (coder: agent-1) | **done** | Landed + 1 ruling round (superseded self-ref placeholder with in-draft TODO). 106 tests green, reviewer e2e verified. See `.coord/reviews/F15.md`. |

## Finished tasks

| ID | Task | Merged by | Summary |
| --- | --- | --- | --- |
| F1–F7 | init/check/gen/test/gate/query, strict-skipped, fitness packages, TDD from F7 | — | Pre-board era. See ARCHITECTURE.md "Workflow". |
