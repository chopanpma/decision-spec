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

## 3. The daily loop

```sh
decispec check     # 1. parse + validate specs (file:line errors; exit 1 = findings)
decispec gen       # 2. render diagrams + test skeletons (idempotent, safe to re-run)
# 3. implement tests/glue/ — the only hand-written test code
decispec test      # 4. run pytest/cucumber/playwright/conftest/import-linter...
decispec gate      # 5. matrix + verdict; exit 0 = compliant, 1 = violation
decispec query AUTH-001   # why is this decision here, what covers it, last gate status
decispec index     # 6. GitHub-renderable docs/decisions.md (diagrams + tables)
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

Unmapped containers fall back to their id with a TODO note in the generated
`.importlinter`.

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
- run: decispec gate --strict-skipped
```

`--strict-skipped` turns "toolchain not installed" (`SKIPPED`) into `FAIL` —
without it a layer with no toolchain silently passes. Use it in CI, not while
hacking locally.

## 7. Reading the gate

- A row fails when a mapped layer has no artifacts, an executed test failed,
  or the toolchain ran but the item has no result (`UNCOVERED`).
- Contract/e2e matching is by generated test name; a naming mismatch degrades
  that row to suite-level judgment with a printed `WARNING`.
- Superseded decisions warn instead of failing (plus a coverage-drift warning
  if tests still map to them).
- `--json` emits `.decispec/report.json`; every run is recorded in
  `.decispec/decispec.db` (`decispec query` reads it).

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

## 9. Troubleshooting

- **gate exits 2 right after editing specs** → run `decispec check`; it prints
  `file:line` diagnostics. (check = findings/1; gen/gate = abort/2.)
- **everything SKIPPED** → toolchains not installed; that's a pass by default.
- **stale artifacts** → you edited a spec id; re-run `gen` — it sweeps files
  whose header says DecisionSpec but whose item no longer exists.
- **green locally, red in CI** → almost always `--strict-skipped` catching a
  toolchain CI doesn't have.
