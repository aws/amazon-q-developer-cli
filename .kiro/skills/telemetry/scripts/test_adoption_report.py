#!/usr/bin/env python3
"""Unit tests for adoption-report.py pure functions (no AWS access required)."""

import importlib.util
import unittest
from datetime import date
from pathlib import Path

# Load the sibling module (hyphenated filename can't be imported normally).
_spec = importlib.util.spec_from_file_location(
    "adoption_report", Path(__file__).with_name("adoption-report.py")
)
adoption_report = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(adoption_report)


def make_response(rows, status="Complete"):
    """Build a Logs Insights response from a list of {field: value} dicts."""
    return {
        "status": status,
        "results": [
            [{"field": k, "value": v} for k, v in row.items()] for row in rows
        ],
    }


class ResolveWindowTests(unittest.TestCase):
    TODAY = date(2026, 8, 11)

    def test_day_count_excludes_current_day(self):
        start, end = adoption_report.resolve_window("7", today=self.TODAY)
        self.assertEqual(end, date(2026, 8, 10))
        self.assertEqual(start, date(2026, 8, 4))

    def test_single_day_count(self):
        start, end = adoption_report.resolve_window("1", today=self.TODAY)
        self.assertEqual(start, date(2026, 8, 10))
        self.assertEqual(end, date(2026, 8, 10))

    def test_date_range(self):
        start, end = adoption_report.resolve_window("2026-08-04", "2026-08-06", today=self.TODAY)
        self.assertEqual(start, date(2026, 8, 4))
        self.assertEqual(end, date(2026, 8, 6))

    def test_end_date_clamped_to_yesterday(self):
        start, end = adoption_report.resolve_window("2026-08-04", "2026-08-11", today=self.TODAY)
        self.assertEqual(end, date(2026, 8, 10))

    def test_zero_days_rejected(self):
        with self.assertRaises(ValueError):
            adoption_report.resolve_window("0", today=self.TODAY)

    def test_end_before_start_rejected(self):
        with self.assertRaises(ValueError):
            adoption_report.resolve_window("2026-08-10", "2026-08-04", today=self.TODAY)

    def test_bad_date_rejected(self):
        with self.assertRaises(ValueError):
            adoption_report.resolve_window("notadate", today=self.TODAY)

    def test_day_count_with_end_rejected(self):
        with self.assertRaises(ValueError):
            adoption_report.resolve_window("7", "2026-08-10", today=self.TODAY)


class ChooseProfileTests(unittest.TestCase):
    def test_explicit_wins(self):
        got = adoption_report.choose_profile("explicit", {"KUTS_AWS_PROFILE": "env"}, {"kuts"})
        self.assertEqual(got, "explicit")

    def test_env_var_over_default(self):
        got = adoption_report.choose_profile(None, {"KUTS_AWS_PROFILE": "env"}, {"kuts"})
        self.assertEqual(got, "env")

    def test_default_chain(self):
        got = adoption_report.choose_profile(None, {}, {"kuts_telemetry_prod_read-only", "kuts"})
        self.assertEqual(got, "kuts_telemetry_prod_read-only")

    def test_fallback_to_kuts(self):
        got = adoption_report.choose_profile(None, {}, {"kuts"})
        self.assertEqual(got, "kuts")

    def test_no_profile_raises(self):
        with self.assertRaises(RuntimeError):
            adoption_report.choose_profile(None, {}, set())


class ParseQueryResultsTests(unittest.TestCase):
    END = date(2026, 8, 10)

    def test_parses_engine_interface_users(self):
        resp = make_response([
            {"day": "2026-08-10 00:00:00.000", "agent_engine": "v2",
             "session_interface": "interactive_cli", "users": "100"},
        ])
        days = adoption_report.parse_query_results(resp, self.END)
        self.assertEqual(days[date(2026, 8, 10)][("v2", "interactive_cli")]["users"], 100)

    def test_future_day_dropped(self):
        resp = make_response([
            {"day": "2026-08-11 00:00:00.000", "agent_engine": "v2",
             "session_interface": "interactive_cli", "users": "5"},
            {"day": "2026-08-10 00:00:00.000", "agent_engine": "v2",
             "session_interface": "interactive_cli", "users": "100"},
        ])
        days = adoption_report.parse_query_results(resp, self.END)
        self.assertNotIn(date(2026, 8, 11), days)
        self.assertIn(date(2026, 8, 10), days)

    def test_missing_engine_normalized_to_unknown(self):
        resp = make_response([
            {"day": "2026-08-10 00:00:00.000", "session_interface": "interactive_cli", "users": "7"},
        ])
        days = adoption_report.parse_query_results(resp, self.END)
        self.assertEqual(days[date(2026, 8, 10)][("unknown", "interactive_cli")]["users"], 7)

    def test_incomplete_status_raises(self):
        with self.assertRaises(ValueError):
            adoption_report.parse_query_results({"status": "Running", "results": []}, self.END)


class ParseTotalUsersTests(unittest.TestCase):
    END = date(2026, 8, 10)

    def test_parses_total(self):
        resp = make_response([{"day": "2026-08-10 00:00:00.000", "users": "500"}])
        days = adoption_report.parse_total_users(resp, self.END)
        self.assertEqual(days[date(2026, 8, 10)], 500)

    def test_empty_results(self):
        days = adoption_report.parse_total_users({"status": "Complete", "results": []}, self.END)
        self.assertEqual(days, {})


class CombineStatisticsTests(unittest.TestCase):
    def test_sums_bytes_and_records(self):
        r1 = {"statistics": {"bytesScanned": 1000, "recordsScanned": 10}}
        r2 = {"statistics": {"bytesScanned": 2000, "recordsScanned": 20}}
        combined = adoption_report.combine_statistics(r1, r2)
        self.assertEqual(combined["bytesScanned"], 3000)
        self.assertEqual(combined["recordsScanned"], 30)

    def test_missing_statistics_treated_as_zero(self):
        combined = adoption_report.combine_statistics({}, {"statistics": {"bytesScanned": 5}})
        self.assertEqual(combined["bytesScanned"], 5)


class PctTests(unittest.TestCase):
    def test_normal(self):
        self.assertEqual(adoption_report.pct(50, 200), "50 (25.0%)")

    def test_zero_total_is_dash(self):
        self.assertEqual(adoption_report.pct(0, 0), "—")


class RenderReportTests(unittest.TestCase):
    START = date(2026, 8, 10)
    END = date(2026, 8, 10)

    def _render(self, days, total_users, total_engine, internal_engine):
        return adoption_report.render_report(
            self.START, self.END, days, total_users, total_engine, internal_engine,
            statistics={"bytesScanned": 0, "recordsScanned": 0},
            generated_at=None,
        )

    def test_internal_external_uses_per_engine_total_not_interface_sum(self):
        # 100 users each used interactive AND noninteractive V1 the same day.
        # Per-interface sum would be 200; per-engine dedup total is 100, all internal.
        days = {self.END: {
            ("v1", "interactive_cli"): {"users": 100},
            ("v1", "noninteractive_cli"): {"users": 100},
        }}
        total_users = {self.END: 100}
        total_engine = {self.END: {"v1": 100}}   # deduplicated per-engine
        internal_engine = {self.END: {"v1": 100}}
        report = self._render(days, total_users, total_engine, internal_engine)
        # V1 Int = 100, V1 Ext = 0 (not 100 as the old summing bug produced)
        self.assertRegex(report, r"\| 2026-08-10 \| 100 \| 0 \|")

    def test_other_column_captures_unknown_and_v1_acp(self):
        days = {self.END: {
            ("v1", "interactive_cli"): {"users": 100},
            ("unknown", "interactive_cli"): {"users": 25},
            ("v1", "external_acp"): {"users": 7},
        }}
        total_users = {self.END: 132}
        total_engine = {self.END: {"v1": 107, "unknown": 25}}
        internal_engine = {self.END: {}}
        report = self._render(days, total_users, total_engine, internal_engine)
        # Other should be 25 + 7 = 32 → "32 (24.2%)"
        self.assertIn("32 (24.2%)", report)

    def test_empty_day_renders_dashes_not_zero_percent(self):
        days = {self.END: {}}
        total_users = {self.END: 0}
        report = self._render(days, total_users, {self.END: {}}, {self.END: {}})
        # Total 0 → all interface cells are em-dash, not "0 (0.0%)"
        self.assertIn("| 2026-08-10 | 0 | — | — |", report)

    def test_title_has_date_range(self):
        report = self._render({}, {}, {}, {})
        self.assertIn("# Kiro CLI Adoption Report (2026-08-10 to 2026-08-10)", report)


if __name__ == "__main__":
    unittest.main()
