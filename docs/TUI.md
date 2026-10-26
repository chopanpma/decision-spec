# How to use `decispec tui`

`decispec tui` is a terminal UI over a DecisionSpec project. Run it inside a
project and the session opens with exactly that project — no registry, no
sidebar of foreign projects — so you can browse its decisions and specs,
preview exactly what `decispec gen` would write (without writing it), see
the last gate verdict, fix a spec with live `file:line` diagnostics, and
push the fix as a draft PR. This guide walks through a first session; the
reference description lives in [USAGE.md §9](USAGE.md#9-the-spec-cockpit-tui).

The TUI is read-only over the project. It only ever writes in two
places, both explicit: a spec file you save with `Ctrl+S`, and the
git branch/draft PR you request with `p`.

## 1. Start a session

```sh
cd ~/work/payments
decispec tui
```

The session opens on this project alone. (Outside any project, or with
`--all`, the cockpit instead browses the registry, below.)

## 2. The registry (fleet mode)

A project is any directory with a `decispec.toml`. To browse several
projects from one session, register them:

```sh
decispec tui --register ~/work/payments
# registered payments -> /Users/you/work/payments
# registry: /Users/you/.config/decispec/registry.toml
```

The display name comes from the project's `[project].name` in
`decispec.toml`. Registering the same project again replaces its entry (no
duplicates). The registry is a plain file you can edit by hand:

```toml
# ~/.config/decispec/registry.toml  ($XDG_CONFIG_HOME is honored)

[[project]]
name = "payments"
path = "~/work/payments"   # ~/ is expanded against $HOME
```

On the very first **fleet-mode** run, if well-known projects
(`~/Projects/FPF/tvgo`, `~/Projects/FPF/verydian-app`) exist on your machine
they are seeded into the registry automatically, so the file is inspectable
from the start. Single-project sessions never touch the registry.

```sh
decispec tui --all   # fleet mode even inside a project
```

In fleet mode you get a sidebar of registered projects (a red dot marks one that failed
to load — see §7) and the current project's view. A project whose specs
don't parse shows its `file:line` diagnostics instead of crashing the
session.

| Key | Action |
| --- | --- |
| `←` `→` | previous / next project |
| `⇥` | cycle view: specs → artifacts → dashboard |
| `↑` `↓` | select in the current view |
| `/` | filter the decision list by id/title (Esc clears, Enter keeps for browsing) |
| `⏎` | page the detail pane down |
| `esc` | detail pane back to top |
| `r` | reload every project from disk |
| `e` | edit a spec file (§5) |
| `p` | open a draft PR (§6) |
| `q` | quit |

## 3. Browse the specs (default view)

The decision list is colored by status — accepted green, proposed yellow,
superseded dimmed, rejected red — and each decision also carries a **proof
mark** from the last gate report: `✓ proven` (green) when every row mapped to
it passed, `◐ partial` (yellow) when only some did, `○ unproven` (dark gray)
when nothing passed on record. Superseded and rejected decisions are outside
coverage and show a dim `—`. A decision added after the last gate run honestly
shows `○ unproven`; a project with no report at all says `no gate run` in the
pane title instead of inventing marks.

Long list? Press `/` and type: the list narrows live by case-insensitive
substring over id and title, with the query and match count shown above it.
While filter mode is engaged, printable keys type into the query — **Esc**
clears the query and hands the keys back (then `q` quits again), while
**Enter** keeps the query and leaves filter mode so `↑`/`↓` browse the
matches. Switching projects resets the filter. Selecting a decision shows its detail
pane: context and consequences, then its requirements (id, layers, EARS
text), scenarios, invariants, and infra policies.

## 4. Preview artifacts and the dashboard

`⇥` to **artifacts**: every file `decispec gen` would write, rendered in
memory from the live workspace — diagrams, per-layer test skeletons, and
the decision index (`docs/decisions.md`). Select a path to preview its
contents. Nothing is written to the project; run `decispec gen` in the
project when you actually want the files.

`⇥` again to **dashboard**: the project's active decision levels
(accepted + proposed counted as active), requirement counts per layer, and
the last gate run — verdict, row counts, failures, and warnings, read from
the project's `.decispec/report.json` — plus the proof-coverage headline
(`coverage: N/M decisions proven (P%)`) directly under the verdict. No report
yet means no gate section and an explicit `coverage: no gate run` line; run
`decispec gate` in the project to produce one.

## 5. Edit or create a spec

Press `e`, pick a spec file with `↑` `↓` + `⏎` — or press `/` to
type-to-filter (case-insensitive substring; `Esc` clears the filter, a
second `Esc` leaves the picker) — or press `n` to create a new one.
A project with no spec files goes straight to the name prompt.
The name is relative to `specs/` and must end in `.spec` (subdirectories
like `payments/010.spec` work); `..` and names that collide with an
existing file are refused. Confirming a name opens the editor on a minimal
PREFIX-001 decision + spec skeleton that already parses clean — rename the
placeholders and fill in the text; the file is written only when you save.

The editor re-parses your buffer **in the context of the whole workspace**
on every keystroke, so cross-file references (a scenario naming another
file's container, a decision id used elsewhere) are validated as you type.
Parse errors come first, phase by phase, in a pane under the text.

- `Ctrl+S` — save into the project, then immediately re-check the
  workspace: you either see `ok: N decision(s)` or the exact `file:line`
  failures. A broken save is reported, never hidden.
- `esc` — close a clean buffer. On a modified buffer it arms a discard
  confirmation; press `esc` again to discard. Typing anything disarms the
  confirmation.
- `Tab` types two spaces (specs are two-space indented). `Home`/`End`,
  `PgUp`/`PgDn`, arrows all work as expected.

A project that failed to load is still editable — the picker works from
its on-disk spec files, which is how you fix it.

## 6. Open a draft PR

With saved spec changes in a git-registered project, press `p`. The TUI:

1. checks readiness — the project is a git repository, the tree has
   uncommitted changes, `gh` is installed;
2. creates a branch `spec/tui-<UTC timestamp>` (one per PR);
3. commits **only `specs/`** — anything else staged from outside `specs/`
   is refused and the worktree is restored;
4. pushes the branch and runs `gh pr create --draft --base main`.

The PR body says it was opened by `decispec tui` and to review before
merging. Nothing is ever merged from the TUI. If a step fails (not a git
repo, clean tree, `gh` missing, push rejected, changes outside `specs/`),
you get a plain message in the status bar and the session continues.

## 7. Troubleshooting

- **Sidebar shows a red dot / "load error"** — the project's specs failed
  to parse or its `decispec.toml` is missing. The error pane shows the
  `file:line` diagnostics; press `e` to fix the specs in place, then `r`
  to reload.
- **Empty sidebar** — fleet mode (`--all` or a cwd outside any project) with
  an empty registry; `decispec tui --register <dir>` adds a project (the
  directory must contain a `decispec.toml`). Run inside a project, the
  cockpit never shows a sidebar at all — it opens on the project directly.
- **Dashboard has no gate section** — no `.decispec/report.json` yet; run
  `decispec gate` inside the project.
- **`p` says "PR needs a loaded project"** — fix the load error first
  (§5 works on broken projects exactly for this).
- **`p` says "no uncommitted changes"** — save a spec edit (`Ctrl+S`)
  first.

## 8. Exit codes

`0` on clean quit, `2` on terminal/registry setup failure. The TUI never
exits `1` — findings are for batch commands (`check`, `gate`).
