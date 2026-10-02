# Writing `.spec` files

A guide for humans and agents authoring DecisionSpec specs. Complements the
DSL reference in [README.md](../README.md#the-dsl) — that documents what the
grammar allows; this documents what makes a spec worth writing and how to
write it well. Rules are imperative and opinionated on purpose.

## 1. What makes a spec worth writing

Write a decision block only for a choice whose violation has a cost you can
name. Everything else is a comment's job. Rank candidates by that cost:

- **L1 — decisions and invariants.** Architectural choices that are expensive
  to reverse (data models, isolation levels, egress routes, signing
  boundaries) and invariants that must hold no matter what. These earn
  `decision` blocks with requirements and scenarios.
- **L2 — user-visible behavior.** Behavior a user can observe and a test can
  assert end to end. Covers requirements and scenarios.
- **L3 — implementation detail.** Function layout, class names, file
  organization. Excluded on purpose: the code already says it, and a spec
  that duplicates the code rots the day after it is written.

The selection criterion is cost-if-violated. If violating the "decision"
would cost nothing, it is not a decision — delete the block.

## 2. Anatomy of a decision block

One `decision` + one `spec` block per choice, in one file per domain module
under `specs/` (register each ID prefix once per project). A port's first
slice may bundle its initial handful of decisions in one `NNN-decisions.spec`;
split into one file per domain module as the count grows — tvgo's
`001-decisions.spec` is exactly that bootstrap bundle.

### One decision = one architectural choice

The title states the CHOICE, not the topic.

- DO: `"Every Samsung call leaves through the rented static-IP proxy"`
- DON'T: `"Proxy config"`

If the title names a subject area instead of a commitment, split the block
until each title can be read as a sentence the system enforces.

### Context = evidence + why now

Cite measurements, with dates. "We chose X because on DATE we measured Y"
outlives "X is best practice" by years.

- DO: `measured on 2026-09-03 through an ordinary session: 22104 allocated
  against an instalment recording 11052, both calls answering success`
- DON'T: `payments must be reliable`

### Consequences = + gains and − costs, both honest

A decision with no minus is not finished. Every real choice trades something;
the minuses are what the next reader needs to re-decide correctly.

- DO: `"+ redelivery returns the recorded transaction. - the whole
  transaction is retried, bounded at four attempts. - these tests need a
  real migrated PostgreSQL, never SQLite."`
- DON'T: `"+ faster and safer payments"`

### The brownfield rule

On a codebase that already exists, requirements are propositions about
behavior the system ALREADY exhibits. The gate's job is to keep true what is
true. If the code does not do it yet, write the decision with `status:
proposed` — that is what the status is for.

### Status lifecycle

`proposed` → `accepted` once the gate rows pass and the choice is ratified.
`superseded` when a later decision replaces it — and `superseded` carries
`superseded_by:` pointing at the replacement (the parser rejects a
superseded status without the pointer). `rejected` for recorded-and-refused:
the choice was considered and will not be made, and the record stops the
next person reopening it from scratch. The default is `proposed`.

### IDs

`PREFIX-NNN` per decision, requirements `PREFIX-NNN-RM`, scenarios
`PREFIX-NNN`-free snake_case words. Register each prefix once per project —
one file per domain module keeps the registry legible. IDs match
`[A-Za-z0-9_-]+` and must be unique across the whole workspace.

### One scenario = one testable behavior

The scenario id IS the glue function name (snake_case) — generated tests call
`tests.glue.<scenario_id>()`, so name it after the behavior, and write
given/when/then against concrete code (function, class, module), never UI
poetry.

- DO: `when: "app.payments.apply runs"`, `then: "db.Serialisable.of raises
  NotSerialisable ... and no write happened"`
- DON'T: `when: "the user pays"`, `then: "everything works nicely"`

A scenario that asserts two behaviors is two scenarios. Split it.

### Requirement text = EARS

One requirement = one sentence:

```
WHEN <trigger> THEN <system> SHALL <observable response>
```

Ban vague adjectives: fast, user-friendly, appropriately, robustly. If the
response cannot be observed by a test, it is not a requirement yet. The
pattern is Easy Approach to Requirements Syntax — Mavin, A. et al., *EARS:
Easy Approach to Requirements Syntax*, IEEE RE'09 — read it before writing
more than a handful of requirements.

## 3. Worked example

From the tvgo project's `specs/001-decisions.spec` (a reviewed, gated spec;
quoted verbatim):

```sdl
decision MONEY-001 "Applying a payment runs in a SERIALIZABLE transaction, retried whole on 40001" {
  status: accepted
  context: "Two payments arriving together can each read an instalment as unpaid and each allocate against it; measured on 2026-09-03 through an ordinary session: 22104 allocated against an instalment recording 11052, both calls answering success and nothing raising. The database is PostgreSQL deliberately: it implements SERIALIZABLE with snapshot isolation and a real anomaly detector that aborts retryably with 40001, where MySQL implements the same keyword with locking reads and fails as a lock wait timeout instead. Webhooks redeliver and users double click, so duplicate delivery is certain and a payment is identified by the provider key, checked inside the same transaction."
  consequences: "+ two concurrent payments cannot both land on one instalment, and a redelivery returns the already-recorded transaction instead of a second one. - the whole transaction is retried, bounded at four attempts, and exhausting them raises rather than answering success. - the money path must be the only caller of PaymentRepository.apply, or the witness is obtained honestly and the retry loop skipped. - these tests need a real migrated PostgreSQL, never SQLite, because the anomaly detector and the CHECK constraints are the mechanism being specified."
}
```

Why it works, element by element:

- **Title** — states the choice and its mechanism ("SERIALIZABLE
  transaction, retried whole on 40001"), not the topic ("Payment
  integrity"). A reader can tell what the system does differently from the
  title alone.
- **Context** — carries the measured incident with a date (22104 allocated
  against an instalment recording 11052, on 2026-09-03), then the why-now
  (webhooks redeliver; duplicate delivery is certain). No adjectives.
- **Consequences** — one plus, three minuses. The minuses (retry bound,
  single-caller constraint, real-Postgres test cost) are the price of the
  decision written down, not hidden.

## 4. Anti-patterns

| Symptom | Why it fails | Fix |
| --- | --- | --- |
| Decision titled by topic ("Auth", "Proxy config") | No commitment to gate; the block can never fail | Retitle to state the choice; split until it does |
| Context with no evidence ("users expect reliability") | Unverifiable; rots; gives the gate nothing | Cite a measurement with a date, or delete the block |
| Requirements speculating about unbuilt features on a brownfield repo, marked `accepted` | The gate fails forever; the spec becomes noise | Gate what is true; mark the aspiration `proposed` |
| One scenario asserting two behaviors | One failing half makes the row unreadable; glue can't name one failure | Split into two scenarios; each gets its own glue function |
| Scenario named after the screen or the user ("happy_path", "payment_page") | The id IS the glue function name; screen names rot and say nothing about the code | Name the behavior after the code it exercises, snake_case |
| Copy-pasting the code's structure into the spec (class-per-class containers) | That is L3; it duplicates the code and rots with it | Lift to L1/L2: name the invariant or the observable behavior |
| Consequences with only "+" entries | The decision is not finished; the cost is the decision | Write the minuses, honestly |

## 5. Writing glue for your scenarios

Generated tests call `tests.glue.<scenario_id>()` directly. The glue is the
only hand-written test code. Two tiers:

**Tier 1 — delegate.** If the repository's own suite already proves the
behavior, the glue function shells out to that exact test
(`pytest path/to/test_file.py::test_exact_name -q`; exit 0 = pass). Name the
node id exactly. A rename breaks the row loudly, not silently. Example: the
tvgo glue delegates `witness_refuses_ordinary_session` to
`backend/tests/test_payments.py::test_aplicar_por_una_sesion_ordinaria_no_se_puede_escribir`.

**Tier 2 — probe.** When no test exists, write a small script the glue runs
with the project's own toolchain (see tvgo's `tests/glue/_probe_*.py`).
Construction and patching only — never send a request to a metered or
external service.

Conventions, whatever the tier:

- Run in a credential-free environment. If a test needs keys to pass, it is
  testing your laptop, not the code.
- Never call metered or external services. Patch the transport; assert zero
  calls were attempted.
- Skip quietly when prerequisites are absent (no venv, no database, no
  node_modules) — a missing prerequisite is a skip, never a failure.
- Clean up what you seed. Probes that insert rows must delete them, or the
  next test run inherits your data.

The reference implementation is tvgo's `tests/glue/__init__.py` (tier
delegation, probes, skip conventions, residue-free seeding).

## 6. Growing coverage

- Order decisions by cost-if-violated, not by module. The next block is the
  one whose violation would hurt most this quarter.
- Budget one decision block ≈ 30–60 minutes including gate rows. Title,
  measured context, honest consequences, 1–3 requirements, 1–3 scenarios,
  glue.
- After editing specs, run `decispec gen` to regenerate artifacts and
  `decispec index` to refresh the repo-readable index.
- Run `decispec check` before every commit — malformed specs are findings
  (exit 1), and CI should never see them.
