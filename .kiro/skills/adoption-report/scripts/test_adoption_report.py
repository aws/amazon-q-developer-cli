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


def result_row(day, *, heartbeats=0, turns=0, **dimensions):
    row = [{"field": "day", "value": f"{day} 00:00:00.000"}]
    if heartbeats:
        row.append({"field": "heartbeats", "value": str(heartbeats)})
    if turns:
        row.append({"field": "turns", "value": str(turns)})
    row.extend(
        {"field": field, "value": value}
        for field, value in dimensions.items()
    )
    return row


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

    def test_query_uses_reviewed_metrics_and_dimensions(self):
        self.assertIn("ispresent(kiro_cli_daily_heartbeat)", REPORT.QUERY)
        self.assertIn("ispresent(kiro_cli_user_turns)", REPORT.QUERY)
        self.assertIn('`kuts.forwarded` = "true"', REPORT.QUERY)
        self.assertIn("version_full", REPORT.QUERY)
        self.assertIn("release_channel", REPORT.QUERY)
        self.assertIn("os_type", REPORT.QUERY)
        self.assertIn("install_method", REPORT.QUERY)
        self.assertIn("session_interface", REPORT.QUERY)
        self.assertIn("agent_engine", REPORT.QUERY)
        self.assertNotIn("agent_mode", REPORT.QUERY)
        self.assertNotIn("user_id", REPORT.QUERY)
        self.assertNotIn("is_subagent", REPORT.QUERY)
        self.assertNotIn("count_distinct", REPORT.QUERY)

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
                result_row(
                    "2026-07-21",
                    heartbeats=60,
                    version_full="2.5.1",
                    release_channel="stable",
                    os_type="macos",
                    install_method="brew",
                ),
                result_row(
                    "2026-07-21",
                    heartbeats=40,
                    version_full="2.5.2-nightly.14",
                    release_channel="nightly",
                    os_type="linux",
                    install_method="unknown",
                ),
                result_row(
                    "2026-07-21",
                    turns=20,
                    version_full="2.5.1",
                    agent_engine="v1",
                    session_interface="interactive_cli",
                    agent_mode="interactive",
                ),
                result_row(
                    "2026-07-21",
                    turns=40,
                    version_full="2.5.1",
                    agent_engine="v2",
                    session_interface="interactive_cli",
                    agent_mode="plan",
                ),
                result_row(
                    "2026-07-21",
                    turns=40,
                    version_full="2.5.2-nightly.14",
                    agent_engine="v3",
                    session_interface="external_acp",
                    agent_mode="custom",
                ),
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

        self.assertIn("| 2026-07-21 | `2.5.1` | stable | 60 | 60.0% |", rendered)
        self.assertIn(
            "| 2026-07-21 | `2.5.2-nightly.14` | nightly | linux | "
            "unknown | 40 |",
            rendered,
        )
        self.assertIn(
            "| 2026-07-21 | 20.0% | 40.0% | 40.0% | 0.0% | 80.0% | 100 |",
            rendered,
        )
        self.assertIn(
            "| 2026-07-21 | `2.5.2-nightly.14` | V3 | external_acp | 40 |",
            rendered,
        )
        self.assertIn("Query scan: 1.0 GiB, 1,000 records", rendered)
        self.assertIn("not user or installation adoption", rendered)

    def test_missing_dimensions_are_visible_as_unknown(self):
        response = {
            "status": "Complete",
            "results": [
                result_row("2026-07-21", heartbeats=3, install_method="unknown"),
                result_row("2026-07-21", turns=4),
            ],
        }
        rendered = REPORT.render_report(
            date(2026, 7, 21),
            date(2026, 7, 21),
            REPORT.parse_query_results(response),
            generated_at=datetime(2026, 7, 22, tzinfo=timezone.utc),
        )

        self.assertIn("| 2026-07-21 | `unknown` | unknown | 3 | 100.0% |", rendered)
        self.assertIn(
            "| 2026-07-21 | 0.0% | 0.0% | 0.0% | 100.0% | 0.0% | 4 |",
            rendered,
        )
        self.assertIn("rollout-era unknown cohort", rendered)

    def test_empty_day_is_not_reported_as_zero_percent(self):
        rendered = REPORT.render_report(
            date(2026, 7, 21),
            date(2026, 7, 21),
            {"heartbeats": {}, "turns": {}},
            generated_at=datetime(2026, 7, 22, tzinfo=timezone.utc),
        )
        self.assertIn("| 2026-07-21 | n/a | n/a | 0 | n/a |", rendered)
        self.assertIn(
            "| 2026-07-21 | n/a | n/a | n/a | n/a | n/a | 0 |",
            rendered,
        )


if __name__ == "__main__":
    unittest.main()
