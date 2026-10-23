"""Pin self-check core (F34): the bijection between workspace Rust tests and
glue delegations.

`parse_list_output` / `extract_pins` / `audit_pins` are pure (unit-tested in
test_pin_audit.py). `enumerate_workspace` shells out to `cargo test --list`
per package — it lives here so the glue function stays a one-liner.
"""

import json
import re
import subprocess

# ---------------------------------------------------------------------------
# pure core
# ---------------------------------------------------------------------------


def parse_list_output(text):
    """`cargo test -- --list` stdout -> sorted test paths.

    Keeps `<path>: test` lines, drops blanks and the `N tests, 0 benchmarks`
    summary.
    """
    tests = []
    for line in text.splitlines():
        line = line.strip()
        if line.endswith(": test"):
            tests.append(line[: -len(": test")].strip())
    return tests


_PIN_RE = re.compile(r'^\s*_delegate\(\s*"([^"]+)"\s*,\s*"([^"]+)"', re.MULTILINE)


def extract_pins(glue_source):
    """tests/glue/__init__.py source -> [(package, dotted_test_path)] in order.

    Line-anchored: real delegations are the first thing on their line, so a
    `_delegate(...)` mention inside a string literal or comment is ignored.
    """
    return _PIN_RE.findall(glue_source)


def audit_pins(enumerated, pinned):
    """Compare {package: [test paths]} against [(package, path)] pins.

    Returns a sorted list of human-readable problems; empty means the
    bijection is exact: no unpinned test, no ghost pin, no duplicate pin.
    """
    problems = []
    pin_counts = {}
    for pkg, path in pinned:
        pin_counts[(pkg, path)] = pin_counts.get((pkg, path), 0) + 1
    for (pkg, path), n in sorted(pin_counts.items()):
        if n > 1:
            problems.append(f"{pkg}::{path} is pinned twice")
        if path not in enumerated.get(pkg, []):
            problems.append(f"ghost pin {pkg}::{path} names no workspace test")
    for pkg, paths in sorted(enumerated.items()):
        pinned_paths = {p for (p_, p) in pinned if p_ == pkg}
        for path in paths:
            if path not in pinned_paths:
                problems.append(f"unpinned test {pkg}::{path} has no scenario")
    return sorted(problems)


# ---------------------------------------------------------------------------
# enumeration (subprocess — runs inside the gate's pytest process)
# ---------------------------------------------------------------------------


def enumerate_workspace(cargo, root):
    """Every #[test] in the workspace: {package: [test paths]}.

    Packages come from `cargo metadata`, so a future crate is enumerated
    automatically and its tests demand pins. One `--tests` pass per package
    covers lib, bin, AND integration (`<crate>/tests/*.rs`) targets — a
    target kind `--lib`/`--bins` would silently miss. A package with no test
    targets is a cargo no-op (exit 0); any real failure propagates as a
    RuntimeError.
    """
    meta = subprocess.run(
        [cargo, "metadata", "--no-deps", "--format-version", "1"],
        cwd=root,
        capture_output=True,
        text=True,
    )
    if meta.returncode != 0:
        raise RuntimeError(f"cargo metadata failed:\n{meta.stdout}{meta.stderr}")
    packages = [p["name"] for p in json.loads(meta.stdout)["packages"]]
    enumerated = {}
    for pkg in sorted(packages):
        listed = subprocess.run(
            [cargo, "test", "-p", pkg, "--tests", "--", "--list"],
            cwd=root,
            capture_output=True,
            text=True,
        )
        if listed.returncode != 0:
            raise RuntimeError(
                f"cargo test --list failed for {pkg} --tests:\n"
                f"{listed.stdout}{listed.stderr}"
            )
        enumerated[pkg] = sorted(set(parse_list_output(listed.stdout)))
    return enumerated
