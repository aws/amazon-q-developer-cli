"""Tests for the shared PR quality report."""

from __future__ import annotations

import importlib.util
import io
import json
import sys
import tempfile
import unittest
from contextlib import redirect_stderr
from pathlib import Path

TESTS_DIR = Path(__file__).resolve().parent
SCRIPT = TESTS_DIR.parent / "pr-report.py"
SPEC = importlib.util.spec_from_file_location("pr_report", SCRIPT)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot load {SCRIPT}")
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)

CoverageMetric = MODULE.CoverageMetric
CapabilityCoverage = MODULE.CapabilityCoverage
CapabilityGap = MODULE.CapabilityGap
MAX_RENDERED_CAPABILITY_GAPS = MODULE.MAX_RENDERED_CAPABILITY_GAPS
MAX_REPORT_BYTES = MODULE.MAX_REPORT_BYTES
REPORT_MARKER = MODULE.REPORT_MARKER
REPORT_HEADING = MODULE.REPORT_HEADING
merge_report_section = MODULE.merge_report_section
render_acp_integration_section = MODULE.render_acp_integration_section
render_visual_stories_section = MODULE.render_visual_stories_section
load_capability_coverage = MODULE.load_capability_coverage
load_visual_metrics = MODULE.load_visual_metrics
main = MODULE.main


class PrReportTests(unittest.TestCase):
    def test_migrates_legacy_comment_and_preserves_it(self) -> None:
        current = f"{REPORT_MARKER}\n## Code Quality\n\nExisting metrics\n"

        merged = merge_report_section(
            current, "visual-stories", "## Visual Stories\n\nNew metrics"
        )

        self.assertEqual(merged.count(REPORT_MARKER), 1)
        self.assertIn(
            "<!-- quality-metrics-section:code-quality:start -->\n"
            "## Code Quality\n\nExisting metrics\n"
            "<!-- quality-metrics-section:code-quality:end -->",
            merged,
        )
        self.assertIn(
            "<!-- quality-metrics-section:visual-stories:start -->\n"
            "## Visual Stories\n\nNew metrics\n"
            "<!-- quality-metrics-section:visual-stories:end -->",
            merged,
        )

    def test_renders_one_heading_and_keeps_section_content_visible(self) -> None:
        merged = merge_report_section("", "visual-stories", "visual")
        merged = merge_report_section(merged, "code-quality", "quality")
        merged = merge_report_section(merged, "acp-integration", "acp")

        self.assertEqual(merged.count(REPORT_HEADING), 1)
        self.assertNotIn("<details>", merged)
        self.assertIn("quality", merged)
        self.assertIn("acp", merged)
        self.assertIn("visual", merged)

    def test_replaces_only_the_owned_section(self) -> None:
        current = merge_report_section("", "code-quality", "old quality")
        current = merge_report_section(current, "visual-stories", "visual")

        merged = merge_report_section(current, "code-quality", "new quality")

        self.assertNotIn("old quality", merged)
        self.assertIn("new quality", merged)
        self.assertIn("visual", merged)

    def test_inserts_code_quality_before_an_existing_visual_section(self) -> None:
        current = merge_report_section("", "visual-stories", "visual")

        merged = merge_report_section(current, "code-quality", "quality")

        self.assertLess(
            merged.index("quality-metrics-section:code-quality:start"),
            merged.index("quality-metrics-section:visual-stories:start"),
        )

    def test_orders_acp_between_code_quality_and_visual_stories(self) -> None:
        current = merge_report_section("", "visual-stories", "visual")
        current = merge_report_section(current, "code-quality", "quality")

        merged = merge_report_section(current, "acp-integration", "acp")

        self.assertLess(
            merged.index("quality-metrics-section:code-quality:start"),
            merged.index("quality-metrics-section:acp-integration:start"),
        )
        self.assertLess(
            merged.index("quality-metrics-section:acp-integration:start"),
            merged.index("quality-metrics-section:visual-stories:start"),
        )

    def test_rejects_malformed_or_injected_section_markers(self) -> None:
        with self.assertRaisesRegex(ValueError, "unclosed"):
            merge_report_section(
                "<!-- quality-metrics-section:code-quality:start -->",
                "visual-stories",
                "visual",
            )
        with self.assertRaisesRegex(ValueError, "control markers"):
            merge_report_section(
                "",
                "visual-stories",
                "<!-- quality-metrics-section:code-quality:start -->",
            )

    def test_rejects_a_merged_comment_above_the_github_safety_limit(self) -> None:
        with self.assertRaisesRegex(
            ValueError, f"limit is {MAX_REPORT_BYTES}"
        ):
            merge_report_section(
                "", "code-quality", "x" * MAX_REPORT_BYTES
            )

    def test_renders_exact_base_deltas_and_advisory_policy(self) -> None:
        head = (
            CoverageMetric("Story coverage", 10, 10),
            CoverageMetric("Semantic variants", 8, 10),
        )
        base = (
            CoverageMetric("Story coverage", 8, 10),
            CoverageMetric("Semantic variants", 5, 10),
        )

        report = render_visual_stories_section(
            head, base, "https://github.example/actions/runs/123"
        )

        self.assertIn(
            "| Story coverage | 80.0% (8/10) | 100.0% (10/10) "
            "| +20.0 pp | Advisory |",
            report,
        )
        self.assertIn(
            "| Semantic variants | 50.0% (5/10) | 80.0% (8/10) "
            "| +30.0 pp | Advisory |",
            report,
        )
        self.assertIn("Percentage floors are not enforced yet.", report)
        self.assertIn("<summary>Coverage policy details</summary>", report)
        self.assertLess(
            report.index("| Story coverage |"),
            report.index("<summary>Coverage policy details</summary>"),
        )
        self.assertIn("[Open RC Certification artifacts]", report)

    def test_marks_missing_base_without_claiming_a_delta(self) -> None:
        report = render_visual_stories_section(
            (CoverageMetric("Story coverage", 1, 1),), None, None, "success"
        )

        self.assertIn("| Story coverage | n/a | 100.0% (1/1) | n/a |", report)
        self.assertIn("exact PR-base capture was unavailable", report)
        self.assertIn("RC workflow result: **success**", report)

    def test_replaces_stale_metrics_with_an_unavailable_lane_status(self) -> None:
        report = render_visual_stories_section(
            None,
            None,
            "https://github.example/actions/runs/123",
            "failure",
            "head coverage artifact is invalid",
        )

        self.assertIn("Metrics unavailable or not selected", report)
        self.assertIn("RC workflow result: **failure**", report)
        self.assertIn("Evidence error: head coverage artifact is invalid", report)
        self.assertNotIn("| Metric |", report)

    def test_loads_all_visual_metrics_and_rejects_impossible_counts(self) -> None:
        payload = {
            "totalStories": 10,
            "coveredStories": 9,
            "totalVariants": 20,
            "executedVariants": 18,
            "verifiedVariants": 15,
            "totalVisualStates": 12,
            "coveredVisualStates": 11,
            "totalComponents": 30,
            "coveredComponents": 25,
        }
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "coverage.json"
            path.write_text(json.dumps(payload), encoding="utf-8")
            metrics = load_visual_metrics(path)
            self.assertEqual(
                tuple(metric.label for metric in metrics),
                (
                    "Story coverage",
                    "Variant execution",
                    "Semantic variants",
                    "Declared visual states",
                    "Component reachability",
                ),
            )

            payload["coveredComponents"] = 31
            path.write_text(json.dumps(payload), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "cannot exceed"):
                load_visual_metrics(path)

    def test_renders_acp_capability_coverage_and_uncovered_details(self) -> None:
        report = CapabilityCoverage(
            "ACP journeys",
            "Supported capabilities",
            2,
            3,
            (
                CoverageMetric("ACP session", 1, 1),
                CoverageMetric("KAS requests", 1, 2),
            ),
            (CapabilityGap("kas.help", "KAS requests", "Request help."),),
        )

        markdown = render_acp_integration_section(
            report, "https://github.example/actions/runs/123", "success"
        )

        self.assertIn("**66.7% (2/3)**", markdown)
        self.assertIn("passing Linux integration-test evidence", markdown)
        self.assertIn("| ACP session | 100.0% (1/1) | 0 |", markdown)
        self.assertIn("| KAS requests | 50.0% (1/2) | 1 |", markdown)
        self.assertIn("`kas.help` (KAS requests): Request help.", markdown)
        self.assertIn("_ACP journeys_", markdown)
        self.assertIn("Coverage denominator: Supported capabilities", markdown)
        self.assertIn("RC workflow result: **success**", markdown)
        self.assertIn("not source-code line coverage", markdown)
        self.assertIn("No base delta is claimed", markdown)
        self.assertLess(
            markdown.index("| ACP session |"),
            markdown.index("<summary>1 uncovered capabilities</summary>"),
        )
        self.assertLess(
            markdown.index("No base delta is claimed"),
            markdown.index("</details>"),
        )

    def test_truncates_large_uncovered_capability_details_explicitly(self) -> None:
        total = MAX_RENDERED_CAPABILITY_GAPS + 5
        report = CapabilityCoverage(
            "ACP journeys",
            "Supported capabilities",
            0,
            total,
            (CoverageMetric("KAS requests", 0, total),),
            tuple(
                CapabilityGap(
                    f"gap.{index:03d}", "KAS requests", "Missing evidence."
                )
                for index in range(total)
            ),
        )

        markdown = render_acp_integration_section(report, None)

        self.assertIn(f"<summary>{total} uncovered capabilities</summary>", markdown)
        self.assertIn(
            f"`gap.{MAX_RENDERED_CAPABILITY_GAPS - 1:03d}`", markdown
        )
        self.assertNotIn(
            f"`gap.{MAX_RENDERED_CAPABILITY_GAPS:03d}`", markdown
        )
        self.assertIn(
            "5 additional uncovered capabilities omitted", markdown
        )

    def test_loads_consistent_acp_coverage_and_rejects_bad_totals(self) -> None:
        payload = {
            "schemaVersion": 1,
            "suite": "acp-integration",
            "description": "ACP journeys",
            "denominator": "Supported capabilities",
            "covered": 2,
            "total": 3,
            "percent": 66.7,
            "areas": [
                {
                    "area": "ACP session",
                    "covered": 1,
                    "total": 1,
                    "percent": 100,
                },
                {
                    "area": "KAS requests",
                    "covered": 1,
                    "total": 2,
                    "percent": 50,
                },
            ],
            "uncovered": [
                {
                    "id": "kas.help",
                    "area": "KAS requests",
                    "description": "Request help.",
                    "evidence": [],
                }
            ],
        }
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "acp-integration.json"
            path.write_text(json.dumps(payload), encoding="utf-8")

            report = load_capability_coverage(path)

            self.assertEqual((report.covered, report.total), (2, 3))
            self.assertEqual(report.uncovered[0].id, "kas.help")

            payload["suite"] = "smoke"
            path.write_text(json.dumps(payload), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "unexpected"):
                load_capability_coverage(path)

            payload["suite"] = "acp-integration"
            payload["covered"] = 1
            path.write_text(json.dumps(payload), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "area covered counts"):
                load_capability_coverage(path)

    def test_bounds_and_escapes_artifact_derived_markdown(self) -> None:
        report = CapabilityCoverage(
            "ACP *journeys* @team",
            "Capabilities | evidence",
            0,
            1,
            (CoverageMetric("KAS | requests", 0, 1),),
            (
                CapabilityGap(
                    "kas.help",
                    "KAS | requests",
                    "Render [help] <safely> @team.",
                ),
            ),
        )

        markdown = render_acp_integration_section(report, None)

        self.assertIn(r"_ACP \*journeys\* &#64;team_", markdown)
        self.assertIn(r"| KAS \| requests |", markdown)
        self.assertIn(r"Capabilities \| evidence", markdown)
        self.assertIn(r"Render \[help\] &lt;safely&gt; &#64;team.", markdown)

        payload = {
            "schemaVersion": 1,
            "suite": "acp-integration",
            "description": "x" * 301,
            "denominator": "Supported capabilities",
            "covered": 1,
            "total": 1,
            "areas": [{"area": "ACP", "covered": 1, "total": 1}],
            "uncovered": [],
        }
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "acp-integration.json"
            path.write_text(json.dumps(payload), encoding="utf-8")

            with self.assertRaisesRegex(ValueError, "exceeds 300"):
                load_capability_coverage(path)

    def test_rejects_acp_gaps_that_do_not_match_their_area(self) -> None:
        payload = {
            "schemaVersion": 1,
            "suite": "acp-integration",
            "description": "ACP journeys",
            "denominator": "Supported capabilities",
            "covered": 1,
            "total": 2,
            "areas": [
                {"area": "ACP session", "covered": 1, "total": 2},
                {"area": "KAS requests", "covered": 0, "total": 0},
            ],
            "uncovered": [
                {
                    "id": "kas.help",
                    "area": "KAS requests",
                    "description": "Request help.",
                }
            ],
        }
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "acp-integration.json"
            path.write_text(json.dumps(payload), encoding="utf-8")

            with self.assertRaisesRegex(ValueError, "area 'ACP session'"):
                load_capability_coverage(path)

    def test_acp_cli_replaces_bad_and_missing_artifacts_with_unavailable(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            report_path = root / "acp-integration.json"
            out = root / "report.md"
            report_path.write_text("{}", encoding="utf-8")

            with redirect_stderr(io.StringIO()):
                exit_code = main(
                    [
                        "acp",
                        "--report",
                        str(report_path),
                        "--run-result",
                        "failure",
                        "--out",
                        str(out),
                    ]
                )

            self.assertEqual(exit_code, 0)
            report = out.read_text(encoding="utf-8")
            self.assertIn("Metrics unavailable", report)
            self.assertIn("artifact is invalid", report)
            self.assertIn("RC workflow result: **failure**", report)

            report_path.unlink()
            exit_code = main(
                [
                    "acp",
                    "--report",
                    str(report_path),
                    "--run-result",
                    "success",
                    "--out",
                    str(out),
                ]
            )

            self.assertEqual(exit_code, 0)
            report = out.read_text(encoding="utf-8")
            self.assertIn("Metrics unavailable", report)
            self.assertNotIn("Evidence error:", report)
            self.assertIn("RC workflow result: **success**", report)

    def test_cli_degrades_malformed_evidence_without_leaving_stale_metrics(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            head = root / "head.json"
            base = root / "base.json"
            out = root / "report.md"
            head.write_text("{", encoding="utf-8")
            base.write_text("{}", encoding="utf-8")

            with redirect_stderr(io.StringIO()):
                exit_code = main(
                    [
                        "visual",
                        "--head",
                        str(head),
                        "--base",
                        str(base),
                        "--run-result",
                        "failure",
                        "--out",
                        str(out),
                    ]
                )

            self.assertEqual(exit_code, 0)
            report = out.read_text(encoding="utf-8")
            self.assertIn("Metrics unavailable or not selected", report)
            self.assertIn("head coverage artifact is invalid", report)

            head.write_text(
                json.dumps(
                    {
                        "totalStories": 1,
                        "coveredStories": 1,
                        "totalVariants": 1,
                        "executedVariants": 1,
                        "verifiedVariants": 1,
                        "totalVisualStates": 1,
                        "coveredVisualStates": 1,
                        "totalComponents": 1,
                        "coveredComponents": 1,
                    }
                ),
                encoding="utf-8",
            )
            with redirect_stderr(io.StringIO()):
                exit_code = main(
                    [
                        "visual",
                        "--head",
                        str(head),
                        "--base",
                        str(base),
                        "--run-result",
                        "success",
                        "--out",
                        str(out),
                    ]
                )

            self.assertEqual(exit_code, 0)
            report = out.read_text(encoding="utf-8")
            self.assertIn("| Story coverage | n/a | 100.0% (1/1) | n/a |", report)


if __name__ == "__main__":
    unittest.main()
