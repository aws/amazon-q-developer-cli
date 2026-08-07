#!/usr/bin/env python3

import importlib.util
import json
import sys
import tempfile
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
    row.extend({"field": field, "value": value} for field, value in dimensions.items())
    return row


def toolkit_response(**segments):
    aggregations = {}
    for segment in REPORT.TOOLKIT_SEGMENTS:
        installations, turns = segments.get(segment, (0, 0))
        aggregations[segment] = {
            "doc_count": turns,
            "installations": {"value": installations},
        }
    return {"aggregations": aggregations}


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

    def test_toolkit_query_preserves_legacy_segments_in_one_search(self):
        query = REPORT.build_toolkit_query()
        encoded = json.dumps(query)

        self.assertIn(REPORT.TOOLKIT_METRIC, encoded)
        self.assertIn("CodeWhisperer for Terminal", encoded)
        self.assertIn("clientId", encoded)
        self.assertEqual(set(query["aggs"]), set(REPORT.TOOLKIT_SEGMENTS))
        self.assertIn(
            {"exists": {"field": "metadata.kirocli_appType"}},
            query["aggs"]["int_v1"]["filter"]["bool"]["must_not"],
        )
        self.assertIn(
            {"match": {"metadata.kirocli_appType": "V2"}},
            query["aggs"]["ext_v2"]["filter"]["bool"]["must"],
        )
        self.assertIn(
            {"match": {"metadata.kirocli_appType": "ACP"}},
            query["aggs"]["acp"]["filter"]["bool"]["must"],
        )

    def test_toolkit_uses_one_search_per_day(self):
        response = toolkit_response(int_v2=(3, 4))
        with patch.object(REPORT, "verify_toolkit_credentials"), patch.object(
            REPORT, "run_toolkit_search", return_value=response
        ) as search:
            results, errors = REPORT.query_toolkit(
                Path("scripts/es-query.sh"),
                date(2026, 7, 20),
                date(2026, 7, 21),
            )

        self.assertFalse(errors)
        self.assertEqual(search.call_count, 2)
        self.assertEqual(
            [call.args[1] for call in search.call_args_list],
            ["metrics-2026-07-20", "metrics-2026-07-21"],
        )
        self.assertEqual(results[date(2026, 7, 21)]["int_v2"]["turns"], 4)

    def test_toolkit_retains_successful_days_after_retry_exhaustion(self):
        success = toolkit_response(acp=(3, 4))
        failure = RuntimeError("daily index unavailable")
        with patch.object(REPORT, "verify_toolkit_credentials"), patch.object(
            REPORT,
            "run_toolkit_search",
            side_effect=[success, failure, failure, failure, success],
        ) as search, patch.object(REPORT.time, "sleep"):
            results, errors = REPORT.query_toolkit(
                Path("scripts/es-query.sh"),
                date(2026, 7, 20),
                date(2026, 7, 22),
            )

        self.assertEqual(search.call_count, 5)
        self.assertEqual(
            list(results),
            [date(2026, 7, 20), date(2026, 7, 22)],
        )
        self.assertEqual(results[date(2026, 7, 22)]["acp"]["turns"], 4)
        self.assertEqual(str(errors[date(2026, 7, 21)]), str(failure))

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

    def test_legacy_input_json_flag_still_selects_kuts_input(self):
        args = REPORT.build_parser().parse_args(
            ["2026-07-21", "--input-json", "kuts.json"]
        )

        self.assertEqual(args.legacy_input_json, Path("kuts.json"))
        self.assertEqual(REPORT.effective_source(args), "kuts")

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
            "| 2026-07-21 | `2.5.2-nightly.14` | nightly | linux | " "unknown | 40 |",
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
        self.assertIn("scanned 1.0 GiB, 1,000 records", rendered)
        self.assertIn("Turn share weights frequent users", rendered)

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
        self.assertIn("records from older clients", rendered)

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

    def test_combined_report_labels_sources_and_compares_turns(self):
        kuts_response = {
            "status": "Complete",
            "results": [
                result_row(
                    "2026-07-21",
                    turns=20,
                    version_full="2.5.1",
                    agent_engine="v1",
                    session_interface="interactive_cli",
                ),
                result_row(
                    "2026-07-21",
                    turns=40,
                    version_full="2.5.1",
                    agent_engine="v2",
                    session_interface="interactive_cli",
                ),
                result_row(
                    "2026-07-21",
                    turns=40,
                    version_full="2.5.1",
                    agent_engine="v3",
                    session_interface="external_acp",
                ),
            ],
        }
        toolkit = {
            date(2026, 7, 21): REPORT.parse_toolkit_response(
                toolkit_response(
                    int_v1=(5, 10),
                    int_v2=(15, 20),
                    ext_v1=(10, 20),
                    ext_v2=(30, 20),
                    acp=(12, 30),
                )
            )
        }

        rendered = REPORT.render_report(
            date(2026, 7, 21),
            date(2026, 7, 21),
            REPORT.parse_query_results(kuts_response),
            toolkit_results=toolkit,
            generated_at=datetime(2026, 7, 22, tzinfo=timezone.utc),
        )

        self.assertIn("**KUTS**: available", rendered)
        self.assertIn("**Toolkit**: available", rendered)
        self.assertIn("| 2026-07-21 | 75.0% | 75.0% |", rendered)
        self.assertIn(
            "| 2026-07-21 | 30 | 20 | -10 (-33.3%) | 40 | 40 | "
            "+0 (+0.0%) | 30 | 40 | +10 (+33.3%) | 100 | 100 | +0 (+0.0%) |",
            rendered,
        )
        self.assertIn(
            "Do not directly compare installation counts across sources", rendered
        )
        self.assertIn("Toolkit cannot isolate V3", rendered)

    def test_toolkit_input_supports_responses_keyed_by_date(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "toolkit.json"
            path.write_text(
                json.dumps(
                    {
                        "responses": {
                            "2026-07-20": toolkit_response(int_v1=(1, 2)),
                            "2026-07-21": toolkit_response(ext_v2=(3, 4)),
                        }
                    }
                )
            )

            results = REPORT.load_toolkit_results(
                path, date(2026, 7, 21), date(2026, 7, 21)
            )

        self.assertEqual(list(results), [date(2026, 7, 21)])
        self.assertEqual(results[date(2026, 7, 21)]["ext_v2"]["turns"], 4)

    def test_toolkit_input_rejects_empty_and_out_of_window_responses(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "toolkit.json"
            for payload in (
                {"responses": {}},
                {"responses": {"2026-07-20": toolkit_response(int_v1=(1, 2))}},
            ):
                path.write_text(json.dumps(payload))
                with self.assertRaisesRegex(ValueError, "requested window"):
                    REPORT.load_toolkit_results(
                        path, date(2026, 7, 21), date(2026, 7, 22)
                    )

    def test_partial_toolkit_input_lists_missing_dates(self):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            toolkit_path = directory / "toolkit.json"
            output = directory / "report.md"
            toolkit_path.write_text(
                json.dumps(
                    {
                        "responses": {
                            "2026-07-20": toolkit_response(int_v1=(1, 2)),
                            "2026-07-22": toolkit_response(acp=(3, 4)),
                        }
                    }
                )
            )
            result = REPORT.main(
                [
                    "2026-07-20",
                    "2026-07-22",
                    "--toolkit-input-json",
                    str(toolkit_path),
                    "--output",
                    str(output),
                ]
            )
            rendered = output.read_text()

        self.assertEqual(result, 0)
        self.assertIn("**Toolkit**: partially available", rendered)
        self.assertNotIn("**KUTS**:", rendered)
        self.assertIn("`2026-07-21` (saved response is missing)", rendered)
        self.assertIn(
            "| 2026-07-21 | n/a | n/a | n/a | n/a | "
            "n/a | n/a | n/a | n/a | n/a | n/a |",
            rendered,
        )

    def test_legacy_offline_input_never_queries_either_source(self):
        kuts_response = {
            "status": "Complete",
            "results": [
                result_row(
                    "2026-07-21",
                    turns=2,
                    agent_engine="v1",
                    session_interface="interactive_cli",
                )
            ],
        }
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            kuts_path = directory / "kuts.json"
            output = directory / "report.md"
            kuts_path.write_text(json.dumps(kuts_response))
            with patch.object(REPORT, "query_metrics") as query_kuts, patch.object(
                REPORT, "query_toolkit"
            ) as query_toolkit:
                result = REPORT.main(
                    [
                        "2026-07-21",
                        "--input-json",
                        str(kuts_path),
                        "--output",
                        str(output),
                    ]
                )
            rendered = output.read_text()

        self.assertEqual(result, 0)
        query_kuts.assert_not_called()
        query_toolkit.assert_not_called()
        self.assertIn("**KUTS**: available", rendered)
        self.assertNotIn("**Toolkit**:", rendered)

    def test_explicit_both_can_mix_saved_kuts_with_live_toolkit(self):
        kuts_response = {"status": "Complete", "results": []}
        toolkit = {
            date(2026, 7, 21): REPORT.parse_toolkit_response(
                toolkit_response(acp=(3, 4))
            )
        }
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            kuts_path = directory / "kuts.json"
            output = directory / "report.md"
            kuts_path.write_text(json.dumps(kuts_response))
            with patch.object(REPORT, "query_metrics") as query_kuts, patch.object(
                REPORT, "query_toolkit", return_value=(toolkit, {})
            ) as query_toolkit:
                result = REPORT.main(
                    [
                        "2026-07-21",
                        "--source",
                        "both",
                        "--kuts-input-json",
                        str(kuts_path),
                        "--output",
                        str(output),
                    ]
                )

        self.assertEqual(result, 0)
        query_kuts.assert_not_called()
        query_toolkit.assert_called_once()

    def test_both_sources_writes_partial_report_when_toolkit_is_unavailable(self):
        kuts_response = {
            "status": "Complete",
            "results": [
                result_row(
                    "2026-07-21",
                    heartbeats=2,
                    version_full="2.5.1",
                    release_channel="stable",
                )
            ],
        }
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            kuts_path = directory / "kuts.json"
            output = directory / "report.md"
            kuts_path.write_text(json.dumps(kuts_response))

            result = REPORT.main(
                [
                    "2026-07-21",
                    "--source",
                    "both",
                    "--kuts-input-json",
                    str(kuts_path),
                    "--toolkit-query-script",
                    str(directory / "missing" / "es-query.sh"),
                    "--output",
                    str(output),
                ]
            )
            rendered = output.read_text()

        self.assertEqual(result, 0)
        self.assertIn("**KUTS**: available", rendered)
        self.assertIn(f"saved KUTS JSON `{kuts_path}`", rendered)
        self.assertIn("**Toolkit**: unavailable", rendered)
        self.assertIn("Active Installation-Version Adoption (KUTS)", rendered)
        self.assertNotIn("Detailed V1/V2 Usage (Toolkit)", rendered)

    def test_toolkit_only_offline_report_does_not_claim_kuts_data(self):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            toolkit_path = directory / "toolkit.json"
            output = directory / "report.md"
            toolkit_path.write_text(
                json.dumps(
                    {"responses": {"2026-07-21": toolkit_response(int_v2=(3, 4))}}
                )
            )

            result = REPORT.main(
                [
                    "2026-07-21",
                    "--source",
                    "toolkit",
                    "--toolkit-input-json",
                    str(toolkit_path),
                    "--output",
                    str(output),
                ]
            )
            rendered = output.read_text()

        self.assertEqual(result, 0)
        self.assertIn("**Toolkit**: available", rendered)
        self.assertIn(f"saved Toolkit JSON `{toolkit_path}`", rendered)
        self.assertNotIn("**KUTS**:", rendered)
        self.assertNotIn("Engine Usage (KUTS)", rendered)
        self.assertIn("Detailed V1/V2/ACP Usage (Toolkit)", rendered)


if __name__ == "__main__":
    unittest.main()
