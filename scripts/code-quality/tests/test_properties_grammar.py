"""Property tests for the shared LINT-DEBT grammar contract."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

TESTS_DIR = Path(__file__).resolve().parent
CODE_QUALITY_DIR = TESTS_DIR.parent
sys.path.insert(0, str(CODE_QUALITY_DIR))

import sa_lib  # noqa: E402
from generators import (  # noqa: E402
    HAS_HYPOTHESIS,
    HYPOTHESIS_SKIP_REASON,
    LintDebtNearMiss,
    given,
    lint_debt_near_miss_cases,
    settings,
    valid_lint_debt_components,
)

_VALID_COMPONENTS = valid_lint_debt_components() if HAS_HYPOTHESIS else None
_NEAR_MISSES = lint_debt_near_miss_cases() if HAS_HYPOTHESIS else None


@unittest.skipUnless(HAS_HYPOTHESIS, HYPOTHESIS_SKIP_REASON)
class LintDebtGrammarPropertyTests(unittest.TestCase):
    @settings(max_examples=200)
    @given(parts=_VALID_COMPONENTS, near_miss=_NEAR_MISSES)
    def test_grammar_round_trip_and_rejection(
        self,
        parts: tuple[str, str],
        near_miss: LintDebtNearMiss,
    ) -> None:
        rule, reason = parts
        comment = sa_lib.format_lint_debt(rule, reason)

        self.assertEqual(sa_lib.parse_lint_debt(comment), (rule, reason))
        self.assertIsNone(
            sa_lib.parse_lint_debt(near_miss.comment),
            msg=f"mutation was accepted: {near_miss.mutation}",
        )


if __name__ == "__main__":
    unittest.main()
