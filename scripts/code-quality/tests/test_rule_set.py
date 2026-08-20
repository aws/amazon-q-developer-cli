"""Proves the Rule_Set thresholds actually red, rather than being decorative."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import unittest
from pathlib import Path

TESTS_DIR = Path(__file__).resolve().parent
REPOSITORY_ROOT = TESTS_DIR.parents[2]
CODE_QUALITY_DIR = TESTS_DIR.parent
sys.path.insert(0, str(CODE_QUALITY_DIR))

from helpers import FixtureTree  # noqa: E402

RULE_SET_CONFIG = """import tseslint from 'typescript-eslint';
import sonarjs from 'eslint-plugin-sonarjs';

export default tseslint.config({
  files: ['**/*.{ts,tsx}'],
  languageOptions: { parser: tseslint.parser, ecmaVersion: 'latest', sourceType: 'module' },
  plugins: { sonarjs },
  rules: {
    complexity: ['error', 30],
    'max-depth': ['error', 5],
    'max-params': ['error', 6],
    'sonarjs/cognitive-complexity': ['error', 30],
  },
});
"""

COMPLIANT = """export function tidy(value: number): number {
  return value > 0 ? value : -value;
}
"""


def _over_threshold_source(branches: int = 40) -> str:
    """A function whose cyclomatic complexity exceeds 30 by construction."""

    conditions = "\n".join(
        f"  if (value === {index}) total += {index};" for index in range(branches)
    )
    return f"export function sprawling(value: number): number {{\n  let total = 0;\n{conditions}\n  return total;\n}}\n"


# The Rule_Set plugins are hoisted into the packages, not the repository root.
PLUGIN_MODULES = REPOSITORY_ROOT / "packages" / "twinki" / "node_modules"


def _eslint_available() -> bool:
    if shutil.which("bunx") is None:
        return False
    return (PLUGIN_MODULES / "typescript-eslint").exists() and (
        PLUGIN_MODULES / "eslint-plugin-sonarjs"
    ).exists()


@unittest.skipUnless(
    _eslint_available(),
    "requires bunx and an installed typescript-eslint",
)
class RuleSetEnforcementTests(unittest.TestCase):
    """Proves the Rule_Set thresholds reject a violating fixture, not just parse."""

    def _run_eslint(self, tree: FixtureTree) -> subprocess.CompletedProcess[str]:
        # Config imports resolve from the config file upwards, so the fixture needs
        # its own node_modules rather than borrowing a package's working directory.
        modules = tree.root / "node_modules"
        if not modules.exists():
            os.symlink(PLUGIN_MODULES, modules)
        return subprocess.run(
            ("bunx", "eslint", ".", "--format", "json"),
            cwd=tree.root,
            capture_output=True,
            text=True,
            check=False,
        )

    def test_a_function_above_the_complexity_threshold_fails_eslint(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            tree.write_text("eslint.config.js", RULE_SET_CONFIG)
            tree.write_text("sprawling.ts", _over_threshold_source())

            result = self._run_eslint(tree)

            self.assertNotEqual(result.returncode, 0, result.stdout)
            rules = {
                message["ruleId"]
                for report in json.loads(result.stdout)
                for message in report["messages"]
            }
            self.assertIn("complexity", rules)

    def test_a_compliant_function_passes_eslint(self) -> None:
        with FixtureTree(parent=TESTS_DIR) as tree:
            tree.write_text("eslint.config.js", RULE_SET_CONFIG)
            tree.write_text("tidy.ts", COMPLIANT)

            result = self._run_eslint(tree)

            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
