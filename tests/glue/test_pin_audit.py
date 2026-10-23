"""Unit tests for the pin self-check's pure core (F34).

The end-to-end check is the gate scenario `every_rust_test_pinned_exactly_once`
whose glue enumerates the workspace and compares; these tests pin the
comparison logic itself with synthetic inputs.

Run: python3 -m pytest tests/glue/test_pin_audit.py
"""

from tests.glue.pin_audit import audit_pins, extract_pins, parse_list_output


class TestParseListOutput:
    def test_parses_test_lines_and_ignores_summary(self):
        out = "\n".join(
            [
                "tests::alpha: test",
                "app::tests::beta: test",
                "",
                "2 tests, 0 benchmarks",
            ]
        )
        assert parse_list_output(out) == ["tests::alpha", "app::tests::beta"]

    def test_empty_output_yields_nothing(self):
        assert parse_list_output("0 tests, 0 benchmarks\n") == []


class TestExtractPins:
    def test_extracts_package_and_path_pairs(self):
        src = (
            'def a():\n'
            '    _delegate("decispec-tui", "app::tests::e_opens")\n'
            'def b():\n'
            '    _delegate("decispec", "tests::tui_register", target="--bins")\n'
        )
        assert extract_pins(src) == [
            ("decispec-tui", "app::tests::e_opens"),
            ("decispec", "tests::tui_register"),
        ]

    def test_ignores_non_delegate_strings(self):
        src = 'x = "_delegate(\\"fake\\", \\"nope\\")"  # inside a string, not a call\n'
        assert extract_pins(src) == []


class TestAuditPins:
    ENUM = {
        "decispec-tui": ["app::tests::a", "app::tests::b"],
        "decispec": ["tests::c"],
    }

    def test_clean_bijection_passes(self):
        pinned = [
            ("decispec-tui", "app::tests::a"),
            ("decispec-tui", "app::tests::b"),
            ("decispec", "tests::c"),
        ]
        assert audit_pins(self.ENUM, pinned) == []

    def test_unpinned_test_is_named_with_its_package(self):
        pinned = [
            ("decispec-tui", "app::tests::a"),
            ("decispec", "tests::c"),
        ]
        problems = audit_pins(self.ENUM, pinned)
        assert len(problems) == 1
        assert "decispec-tui" in problems[0]
        assert "app::tests::b" in problems[0]
        assert "unpinned" in problems[0]

    def test_ghost_pin_is_named(self):
        pinned = [
            ("decispec-tui", "app::tests::a"),
            ("decispec-tui", "app::tests::b"),
            ("decispec", "tests::c"),
            ("decispec", "tests::renamed_away"),
        ]
        problems = audit_pins(self.ENUM, pinned)
        assert len(problems) == 1
        assert "tests::renamed_away" in problems[0]
        assert "ghost" in problems[0]

    def test_duplicate_pin_is_named(self):
        pinned = [
            ("decispec-tui", "app::tests::a"),
            ("decispec-tui", "app::tests::a"),
            ("decispec-tui", "app::tests::b"),
            ("decispec", "tests::c"),
        ]
        problems = audit_pins(self.ENUM, pinned)
        assert len(problems) == 1
        assert "app::tests::a" in problems[0]
        assert "twice" in problems[0]

    def test_multiple_problems_are_all_reported(self):
        pinned = [("decispec-tui", "app::tests::a"), ("decispec", "tests::nope")]
        problems = audit_pins(self.ENUM, pinned)
        assert len(problems) == 3  # two unpinned + one ghost

    def test_empty_workspace_passes_vacuously(self):
        assert audit_pins({}, []) == []
