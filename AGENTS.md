# AGENTS.md — rules for every agent working in this repo

DecisionSpec is a Rust workspace (`crates/`: ir, parse, codegen, gate, store,
extract, cli).
Behavior is documented in `README.md`, design in `ARCHITECTURE.md`. Both are
normative: if code and docs disagree, that is a bug — fix the code or flag the
doc, never silently let them drift.

## 1. TDD is mandatory

**Default workflow for any behavior change: red → green → refactor.**

1. Write the failing test first. Run it. Confirm it fails **for the right
   reason** (the error must be about the missing behavior, not a typo or
   compile error in the test itself).
2. Implement the minimum that makes it pass.
3. Re-run the full suite: `cargo build && cargo test` — everything green.
4. Refactor only with the tests green.

Rules:

- **Bug fix = regression test first.** The test must fail on the unfixed code.
- **No tautological tests.** A test that asserts a constant, re-states the
  implementation, or cannot fail under any correct-or-incorrect implementation
  is worse than no test. Reviewers are expected to delete these.
- **No `#[ignore]`, no skipped assertions** without a written justification in
  the claim file (see below).
- **Tests are the spec.** When a brief/task description and a test disagree,
  stop and reconcile before proceeding — the existing tests pin prior behavior
  (characterization before refactor, see ARCHITECTURE.md).
- If TDD is genuinely impossible for a change (e.g. pure codegen string shape
  where the test IS the first artifact), say so in the claim file; the reviewer
  decides whether the excuse holds.

F7 onward was built this way; F1–F6 were not, and both real bugs of that era
surfaced in review instead of as red tests. That is the documented reason this
rule is now hard.

## 2. Multi-agent coordination (no git — this repo is not a VCS checkout)

There is **no git**. Edits land directly on disk, so two agents editing the
same file destroy each other's work. The protocol below is the only thing
preventing that. It lives in `.coord/`:

```
.coord/BOARD.md          task board — single source of truth
.coord/claims/<ID>.md    one claim file per task (ID = board row, e.g. F8)
.coord/reviews/<ID>.md   one review report per reviewed task
```

### Protocol — before starting any task

1. Read `.coord/BOARD.md` and every claim file whose status is `claimed`.
2. Choose a task whose **scope does not overlap** any active claim. Overlap is
   judged by crate and by file list (see rules below), not by task title.
3. Create `.coord/claims/<ID>.md` with: task summary, your agent name, the
   **exact files/dirs you will touch** (scope), the files you must **not**
   touch, the test plan (which tests you will write first), and a timestamp.
4. Flip the board row to `claimed` + owner. Keep board edits minimal — touch
   only your row.

### Hard scope rules

- **Crate exclusivity.** No two `claimed` tasks may list the same crate as
  in-scope. Crates are the unit of ownership.
- **Root files must be claimed explicitly.** `Cargo.toml`, `Cargo.lock`,
  `README.md`, `ARCHITECTURE.md`, `AGENTS.md`, `docs/*`, `.gitignore` — none of
  these may appear in two active claims, and none may be edited without being
  listed in the claim.
- **Orchestrator-owned docs.** `README.md` and `ARCHITECTURE.md` should be
  updated by the task owner only when the task's acceptance criteria say so;
  otherwise record proposed doc changes under "Proposed doc updates" in the
  claim file and let the orchestrator fold them in.
- **Never touch another agent's claim file or `.coord/reviews/` entries.**
  The only shared file is `BOARD.md`, and only your own row in it.
- If mid-task you discover you need an out-of-scope file: stop, append a note
  to your claim ("Scope change request: …"), and only proceed once the overlap
  is clear (the other agent finished, or the orchestrator ruled).

### Protocol — finishing a task

1. `cargo build && cargo test` green — paste the tail of the output into the
   claim file under "Build evidence".
2. Append a "Change summary" to the claim: files changed, behavior added,
   exit-code/DSL surface changes, proposed doc updates.
3. Flip your board row to `review`.
4. Only after a reviewer approves (see `docs/REVIEW.md`) may the row go to
   `done`.

### Emergency conflicts

If you discover another agent has edited your in-scope files (board said free,
disk says otherwise): do **not** revert their work. Append a "Conflict" note to
your claim and the board, finish your own files, and flag it for the
orchestrator. Reverts without orchestrator sign-off are forbidden.

## 3. Where things live (orientation)

| Crate | Responsibility | Never contains |
| --- | --- | --- |
| `ir` | typed IR + deterministic test-naming registry (`ir::names`) | I/O |
| `parse` | `.spec` parser + cross-ref validation, `file:line` diagnostics | I/O beyond reading spec files |
| `codegen` | IR → artifacts, pure `&Workspace -> Vec<GeneratedFile>` | filesystem writes |
| `gate` | JUnit/manifest → matrix + verdict, pure | process spawning |
| `store` | rusqlite run history | CLI concerns |
| `extract` | bootstrap a draft `.spec` from an existing py/js/ts project (`analyze`, `render_*`); never infers decisions | filesystem writes (cli owns those) |
| `cli` | clap binary, all real I/O, exit-code mapping | business logic (delegate to libs) |

Invariants that apply no matter what you build:

- Libraries are pure: filesystem-in, report-out. All real I/O (writes, process
  runs, exit codes) lives in `cli`.
- Exit-code contract: `0` = clean, `1` = completed with findings,
  `2` = could not complete. Never introduce a new exit code.
- Generated names go through `ir::names` — never duplicate a format string
  that codegen emits and gate matches on.
- New toolchain support touches `run_adapters()` and nowhere else.
