"""Property tests for the lint-debt checker, guarded for stdlib-only environments."""

from __future__ import annotations

import importlib.util
import io
import sys
import unittest
from collections import Counter
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import ModuleType
from typing import Callable, Mapping, Sequence

TESTS_DIR = Path(__file__).resolve().parent
CODE_QUALITY_DIR = TESTS_DIR.parent
CHECKER_PATH = CODE_QUALITY_DIR / "check-lint-debt.py"
sys.path.insert(0, str(CODE_QUALITY_DIR))

import sa_lib  # noqa: E402
from helpers import FixtureTree  # noqa: E402
from generators import (  # noqa: E402
    HAS_HYPOTHESIS,
    HYPOTHESIS_SKIP_REASON,
    given,
    settings,
)

if HAS_HYPOTHESIS:
    from hypothesis import strategies as st
else:

    class _StubStrategies:
        @staticmethod
        def composite(_function: Callable) -> Callable:
            return lambda *_args, **_kwargs: None

    st = _StubStrategies()  # type: ignore[assignment]


RULES = (
    "complexity",
    "max-depth",
    "max-params",
    "sonarjs/cognitive-complexity",
    "@scope/custom-rule",
)
DIRECTIVE_FORMS = (
    "line-next",
    "line-current",
    "block-global",
    "block-next",
)
SOURCE_PATHS = (
    "src/generated.ts",
    "src/nested/module.ts",
    "src/deep/path/value.tsx",
)


def _load_checker_module() -> ModuleType:
    module_name = "check_lint_debt_property_test"
    existing = sys.modules.get(module_name)
    if existing is not None:
        return existing
    spec = importlib.util.spec_from_file_location(module_name, CHECKER_PATH)
    if spec is None or spec.loader is None:
        raise RuntimeError("could not load Debt_Checker module")
    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    spec.loader.exec_module(module)
    return module


CHECKER = _load_checker_module()


def _directive_line(form: str, rules: tuple[str, ...], site: int) -> str:
    joined = ", ".join(rules)
    if form == "line-next":
        return f"// eslint-disable-next-line {joined} -- generated description"
    if form == "line-current":
        return f"export const value{site} = {site}; // eslint-disable-line {joined} -- generated description"
    if form == "block-global":
        return f"/* eslint-disable {joined} */"
    if form == "block-next":
        return f"/* eslint-disable-next-line {joined} */"
    raise ValueError(f"unsupported directive form: {form}")


def _append_justified_site(
    lines: list[str],
    form: str,
    rules: tuple[str, ...],
    site: int,
) -> None:
    for rule in rules:
        lines.append(f"// LINT-DEBT({rule}): generated existing debt at site {site}")
    lines.append(_directive_line(form, rules, site))
    if form != "line-current":
        lines.append(f"export const value{site} = {site};")
    if form == "block-global":
        lines.append(f"/* eslint-enable {', '.join(rules)} */")


def _render_justified_sites(sites: tuple[tuple[str, tuple[str, ...]], ...]) -> str:
    lines: list[str] = []
    for site, (form, rules) in enumerate(sites):
        _append_justified_site(lines, form, rules, site)
    if not lines:
        lines.append("export const clean = true;")
    return "\n".join(lines) + "\n"


def _write_fixture(
    tree: FixtureTree,
    source_path: str,
    source: str,
    baseline: Mapping[str, int],
) -> None:
    tree.write_json(
        ".jscpd.json",
        {"include": ["src/**"], "ignore": ["**/*.test.*", "**/tests/**"]},
    )
    tree.write_json(".lint-debt-baseline.json", dict(baseline))
    tree.write_text(source_path, source)


def _rule_counts(
    *site_groups: Sequence[tuple[str, tuple[str, ...]]],
) -> dict[str, int]:
    counts: Counter[str] = Counter()
    for sites in site_groups:
        for _form, rules in sites:
            counts.update(rules)
    return dict(counts)


def _run_checker(tree: FixtureTree) -> tuple[int, str, str]:
    stdout = io.StringIO()
    stderr = io.StringIO()
    with redirect_stdout(stdout), redirect_stderr(stderr):
        exit_code = CHECKER.main(("--root", str(tree.root)))
    return exit_code, stdout.getvalue(), stderr.getvalue()


@st.composite
def justified_tree_cases(draw: Callable) -> tuple[
    tuple[tuple[str, tuple[str, ...]], ...], dict[str, int]
]:
    site_strategy = st.tuples(
        st.sampled_from(DIRECTIVE_FORMS),
        st.lists(st.sampled_from(RULES), min_size=1, max_size=3, unique=True).map(tuple),
    )
    sites = tuple(draw(st.lists(site_strategy, min_size=0, max_size=5)))
    # An allowance per known rule, spanning both sides of the measured counts so the
    # law is exercised over and under the baseline for each rule independently.
    baseline = {rule: draw(st.integers(min_value=0, max_value=4)) for rule in RULES}
    return sites, baseline


@st.composite
def unjustified_disable_cases(draw: Callable) -> dict[str, object]:
    site_strategy = st.tuples(
        st.sampled_from(DIRECTIVE_FORMS),
        st.lists(st.sampled_from(RULES), min_size=1, max_size=3, unique=True).map(tuple),
    )
    before = tuple(draw(st.lists(site_strategy, min_size=0, max_size=3)))
    after = tuple(draw(st.lists(site_strategy, min_size=0, max_size=3)))
    return {
        "before": before,
        "after": after,
        "target_form": draw(st.sampled_from(DIRECTIVE_FORMS)),
        "target_rule": draw(st.sampled_from(RULES)),
        "source_path": draw(st.sampled_from(SOURCE_PATHS)),
    }


@st.composite
def malformed_justification_cases(draw: Callable) -> dict[str, object]:
    rule = draw(st.sampled_from(RULES))
    other_rules = tuple(candidate for candidate in RULES if candidate != rule)
    mutation = draw(
        st.sampled_from(
            (
                "missing-colon",
                "empty-reason",
                "invalid-rule",
                "lowercase-marker",
                "block-comment",
                "mismatched-rule",
            )
        )
    )
    prefix_count = draw(st.integers(min_value=0, max_value=8))
    return {
        "rule": rule,
        "other_rule": draw(st.sampled_from(other_rules)),
        "mutation": mutation,
        "prefix_count": prefix_count,
        "target_form": draw(st.sampled_from(DIRECTIVE_FORMS)),
        "source_path": draw(st.sampled_from(SOURCE_PATHS)),
    }


def _mutated_comment(rule: str, other_rule: str, mutation: str) -> str:
    if mutation == "missing-colon":
        return f"// LINT-DEBT({rule}) generated reason"
    if mutation == "empty-reason":
        return f"// LINT-DEBT({rule}):   "
    if mutation == "invalid-rule":
        return f"// LINT-DEBT({rule}.invalid): generated reason"
    if mutation == "lowercase-marker":
        return f"// lint-debt({rule}): generated reason"
    if mutation == "block-comment":
        return f"/* LINT-DEBT({rule}) generated reason */"
    if mutation == "mismatched-rule":
        return f"// LINT-DEBT({other_rule}): generated reason"
    raise ValueError(f"unsupported mutation: {mutation}")


class DebtCheckerPropertyTests(unittest.TestCase):
    @unittest.skipUnless(HAS_HYPOTHESIS, HYPOTHESIS_SKIP_REASON)
    @settings(max_examples=100, deadline=None)
    @given(case=justified_tree_cases())
    def test_debt_exit_code_law(
        self,
        case: tuple[tuple[tuple[str, tuple[str, ...]], ...], dict[str, int]],
    ) -> None:
        sites, baseline = case
        measured = _rule_counts(sites)
        over = {
            rule: count
            for rule, count in measured.items()
            if count > baseline.get(rule, 0)
        }
        under = {
            rule: allowed
            for rule, allowed in baseline.items()
            if measured.get(rule, 0) < allowed
        }
        with FixtureTree(parent=TESTS_DIR) as tree:
            _write_fixture(
                tree,
                "src/generated.ts",
                _render_justified_sites(sites),
                baseline,
            )

            exit_code, stdout, stderr = _run_checker(tree)

        self.assertEqual(stderr, "")
        if not over and not under:
            self.assertEqual(exit_code, sa_lib.EXIT_CLEAN)
            self.assertNotIn("over-baseline", stdout)
            self.assertNotIn("under-baseline", stdout)
        else:
            self.assertNotEqual(exit_code, sa_lib.EXIT_CLEAN)
            for rule, count in over.items():
                self.assertIn(f"rule {rule!r} measured debt count {count}", stdout)
                self.assertIn(f"exceeds baseline {baseline.get(rule, 0)}", stdout)
            for rule, allowed in under.items():
                self.assertIn(f"is below baseline {allowed}", stdout)
            for rule, count in measured.items():
                if rule not in over and rule not in under:
                    self.assertNotIn(f"rule {rule!r} measured", stdout)

    @unittest.skipUnless(HAS_HYPOTHESIS, HYPOTHESIS_SKIP_REASON)
    @settings(max_examples=100, deadline=None)
    @given(case=unjustified_disable_cases())
    def test_unjustified_disable_detection(self, case: dict[str, object]) -> None:
        before = case["before"]
        after = case["after"]
        target_form = str(case["target_form"])
        target_rule = str(case["target_rule"])
        source_path = str(case["source_path"])
        self.assertIsInstance(before, tuple)
        self.assertIsInstance(after, tuple)

        lines: list[str] = []
        for site, (form, rules) in enumerate(before):  # type: ignore[union-attr]
            _append_justified_site(lines, form, rules, site)
        target_line = len(lines) + 1
        lines.append(_directive_line(target_form, (target_rule,), len(lines)))
        if target_form != "line-current":
            lines.append("export const planted = true;")
        if target_form == "block-global":
            lines.append(f"/* eslint-enable {target_rule} */")
        offset = len(before)  # type: ignore[arg-type]
        for site, (form, rules) in enumerate(after, start=offset + 1):  # type: ignore[union-attr]
            _append_justified_site(lines, form, rules, site)

        justified_counts = _rule_counts(before, after)  # type: ignore[arg-type]
        with FixtureTree(parent=TESTS_DIR) as tree:
            _write_fixture(tree, source_path, "\n".join(lines) + "\n", justified_counts)

            exit_code, stdout, _stderr = _run_checker(tree)

        self.assertNotEqual(exit_code, sa_lib.EXIT_CLEAN)
        self.assertIn(f"{source_path}:{target_line}: unjustified:", stdout)
        self.assertIn(target_rule, stdout)

    @unittest.skipUnless(HAS_HYPOTHESIS, HYPOTHESIS_SKIP_REASON)
    @settings(max_examples=100, deadline=None)
    @given(case=malformed_justification_cases())
    def test_malformed_justification_detection(
        self,
        case: dict[str, object],
    ) -> None:
        rule = str(case["rule"])
        other_rule = str(case["other_rule"])
        mutation = str(case["mutation"])
        prefix_count = int(case["prefix_count"])
        target_form = str(case["target_form"])
        source_path = str(case["source_path"])

        lines = [f"export const prefix{index} = {index};" for index in range(prefix_count)]
        comment_line = len(lines) + 1
        lines.append(_mutated_comment(rule, other_rule, mutation))
        lines.append(_directive_line(target_form, (rule,), prefix_count))
        if target_form != "line-current":
            lines.append("export const target = true;")
        if target_form == "block-global":
            lines.append(f"/* eslint-enable {rule} */")

        with FixtureTree(parent=TESTS_DIR) as tree:
            _write_fixture(tree, source_path, "\n".join(lines) + "\n", {rule: 1})

            exit_code, stdout, _stderr = _run_checker(tree)

        self.assertNotEqual(exit_code, sa_lib.EXIT_CLEAN)
        self.assertIn(f"{source_path}:{comment_line}: malformed:", stdout)


if __name__ == "__main__":
    unittest.main()
