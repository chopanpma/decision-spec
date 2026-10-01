# Cross-agent review process

Every task finishes in `review` before it is `done` (see `.coord/BOARD.md` and
`AGENTS.md` §2). The reviewer is always an agent **other than the author**.
Reviews are how refactors and design changes get proposed between agents, not
just how bugs get caught.

## Picking up a review

1. Board shows a row in `review` you did not author.
2. Read `.coord/claims/<ID>.md` first: task, claimed scope, test plan, change
   summary, build evidence.
3. Re-run `cargo build && cargo test` yourself. Trust no pasted output.

## Review checklist

**Correctness**
- [ ] The change does what the claim says, nothing more (no scope creep).
- [ ] Exit-code contract holds: `0` clean / `1` findings / `2` cannot-complete.
      New failure modes are classified correctly, not forced into `1`.
- [ ] Errors are actionable: `file:line` or named entity, no bare `unwrap`/`?`
      chains that swallow context in library code.

**TDD evidence** (the core check — see `AGENTS.md` §1)
- [ ] Tests for the new behavior exist and are real: each asserts observable
      behavior and *could* fail.
- [ ] Spot-check: pick the most important new test and confirm it fails if the
      implementation is reverted (temporarily revert in a scratch copy, or
      reason it through line by line if reverting is impractical).
- [ ] Bug fixes ship a regression test that fails without the fix.
- [ ] No `#[ignore]`, no commented-out assertions, no tautologies.

**Architecture fit** (invariants from `AGENTS.md` §3)
- [ ] Libraries stay pure; I/O only in `cli`.
- [ ] Generated/matched names go through `ir::names` — no duplicated format
      strings between codegen and gate.
- [ ] Existing patterns are reused (Visitor traversal uniformity, Adapter
      record, Null Object for skipped toolchains) instead of adding parallel
      machinery.

**Docs**
- [ ] Behavior changes come with README/ARCHITECTURE updates (either in the
      change or queued under "Proposed doc updates" in the claim).
- [ ] MVP limitations updated if a limitation was removed.

## Verdict

Write `.coord/reviews/<ID>.md`:

```
# Review F8 — <task> — reviewer: <agent>
Verdict: APPROVE | APPROVE-NITS | REQUEST-CHANGES
## Findings (blocker / major / minor / nit)
## Recommendations (refactors, follow-ups)
```

- **blocker** — wrong behavior, broken contract, data loss. Row goes back to
  `claimed`.
- **major** — real defect or missing test coverage of the new surface. Row
  goes back to `claimed`.
- **minor / nit** — style, clarity, simplifications. `APPROVE-NITS`; author
  addresses at their discretion but must record accept/decline + one-line
  reason in the claim file.

## Recommendations section (required, not optional)

Every review must include at least one considered **recommendation** — a
refactor, a simplification, a pattern the author may not have seen, or an
explicit "no changes recommended, because …". Silence is not a verdict. This
is the mechanism by which agents improve each other's work instead of
accumulating drift.

Re-review after REQUEST-CHANGES follows the same flow; the reviewer should
verify only the findings and any new code they caused.
