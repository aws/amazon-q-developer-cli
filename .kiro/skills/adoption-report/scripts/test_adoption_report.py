#!/usr/bin/env python3

import importlib.util
import sys
import unittest
from datetime import date, datetime, timezone
from pathlib import Path
from unittest.mock import patch

SCRIPT = Path(__file__).with_name("adoption-report.py")
SPEC = importlib.util.spec_from_file_location("adoption_report", SCRIPT)
REPORT = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = REPORT
SPEC.loader.exec_module(REPORT)


def result_row(day, engine, users, turns):
    return [
        {"field": "day", "value": f"{day} 00:00:00.000"},
        {"field": "engine", "value": engine},
        {"field": "users", "value": str(users)},
        {"field": "turns", "value": str(turns)},
    ]


class AdoptionReportTest(unittest.TestCase):
    def test_resolve_window_uses_complete_utc_days(self):
        self.assertEqual(
            REPORT.resolve_window("3", today=date(2026, 7, 22)),
            (date(2026, 7, 19), date(2026, 7, 21)),
        )
        self.assertEqual(
            REPORT.resolve_window("2026-07-20", "2026-07-21"),
            (date(2026, 7, 20), date(2026, 7, 21)),
        )

    def test_query_filters_to_forwarded_top_level_turns(self):
        self.assertIn("ispresent(kiro_cli_user_turns)", REPORT.QUERY)
        self.assertIn('`kuts.forwarded` = "true"', REPORT.QUERY)
        self.assertIn('is_subagent = "false"', REPORT.QUERY)
        self.assertIn('engine in ["v1", "v2", "v3"]', REPORT.QUERY)
        self.assertIn("count_distinct(user_id)", REPORT.QUERY)

    def test_profile_precedence(self):
        available = {"kuts", "kuts_telemetry_prod_read-only"}
        self.assertEqual(REPORT.choose_profile("explicit", {}, available), "explicit")
        self.assertEqual(
            REPORT.choose_profile(None, {"KUTS_AWS_PROFILE": "env"}, available),
            "env",
        )
        self.assertEqual(
            REPORT.choose_profile(None, {}, available),
            "kuts_telemetry_prod_read-only",
        )

    def test_account_verification_rejects_another_account(self):
        with patch.object(REPORT, "run_aws", return_value={"Account": "111122223333"}):
            with self.assertRaisesRegex(RuntimeError, REPORT.ACCOUNT):
                REPORT.verify_account("wrong-account", REPORT.DEFAULT_REGION)

    def test_parse_and_render_report(self):
        response = {
            "status": "Complete",
            "results": [
                result_row("2026-07-21", "v1", 10, 20),
                result_row("2026-07-21", "v2", 20, 40),
                result_row("2026-07-21", "v3", 10, 30),
            ],
            "statistics": {"bytesScanned": 1073741824, "recordsScanned": 1000},
        }
        parsed = REPORT.parse_query_results(response)
        rendered = REPORT.render_report(
            date(2026, 7, 21),
            date(2026, 7, 21),
            parsed,
            response["statistics"],
            datetime(2026, 7, 22, 12, 0, tzinfo=timezone.utc),
        )

        self.assertIn("| 2026-07-21 | 25.0% | 50.0% | 25.0% | 75.0% |", rendered)
        self.assertIn("| 2026-07-21 | V3 | 10 | 30 |", rendered)
        self.assertIn("Query scan: 1.0 GiB, 1,000 records", rendered)
        self.assertIn("adoption proxy", rendered)

    def test_empty_day_is_not_reported_as_zero_percent(self):
        rendered = REPORT.render_report(
            date(2026, 7, 21),
            date(2026, 7, 21),
            {},
            generated_at=datetime(2026, 7, 22, tzinfo=timezone.utc),
        )
        self.assertIn("| 2026-07-21 | n/a | n/a | n/a | n/a |", rendered)
        self.assertIn("unexpected zeros as possible telemetry gaps", rendered)


if __name__ == "__main__":
    unittest.main()
