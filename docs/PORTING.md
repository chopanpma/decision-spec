# Porting an existing project to DecisionSpec

For projects already in development (including production ones). The port is
**two deterministic passes that the tool does for you**, plus **one
interpretive pass that agents or humans must do** — because intent was never
written down anywhere a parser can reach it.

## What comes from where

| Layer | Source | Who does it |
| --- | --- | --- |
| Structure: containers + relationships | the code itself (layout + import graph) | `decispec extract` — deterministic |
| Decisions (title, status, context, consequences) | ADR/markdown decision docs (`docs/adr/…`) | `decispec extract` — deterministic |
| Requirements + scenarios + layers | nowhere — intent lives in code and heads | **agents propose, humans review** |
| Test glue (`tests/glue/`) | your test conventions | humans (agents can draft from your existing tests) |

## Pass 1–2: extraction (minutes, no agents)

```sh
cd your-project
decispec init            # scaffold: decispec.toml, specs/, tests/glue/
decispec extract         # draft: decisions from ADRs + model from code
decispec check           # must exit 0
decispec gen             # diagrams + skeletons
```

`extract` analyzes one language family per run (the majority wins). A
full-stack repo (e.g. Python backend + TypeScript frontend) gets one run per
subtree — `decispec extract backend`, `decispec extract frontend` — each
writes its draft under that subtree's `specs/`; move both into the project's
`specs/` and curate (raw drafts name top-level directories; rename the
containers to your real architectural units and expect noise like test and
migration dirs — see tvgo's port for a worked example).

Then review the draft (`specs/extracted.spec`):

- Rename containers that extracted poorly (e.g. umbrella children that need
  real names).
- Fix every `superseded_by` TODO left by deprecated/superseded ADRs.
- Commit it. Convention: rename the reviewed file to something like
  `specs/000-decisions-and-model.spec` — `extract` refuses to overwrite
  `extracted.spec`, and keeping the name free lets you re-run extraction later
  to **diff code reality against your committed specs** (poor man's drift
  detection).

If the project has no ADRs, pass 2 yields nothing — that's fine; agents start
from the model block.

## Pass 3: the interpretive port (agents, reviewed by humans)

For each decision (cheapest first, riskiest soon after):

1. **An agent reads** the decision block + the ADR source + the code it
   governs, and **proposes** `requirement` blocks in EARS style with layers,
   referencing existing scenarios where the behavior is already tested.
2. **A human reviews** the proposal against the checklist in
   `docs/REVIEW.md` (would this requirement fail if the decision were
   violated? is the layer honest about what will actually be tested?).
3. **Write the glue** in `tests/glue/` (agents draft it from your existing
   tests; you adjust imports and fixtures).
4. `decispec gate` → the decision's rows go green.

Never bulk-import agent proposals unreviewed. A requirement that doesn't
match reality is worse than no requirement — it greens the matrix falsely.

## Rolling out the gate

- **Expect the gate to be red** for a while: accepted/proposed decisions
  without requirements are gate failures by design. That is the tool showing
  exactly what is unverified — the list IS the work queue.
- **Production projects:** run `decispec gate` locally and in CI as an
  informational step (`decispec gate --json`, archive `.decispec/report.json`)
  but do not block merges on it until a meaningful slice of decisions is
  covered. When you flip it to blocking, add `--strict-skipped`.
- **CI without toolchains installed:** keep layers whose toolchains CI lacks
  out of requirements until installed, or they report SKIPPED (a pass unless
  `--strict-skipped`).

## Working multiple projects with agents

Use the coordination protocol (`AGENTS.md` §2): one claim per project on
`.coord/BOARD.md`, agents port non-overlapping decision sets, reviewers are
always a different agent than the author. The DecisionSpec repo's own
`.coord/` history (F8, F14, F15) is a worked example of the flow.

## Agent tooling: the MCP server

Agents porting a project should use `decispec mcp` (MCP over stdio) instead
of shelling out — typed tools, structured errors, no path/exit-code plumbing:

```json
{ "mcpServers": { "decispec": { "command": "decispec", "args": ["mcp"], "cwd": "/path/to/project" } } }
```

The tools that matter for porting:

| Tool | Use in the port |
| --- | --- |
| `validate_spec` | Iterate on a proposed requirement/scenario block — file:line diagnostics, nothing written to disk until it's valid |
| `workspace_overview` | The work queue: decisions without requirements, requirements without scenarios, unfixed superseded TODOs |
| `extract_project` | Re-bootstrap or compare code reality against committed specs |
| `check_workspace` / `gate_status` | Progress checks after edits / last gate verdict |

Recommended agent loop per decision: read the decision + code → draft
requirement text → `validate_spec` → human review → apply to `specs/*.spec`
→ `check_workspace`. Full tool list and schemas: `tools/list` against the
server, or README's MCP section.

## Verifying a port is complete

`decispec query <ID>` per decision: requirements exist, layers map to
artifacts, last gate status is recorded. Then `decispec gate --strict-skipped`
exit 0 — the matrix is fully green with nothing skipped.
