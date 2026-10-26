# Portable specs — prefix PORT.
# Authored 2026-10-22 (F44), implemented 2026-10-23 (F48): both decisions are
# status: accepted and every scenario carries a glue delegate to the cli Rust
# test that proves it (F48 landed the commands; the spec-first red gate the
# quickstart documents was the 6 PORT rows going red-then-green).
#
# Why these two commands are one domain module: an external spec store is
# only useful if specs can leave a repo (extract --to) AND enter a new
# project (init --from). Each is trivial alone; the pair makes specs
# portable artifacts that outlive any single checkout.

decision PORT-001 "decispec extract --to writes the draft spec outside the source repo" {
  status: accepted
  context: "User requirement 2026-10-22: specs for the projects are stored apart from the projects, in a store directory under the user's control. Today extract writes specs/extracted.spec inside the analyzed project or prints to stdout (CMD-003 pins the in-repo default and its refusal to overwrite). Both fight the store: writing inside the analyzed repo dirties a foreign checkout, and a store that lives one directory per repo defeats the point of a store. The analyzed repo is often a foreign project without a decispec.toml, so the written file is named after the project's directory, which always exists."
  consequences: "+ one store directory can hold many projects' drafts, sit next to the TUI registry in user space, and be backed up, versioned, or shared independently of any checkout. - the write crosses the repo boundary, so the refusal rules matter: an existing target is never overwritten and an unwritable target fails loudly, both exit 2. - --to and --stdout are mutually exclusive; asking for both is a usage error naming the conflict, not a guessed priority."
}

spec PORT-001 {
  requirement PORT-001-R1 (EARS) {
    text: "WHEN extract runs with --to <dir> THEN it SHALL write <dir>/<dirname>.spec, where <dirname> is the analyzed project's directory name, without writing into the analyzed project's own specs/ tree and exit 0, and when the target file already exists or cannot be written it SHALL exit 2 leaving any existing file byte-identical"
    layers: [unit]
    scenarios: [extract_to_writes_named_spec_outside_the_repo, extract_to_refuses_to_overwrite_existing_file]
  }
  requirement PORT-001-R2 (EARS) {
    text: "WHEN --to and --stdout are given together THEN extract SHALL exit 2 naming the mutual exclusion and SHALL write nothing"
    layers: [unit]
    scenarios: [extract_to_and_stdout_are_mutually_exclusive]
  }
  scenario extract_to_writes_named_spec_outside_the_repo {
    given: "an analyzable project at /work/tvgo and an empty store directory outside it"
    when: "extract --to ~/specs runs against /work/tvgo"
    then: "~/specs/tvgo.spec exists, no specs/extracted.spec was written inside /work/tvgo, and the exit code is 0"
    # delegates: decispec (cli) tests::extract_to_writes_named_spec_outside_the_repo
  }
  scenario extract_to_refuses_to_overwrite_existing_file {
    given: "a store directory that already holds tvgo.spec"
    when: "extract --to that directory runs against /work/tvgo again"
    then: "it exits 2 and the existing tvgo.spec is byte-identical"
    # delegates: decispec (cli) tests::extract_to_refuses_to_overwrite_existing_file
  }
  scenario extract_to_and_stdout_are_mutually_exclusive {
    given: "an extract invocation carrying both --to and --stdout"
    when: "it runs"
    then: "it exits 2 naming the conflict and neither writes a file nor prints a draft"
    # delegates: decispec (cli) tests::extract_to_and_stdout_are_mutually_exclusive
  }
}

decision PORT-002 "decispec init --from scaffolds a new project from stored specs" {
  status: accepted
  context: "User requirement 2026-10-22: the store must be usable to create a project from its specs. Today init ships an empty glue package and optionally the AUTH-001 demo (README quickstart) — a new project starts from zero decisions. The inverse path makes the store write-read instead of write-only: point init at stored .spec files or at a store directory and the new project opens with its decisions, requirements, and scenarios already loaded. The copied specs are validated as a workspace before the scaffold is written, so a broken store file cannot produce a half-born project."
  consequences: "+ a project's spec travels ahead of its code: a fresh directory becomes a checked, decision-first project in one command. - the copied specs are the author's responsibility — global ID uniqueness is enforced by check on first run, and the glue package starts empty, so the gate is red until glue lands (the spec-first red gate the quickstart documents). - directories are read one level deep; a nested store layout needs one --from argument per directory."
}

spec PORT-002 {
  requirement PORT-002-R1 (EARS) {
    text: "WHEN init runs with --from paths in a directory whose specs/ tree holds no .spec files THEN it SHALL write decispec.toml and tests/glue/ and copy each given .spec file and each given directory's direct .spec children into specs/ under their original names"
    layers: [unit]
    scenarios: [init_from_files_and_dirs_scaffolds_a_project, init_from_refuses_to_clobber_existing_specs]
  }
  requirement PORT-002-R2 (EARS) {
    text: "WHEN the specs selected by --from fail to parse or validate as a workspace THEN init SHALL exit 2 reporting the file:line diagnostics and SHALL not write the scaffold"
    layers: [unit]
    scenarios: [init_from_broken_specs_exit_2_naming_diagnostics]
  }
  scenario init_from_files_and_dirs_scaffolds_a_project {
    given: "a store directory holding tvgo.spec and verydian.spec, plus an empty target directory"
    when: "decispec init --from <store> runs in the target directory"
    then: "specs/ contains both files under their original names, decispec.toml and tests/glue/ exist, and decispec check exits 0 in the new project"
    # delegates: decispec (cli) tests::init_from_files_and_dirs_scaffolds_a_project
  }
  scenario init_from_refuses_to_clobber_existing_specs {
    given: "a directory whose specs/ already holds a .spec file"
    when: "decispec init --from <store> runs there"
    then: "it exits 2 and the existing spec file is untouched"
    # delegates: decispec (cli) tests::init_from_refuses_to_clobber_existing_specs
  }
  scenario init_from_broken_specs_exit_2_naming_diagnostics {
    given: "a store file whose string literal is never terminated"
    when: "decispec init --from <store> runs in an empty directory"
    then: "it exits 2 with file:line diagnostics and writes nothing — no decispec.toml, no specs/, no glue"
    # delegates: decispec (cli) tests::init_from_broken_specs_exit_2_naming_diagnostics
  }
}
