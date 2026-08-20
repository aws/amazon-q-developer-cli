"""Tests for the committed-limit direction checker."""

from __future__ import annotations

import json
import subprocess
import sys
import unittest
from pathlib import Path

TESTS_DIR = Path(__file__).resolve().parent
CODE_QUALITY_DIR = TESTS_DIR.parent
SCRIPT = CODE_QUALITY_DIR / "check-baseline-direction.py"
sys.path.insert(0, str(CODE_QUALITY_DIR))

import sa_lib  # noqa: E402
from helpers import FixtureTree, run_script  # noqa: E402

FLOORS = {
    "packages/app": {
        "lines": 80.0,
        "functions": 70.0,
        "measured": {"lines": 100, "functions": 20},
    }
}
DEBT = {"complexity": 5, "max-depth": 2}
DUPLICATION = {"clones": 7}
PERIMETER = {
    "mode": "mild",
    "minLines": 10,
    "minTokens": 75,
    "format": ["typescript", "tsx"],
    "path": ["packages/app/src", "packages/other/src"],
    "include": ["packages/app/src/**"],
    "ignore": ["**/node_modules/**"],
}


def _git(tree: FixtureTree, *args: str) -> None:
    subprocess.run(
        ("git", "-C", str(tree.root), *args),
        check=True,
        capture_output=True,
        text=True,
    )


def _commit(tree: FixtureTree, message: str) -> None:
    _git(tree, "add", "-A")
    _git(
        tree,
        "-c",
        "user.email=gate@example.invalid",
        "-c",
        "user.name=gate",
        "commit",
        "-qm",
        message,
    )


def _committed_tree() -> FixtureTree:
    tree = FixtureTree(
        {
            ".coverage-floors.json": json.dumps(FLOORS),
            ".lint-debt-baseline.json": json.dumps(DEBT),
            ".jscpd-baseline.json": json.dumps(DUPLICATION),
            ".jscpd.json": json.dumps(PERIMETER),
        },
        parent=TESTS_DIR,
    )
    tree.__enter__()
    _git(tree, "init", "-q", ".")
    _commit(tree, "base")
    return tree


def _run(tree: FixtureTree):
    return run_script(SCRIPT, "--base-ref", "HEAD", "--repository", ".", cwd=tree.root)


class BaselineDirectionTests(unittest.TestCase):
    def test_unchanged_limits_exit_zero(self) -> None:
        tree = _committed_tree()
        try:
            result = _run(tree)
            self.assertEqual(result.returncode, sa_lib.EXIT_CLEAN, result.stderr)
        finally:
            tree.__exit__(None, None, None)

    def test_tightening_every_limit_exits_zero(self) -> None:
        tree = _committed_tree()
        try:
            tree.write_json(
                ".coverage-floors.json",
                {
                    "packages/app": {
                        "lines": 85.0,
                        "functions": 75.0,
                        "measured": {"lines": 120, "functions": 25},
                    }
                },
            )
            tree.write_json(".lint-debt-baseline.json", {"complexity": 4, "max-depth": 1})
            tree.write_json(".jscpd-baseline.json", {"clones": 6})

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_CLEAN, result.stderr)
        finally:
            tree.__exit__(None, None, None)

    def test_lowered_floor_exits_one(self) -> None:
        tree = _committed_tree()
        try:
            lowered = json.loads(json.dumps(FLOORS))
            lowered["packages/app"]["functions"] = 40.0
            tree.write_json(".coverage-floors.json", lowered)

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("packages/app.functions fell from 70 to 40", result.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_lowered_denominator_exits_one(self) -> None:
        # A denominator is the only guard against a percentage rising because
        # covered code left the measured set, so lowering it needs review.
        tree = _committed_tree()
        try:
            lowered = json.loads(json.dumps(FLOORS))
            lowered["packages/app"]["measured"]["lines"] = 10
            tree.write_json(".coverage-floors.json", lowered)

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("packages/app.measured.lines fell from 100 to 10", result.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_acknowledged_denominator_decrease_exits_zero(self) -> None:
        # Deletion large enough to breach the committed size is legitimate; it
        # just has to name the new value.
        tree = _committed_tree()
        try:
            lowered = json.loads(json.dumps(FLOORS))
            lowered["packages/app"]["measured"]["lines"] = 90
            tree.write_json(".coverage-floors.json", lowered)
            tree.write_json(
                ".baseline-loosening.json",
                {
                    ".coverage-floors.json": {
                        "packages/app.measured.lines": {
                            "value": 90,
                            "reason": "deleted a retired module",
                        }
                    }
                },
            )

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_CLEAN, result.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_raised_debt_and_clone_ceilings_exit_one(self) -> None:
        tree = _committed_tree()
        try:
            tree.write_json(".lint-debt-baseline.json", {"complexity": 9, "max-depth": 2})
            tree.write_json(".jscpd-baseline.json", {"clones": 8})

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("complexity rose from 5 to 9", result.stdout)
            self.assertIn("clones rose from 7 to 8", result.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_retiring_a_ceiling_key_is_a_tightening(self) -> None:
        # An absent ceiling key allows nothing, so dropping one is a tightening:
        # it is how these counts ratchet down.
        tree = _committed_tree()
        try:
            tree.write_json(".lint-debt-baseline.json", {"complexity": 5})

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_CLEAN, result.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_retiring_a_ceiling_key_must_not_orphan_its_acknowledgement(self) -> None:
        # An orphan keeps naming the value a re-add compares against zero, so it
        # would license the re-add without appearing in that change's diff.
        tree = _committed_tree()
        try:
            tree.write_json(".lint-debt-baseline.json", {"complexity": 5})
            tree.write_json(
                ".baseline-loosening.json",
                {".lint-debt-baseline.json": {"max-depth": {"value": 2, "reason": "reviewed"}}},
            )

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("max-depth was retired but keeps its", result.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_removing_a_floor_exits_one(self) -> None:
        tree = _committed_tree()
        try:
            tree.write_json(".coverage-floors.json", {})

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("packages/app.lines was removed", result.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_acknowledged_floor_removal_exits_zero(self) -> None:
        # Retiring a package has to have some passing configuration.
        tree = _committed_tree()
        try:
            tree.write_json(".coverage-floors.json", {})
            tree.write_json(
                ".baseline-loosening.json",
                {
                    ".coverage-floors.json": {
                        "packages/app.lines": {"value": None, "reason": "package retired"},
                        "packages/app.functions": {"value": None, "reason": "package retired"},
                        "packages/app.measured.lines": {
                            "value": None,
                            "reason": "package retired",
                        },
                        "packages/app.measured.functions": {
                            "value": None,
                            "reason": "package retired",
                        },
                    }
                },
            )

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_CLEAN, result.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_deleting_a_limit_file_exits_one(self) -> None:
        tree = _committed_tree()
        try:
            tree.path(".jscpd-baseline.json").unlink()

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("committed limits were deleted", result.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_acknowledged_loosening_exits_zero(self) -> None:
        tree = _committed_tree()
        try:
            lowered = json.loads(json.dumps(FLOORS))
            lowered["packages/app"]["functions"] = 40.0
            tree.write_json(".coverage-floors.json", lowered)
            tree.write_json(
                ".baseline-loosening.json",
                {
                    ".coverage-floors.json": {
                        "packages/app.functions": {
                            "value": 40.0,
                            "reason": "denominator corrected, floor re-measured",
                        }
                    }
                },
            )

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_CLEAN, result.stderr)
        finally:
            tree.__exit__(None, None, None)

    def test_blank_acknowledgement_reason_does_not_allow_loosening(self) -> None:
        tree = _committed_tree()
        try:
            lowered = json.loads(json.dumps(FLOORS))
            lowered["packages/app"]["functions"] = 40.0
            tree.write_json(".coverage-floors.json", lowered)
            tree.write_json(
                ".baseline-loosening.json",
                {".coverage-floors.json": {"packages/app.functions": {"value": 40.0, "reason": "   "}}},
            )

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("packages/app.functions fell", result.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_new_ceiling_key_is_a_loosening_against_an_implicit_zero(self) -> None:
        tree = _committed_tree()
        try:
            tree.write_json(
                ".lint-debt-baseline.json", {"complexity": 5, "max-depth": 2, "max-params": 12}
            )

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("max-params rose from 0 to 12", result.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_new_floor_key_is_not_a_loosening(self) -> None:
        tree = _committed_tree()
        try:
            floors = json.loads(json.dumps(FLOORS))
            floors["packages/other"] = {
                "lines": 10.0,
                "functions": 5.0,
                "measured": {"lines": 4, "functions": 1},
            }
            tree.write_json(".coverage-floors.json", floors)

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_CLEAN, result.stderr)
        finally:
            tree.__exit__(None, None, None)

    def test_new_package_whose_every_limit_is_zero_exits_one(self) -> None:
        tree = _committed_tree()
        try:
            floors = json.loads(json.dumps(FLOORS))
            floors["packages/hollow"] = {
                "lines": 0.0,
                "functions": 0.0,
                "measured": {"lines": 0, "functions": 0},
            }
            tree.write_json(".coverage-floors.json", floors)

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("packages/hollow is new and every limit is zero", result.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_new_package_with_only_denominators_exits_one(self) -> None:
        # A populated denominator must not make a zero floor look enforceable.
        tree = _committed_tree()
        try:
            floors = json.loads(json.dumps(FLOORS))
            floors["packages/hollow"] = {
                "lines": 0.0,
                "functions": 0.0,
                "measured": {"lines": 44500, "functions": 3500},
            }
            tree.write_json(".coverage-floors.json", floors)

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("packages/hollow is new and every limit is zero", result.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_acknowledgement_does_not_license_a_later_movement(self) -> None:
        tree = _committed_tree()
        try:
            tree.write_json(".lint-debt-baseline.json", {"complexity": 6, "max-depth": 2})
            tree.write_json(
                ".baseline-loosening.json",
                {
                    ".lint-debt-baseline.json": {
                        "complexity": {"value": 6, "reason": "one reviewed spike"}
                    }
                },
            )
            allowed = _run(tree)
            self.assertEqual(allowed.returncode, sa_lib.EXIT_CLEAN, allowed.stderr)

            # The same entry must not cover a further rise.
            tree.write_json(".lint-debt-baseline.json", {"complexity": 99, "max-depth": 2})
            later = _run(tree)

            self.assertEqual(later.returncode, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("does not name this value", later.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_stale_acknowledgement_must_be_removed(self) -> None:
        tree = _committed_tree()
        try:
            tree.write_json(
                ".baseline-loosening.json",
                {
                    ".lint-debt-baseline.json": {
                        "complexity": {"value": 4, "reason": "reviewed once, long ago"}
                    }
                },
            )

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("stale", result.stdout)
            self.assertIn("the tree commits 5", result.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_merged_acknowledgement_does_not_fail_a_later_unrelated_change(self) -> None:
        # Once a loosening merges the limit stops moving against base; the entry
        # that allowed it must not red every pull request that follows.
        tree = _committed_tree()
        try:
            lowered = json.loads(json.dumps(FLOORS))
            lowered["packages/app"]["functions"] = 40.0
            tree.write_json(".coverage-floors.json", lowered)
            tree.write_json(
                ".baseline-loosening.json",
                {
                    ".coverage-floors.json": {
                        "packages/app.functions": {"value": 40.0, "reason": "reviewed drop"}
                    }
                },
            )
            _git(tree, "add", "-A")
            _git(
                tree,
                "-c",
                "user.email=gate@example.invalid",
                "-c",
                "user.name=gate",
                "commit",
                "-qm",
                "merged loosening",
            )
            tree.path("unrelated.txt").write_text("change\n", encoding="utf-8")

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_CLEAN, result.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_acknowledgement_without_a_value_is_rejected(self) -> None:
        tree = _committed_tree()
        try:
            tree.write_json(".jscpd-baseline.json", {"clones": 9})
            tree.write_json(
                ".baseline-loosening.json",
                {".jscpd-baseline.json": {"clones": {"reason": "no value named"}}},
            )

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("does not name this value", result.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_unresolvable_base_ref_is_an_operational_error(self) -> None:
        tree = _committed_tree()
        try:
            result = run_script(
                SCRIPT, "--base-ref", "no-such-ref", "--repository", ".", cwd=tree.root
            )

            self.assertEqual(result.returncode, sa_lib.EXIT_ERROR)
            self.assertIn("cannot resolve base ref", result.stderr)
        finally:
            tree.__exit__(None, None, None)

    def test_limits_are_read_from_the_repository_not_the_working_directory(self) -> None:
        tree = _committed_tree()
        try:
            tree.write_json(".lint-debt-baseline.json", {"complexity": 99, "max-depth": 2})

            result = run_script(
                SCRIPT,
                "--base-ref",
                "HEAD",
                "--repository",
                str(tree.root),
                cwd=TESTS_DIR,
            )

            self.assertEqual(result.returncode, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("complexity rose from 5 to 99", result.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_limit_file_absent_from_base_ref_is_not_a_violation(self) -> None:
        tree = _committed_tree()
        try:
            tree.write_json(".new-baseline.json", {"clones": 1})

            result = run_script(
                SCRIPT,
                "--base-ref",
                "HEAD",
                "--repository",
                ".",
                "--duplication-baseline",
                ".new-baseline.json",
                cwd=tree.root,
            )

            self.assertEqual(result.returncode, sa_lib.EXIT_CLEAN, result.stderr)
        finally:
            tree.__exit__(None, None, None)

    def test_acknowledgement_for_a_key_in_neither_revision_exits_one(self) -> None:
        # Such an entry is inert in the change that adds it and authoritative in a
        # later one, letting a ceiling rise from zero with no reviewed acknowledgement.
        tree = _committed_tree()
        try:
            tree.write_json(
                ".baseline-loosening.json",
                {
                    ".lint-debt-baseline.json": {
                        "never-existed": {"value": 12, "reason": "pre-planted"}
                    }
                },
            )

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("never-existed", result.stdout)
            self.assertIn("neither the base nor the current limits", result.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_numeric_floor_acknowledgement_for_an_absent_key_exits_one(self) -> None:
        # Only a null-valued entry — what a completed retirement leaves — may
        # survive on an absent floors key; a numeric one pre-authorises adoption.
        tree = _committed_tree()
        try:
            tree.write_json(
                ".baseline-loosening.json",
                {
                    ".coverage-floors.json": {
                        "packages/ghost.lines": {"value": 1.0, "reason": "planted"}
                    }
                },
            )

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("packages/ghost.lines", result.stdout)
            self.assertIn("neither the base nor the current limits", result.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_merged_floor_retirement_does_not_fail_a_later_unrelated_change(self) -> None:
        tree = _committed_tree()
        try:
            retirement_acks = {
                ".coverage-floors.json": {
                    "packages/app.lines": {"value": None, "reason": "package retired"},
                    "packages/app.functions": {"value": None, "reason": "package retired"},
                    "packages/app.measured.lines": {"value": None, "reason": "package retired"},
                    "packages/app.measured.functions": {
                        "value": None,
                        "reason": "package retired",
                    },
                }
            }
            tree.write_json(".coverage-floors.json", {})
            tree.write_json(".baseline-loosening.json", retirement_acks)
            _commit(tree, "retire package")
            tree.path("unrelated.txt").write_text("change\n", encoding="utf-8")

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_CLEAN, result.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_readopting_a_retired_floor_trips_over_its_retirement_entries(self) -> None:
        # Without this, a package retires at 70 and returns at 1.0 with an empty
        # acknowledgement diff, because current-only floors keys were never compared.
        tree = _committed_tree()
        try:
            tree.write_json(".coverage-floors.json", {})
            tree.write_json(
                ".baseline-loosening.json",
                {
                    ".coverage-floors.json": {
                        "packages/app.lines": {"value": None, "reason": "package retired"},
                        "packages/app.functions": {"value": None, "reason": "package retired"},
                        "packages/app.measured.lines": {"value": None, "reason": "package retired"},
                        "packages/app.measured.functions": {
                            "value": None,
                            "reason": "package retired",
                        },
                    }
                },
            )
            _commit(tree, "retire package")
            readopted = json.loads(json.dumps(FLOORS))
            readopted["packages/app"]["lines"] = 1.0
            tree.write_json(".coverage-floors.json", readopted)

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("packages/app.lines carries a stale", result.stdout)
            self.assertIn("the tree commits 1", result.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_readoption_acknowledged_at_the_committed_value_exits_zero(self) -> None:
        tree = _committed_tree()
        try:
            tree.write_json(".coverage-floors.json", {})
            tree.write_json(
                ".baseline-loosening.json",
                {
                    ".coverage-floors.json": {
                        "packages/app.lines": {"value": None, "reason": "package retired"},
                        "packages/app.functions": {"value": None, "reason": "package retired"},
                        "packages/app.measured.lines": {"value": None, "reason": "package retired"},
                        "packages/app.measured.functions": {
                            "value": None,
                            "reason": "package retired",
                        },
                    }
                },
            )
            _commit(tree, "retire package")
            tree.write_json(".coverage-floors.json", FLOORS)
            tree.write_json(
                ".baseline-loosening.json",
                {
                    ".coverage-floors.json": {
                        "packages/app.lines": {"value": 80.0, "reason": "re-adopted"},
                        "packages/app.functions": {"value": 70.0, "reason": "re-adopted"},
                        "packages/app.measured.lines": {"value": 100, "reason": "re-adopted"},
                        "packages/app.measured.functions": {"value": 20, "reason": "re-adopted"},
                    }
                },
            )

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_CLEAN, result.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_new_package_with_a_vanishing_floor_exits_one(self) -> None:
        # 1e-9 is truthy but renders as 0.0% and can never meaningfully fail.
        tree = _committed_tree()
        try:
            floors = json.loads(json.dumps(FLOORS))
            floors["packages/hollow"] = {
                "lines": 1e-9,
                "functions": 0.0,
                "measured": {"lines": 100, "functions": 20},
            }
            tree.write_json(".coverage-floors.json", floors)

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("packages/hollow is new and every limit is zero", result.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_removing_an_include_pattern_exits_one(self) -> None:
        # The perimeter file carries no numbers, but shrinking it lowers every
        # count the exact-count gates then require to be banked as improvement.
        tree = _committed_tree()
        try:
            tree.write_json(".jscpd.json", {"include": [], "ignore": PERIMETER["ignore"]})

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("include pattern 'packages/app/src/**' was removed", result.stdout)
            self.assertIn("narrows the measured perimeter", result.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_adding_an_ignore_pattern_exits_one(self) -> None:
        tree = _committed_tree()
        try:
            widened = json.loads(json.dumps(PERIMETER))
            widened["ignore"].append("packages/app/src/hot/**")
            tree.write_json(".jscpd.json", widened)

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_VIOLATIONS)
            self.assertIn(
                "ignore pattern 'packages/app/src/hot/**' was added", result.stdout
            )
        finally:
            tree.__exit__(None, None, None)

    def test_raising_a_duplication_threshold_exits_one(self) -> None:
        # A raised threshold makes jscpd report fewer clones without any pattern
        # changing, and banking the lower count reads as a tightening.
        tree = _committed_tree()
        try:
            loosened = json.loads(json.dumps(PERIMETER))
            loosened["minTokens"] = 5000
            tree.write_json(".jscpd.json", loosened)
            tree.write_json(".jscpd-baseline.json", {"clones": 3})

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("minTokens rose from 75 to 5000", result.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_acknowledged_threshold_rise_exits_zero(self) -> None:
        tree = _committed_tree()
        try:
            loosened = json.loads(json.dumps(PERIMETER))
            loosened["minTokens"] = 5000
            tree.write_json(".jscpd.json", loosened)
            tree.write_json(".jscpd-baseline.json", {"clones": 3})
            tree.write_json(
                ".baseline-loosening.json",
                {".jscpd.json": {"minTokens:5000": {"reason": "reviewed once"}}},
            )

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_CLEAN, result.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_lowering_a_duplication_threshold_is_a_tightening(self) -> None:
        tree = _committed_tree()
        try:
            tightened = json.loads(json.dumps(PERIMETER))
            tightened["minTokens"] = 50
            tree.write_json(".jscpd.json", tightened)

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_CLEAN, result.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_weakening_the_duplication_mode_exits_one(self) -> None:
        tree = _committed_tree()
        try:
            loosened = json.loads(json.dumps(PERIMETER))
            loosened["mode"] = "weak"
            tree.write_json(".jscpd.json", loosened)

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("mode changed from 'mild' to 'weak'", result.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_dropping_a_measured_language_exits_one(self) -> None:
        tree = _committed_tree()
        try:
            narrowed = json.loads(json.dumps(PERIMETER))
            narrowed["format"] = ["typescript"]
            tree.write_json(".jscpd.json", narrowed)

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("format 'tsx' was dropped", result.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_omitting_a_field_jscpd_defaults_the_same_way_is_not_a_change(self) -> None:
        # jscpd's own default mode is mild, so deleting a redundant key changes
        # nothing about what it counts and must not demand an exception.
        tree = _committed_tree()
        try:
            trimmed = {k: v for k, v in PERIMETER.items() if k != "mode"}
            tree.write_json(".jscpd.json", trimmed)

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_CLEAN, result.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_omitting_a_threshold_compares_against_the_default(self) -> None:
        tree = _committed_tree()
        try:
            # Committed 10 is above jscpd's default of 5, so dropping the key
            # lowers the threshold and counts more duplicates.
            trimmed = {k: v for k, v in PERIMETER.items() if k != "minLines"}
            tree.write_json(".jscpd.json", trimmed)
            self.assertEqual(_run(tree).returncode, sa_lib.EXIT_CLEAN)

            # Committed below the default, the same deletion raises it.
            below = json.loads(json.dumps(PERIMETER))
            below["minTokens"] = 30
            tree.write_json(".jscpd.json", below)
            _git(tree, "add", "-A")
            _git(
                tree,
                "-c",
                "user.email=gate@example.invalid",
                "-c",
                "user.name=gate",
                "commit",
                "-qm",
                "threshold below default",
            )
            tree.write_json(
                ".jscpd.json", {k: v for k, v in below.items() if k != "minTokens"}
            )

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("minTokens rose from 30 to 50", result.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_removing_a_scan_root_exits_one(self) -> None:
        tree = _committed_tree()
        try:
            narrowed = json.loads(json.dumps(PERIMETER))
            narrowed["path"] = ["packages/app/src"]
            tree.write_json(".jscpd.json", narrowed)

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("scan root 'packages/other/src' was removed", result.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_an_unwatched_perimeter_key_needs_an_acknowledgement(self) -> None:
        # Naming fields one by one is an allowlist: maxLines, maxSize, skipLocal and
        # any future option narrow what jscpd sees just as effectively.
        tree = _committed_tree()
        try:
            narrowed = json.loads(json.dumps(PERIMETER))
            narrowed["maxLines"] = 20
            tree.write_json(".jscpd.json", narrowed)
            tree.write_json(".jscpd-baseline.json", {"clones": 3})

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("maxLines changed from absent to 20", result.stdout)

            tree.write_json(
                ".baseline-loosening.json",
                {".jscpd.json": {"maxLines:20": {"reason": "reviewed once"}}},
            )
            self.assertEqual(_run(tree).returncode, sa_lib.EXIT_CLEAN)
        finally:
            tree.__exit__(None, None, None)

    def test_a_quoted_threshold_is_compared_as_a_number(self) -> None:
        # jscpd compares these numerically in JavaScript, so a quoted number
        # narrows exactly as the number does.
        tree = _committed_tree()
        try:
            loosened = json.loads(json.dumps(PERIMETER))
            loosened["minTokens"] = "5000"
            tree.write_json(".jscpd.json", loosened)
            tree.write_json(".jscpd-baseline.json", {"clones": 3})

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("minTokens rose from 75 to 5000", result.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_a_threshold_jscpd_could_not_compare_is_an_operational_error(self) -> None:
        tree = _committed_tree()
        try:
            broken = json.loads(json.dumps(PERIMETER))
            broken["minTokens"] = [5000]
            tree.write_json(".jscpd.json", broken)

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_ERROR)
            self.assertIn("minTokens must be a number, not list", result.stderr)
        finally:
            tree.__exit__(None, None, None)

    def test_a_non_list_pattern_field_is_an_operational_error(self) -> None:
        # A string coerced to an empty set compared as zero additions, so a
        # narrowed perimeter passed unremarked.
        tree = _committed_tree()
        try:
            broken = json.loads(json.dumps(PERIMETER))
            broken["ignore"] = "**/node_modules/**"
            tree.write_json(".jscpd.json", broken)

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_ERROR)
            self.assertIn("ignore must be a list of patterns, not str", result.stderr)
        finally:
            tree.__exit__(None, None, None)

    def test_a_non_string_pattern_entry_is_an_operational_error(self) -> None:
        tree = _committed_tree()
        try:
            broken = json.loads(json.dumps(PERIMETER))
            broken["include"] = ["packages/app/src/**", {"glob": "x"}]
            tree.write_json(".jscpd.json", broken)

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_ERROR)
            self.assertIn("include entries must be strings, not dict", result.stderr)
        finally:
            tree.__exit__(None, None, None)

    def test_a_cosmetic_perimeter_key_is_inert(self) -> None:
        tree = _committed_tree()
        try:
            cosmetic = json.loads(json.dumps(PERIMETER))
            cosmetic["reporters"] = ["json"]
            tree.write_json(".jscpd.json", cosmetic)

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_CLEAN, result.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_acknowledged_perimeter_narrowing_exits_zero(self) -> None:
        tree = _committed_tree()
        try:
            narrowed = json.loads(json.dumps(PERIMETER))
            narrowed["include"] = []
            narrowed["ignore"].append("packages/app/src/hot/**")
            tree.write_json(".jscpd.json", narrowed)
            tree.write_json(
                ".baseline-loosening.json",
                {
                    ".jscpd.json": {
                        "include:packages/app/src/**": {"reason": "package deleted"},
                        "ignore:packages/app/src/hot/**": {"reason": "generated code"},
                    }
                },
            )

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_CLEAN, result.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_perimeter_acknowledgement_must_describe_the_committed_perimeter(self) -> None:
        # An entry for an uncommitted ignore pre-authorises a widening that has not
        # happened; one for a still-present include outlived the removal it covered.
        tree = _committed_tree()
        try:
            tree.write_json(
                ".baseline-loosening.json",
                {
                    ".jscpd.json": {
                        "ignore:packages/app/src/hot/**": {"reason": "planted early"},
                        "include:packages/app/src/**": {"reason": "never removed"},
                    }
                },
            )

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_VIOLATIONS)
            self.assertIn(
                "ignore:packages/app/src/hot/** carries a", result.stdout
            )
            self.assertIn(
                "include:packages/app/src/** carries a", result.stdout
            )
            self.assertIn("does not describe the committed perimeter", result.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_merged_perimeter_acknowledgement_does_not_fail_a_later_change(self) -> None:
        tree = _committed_tree()
        try:
            widened = json.loads(json.dumps(PERIMETER))
            widened["ignore"].append("packages/app/src/generated/**")
            tree.write_json(".jscpd.json", widened)
            tree.write_json(
                ".baseline-loosening.json",
                {
                    ".jscpd.json": {
                        "ignore:packages/app/src/generated/**": {"reason": "generated code"}
                    }
                },
            )
            _commit(tree, "ignore generated code")
            tree.path("unrelated.txt").write_text("change\n", encoding="utf-8")

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_CLEAN, result.stdout)
        finally:
            tree.__exit__(None, None, None)

    def test_deleting_the_perimeter_config_exits_one(self) -> None:
        tree = _committed_tree()
        try:
            tree.path(".jscpd.json").unlink()

            result = _run(tree)

            self.assertEqual(result.returncode, sa_lib.EXIT_VIOLATIONS)
            self.assertIn("perimeter config was deleted", result.stdout)
        finally:
            tree.__exit__(None, None, None)


if __name__ == "__main__":
    unittest.main()
