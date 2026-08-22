"""Behavioral tests for the shared quality-comment publisher."""

from __future__ import annotations

import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
PUBLISHER = ROOT / ".github/scripts/publish-quality-report.sh"
WORKFLOW = ROOT / ".github/workflows/visual-quality-report.yml"
CODE_QUALITY_WORKFLOW = ROOT / ".github/workflows/code-quality.yml"
ACTION = ROOT / ".github/actions/publish-quality-report/action.yml"
HEAD_SHA = "a" * 40
STALE_SHA = "b" * 40

FAKE_GH = """#!/usr/bin/env python3
import json
import os
import sys
from pathlib import Path

args = sys.argv[1:]
log = Path(os.environ["FAKE_GH_LOG"])
with log.open("a", encoding="utf-8") as stream:
    stream.write(json.dumps(args) + "\\n")

endpoint = next((arg for arg in args if arg.startswith("repos/")), "")
if "/pulls/" in endpoint:
    state = Path(os.environ["FAKE_HEAD_STATE"])
    call = int(state.read_text(encoding="utf-8")) if state.exists() else 0
    heads = os.environ["FAKE_HEADS"].split(",")
    print(heads[min(call, len(heads) - 1)])
    state.write_text(str(call + 1), encoding="utf-8")
elif "/issues/" in endpoint and endpoint.endswith("comments?per_page=100"):
    if os.environ.get("FAKE_LIST_FAILURE") == "1":
        sys.exit(42)
    print(os.environ.get("FAKE_COMMENT_IDS", ""))
elif "/issues/comments/" in endpoint and "--method" not in args:
    print(Path(os.environ["FAKE_CURRENT_BODY"]).read_text(encoding="utf-8"))
elif "--method" in args:
    field = args[args.index("--raw-field") + 1]
    Path(os.environ["FAKE_WRITTEN_BODY"]).write_text(
        field.removeprefix("body="), encoding="utf-8"
    )
else:
    raise SystemExit(f"unexpected gh invocation: {args}")
"""


class PublishQualityReportTests(unittest.TestCase):
    def run_publisher(
        self,
        directory: Path,
        *,
        comment_ids: str = "",
        current_body: str = "",
        heads: tuple[str, ...] = (HEAD_SHA, HEAD_SHA),
        list_failure: bool = False,
    ) -> subprocess.CompletedProcess[str]:
        fake_bin = directory / "bin"
        fake_bin.mkdir()
        fake_gh = fake_bin / "gh"
        fake_gh.write_text(FAKE_GH, encoding="utf-8")
        fake_gh.chmod(0o755)

        content = directory / "section.md"
        content.write_text("## Code Quality\n\nCurrent metrics\n", encoding="utf-8")
        current = directory / "current.md"
        current.write_text(current_body, encoding="utf-8")

        env = os.environ.copy()
        env.update(
            {
                "EXPECTED_HEAD_SHA": HEAD_SHA,
                "FAKE_COMMENT_IDS": comment_ids,
                "FAKE_CURRENT_BODY": str(current),
                "FAKE_GH_LOG": str(directory / "gh.log"),
                "FAKE_HEADS": ",".join(heads),
                "FAKE_HEAD_STATE": str(directory / "head-state"),
                "FAKE_LIST_FAILURE": "1" if list_failure else "0",
                "FAKE_WRITTEN_BODY": str(directory / "written.md"),
                "GH_TOKEN": "test-token",
                "GITHUB_REPOSITORY": "owner/repository",
                "GITHUB_STEP_SUMMARY": str(directory / "summary.md"),
                "GITHUB_WORKSPACE": str(ROOT),
                "PATH": f"{fake_bin}{os.pathsep}{env['PATH']}",
                "PR_NUMBER": "42",
                "REPORT_CONTENT": str(content),
                "REPORT_SECTION": "code-quality",
                "RUNNER_TEMP": str(directory),
            }
        )
        return subprocess.run(
            ["bash", str(PUBLISHER)],
            cwd=ROOT,
            env=env,
            check=False,
            capture_output=True,
            text=True,
        )

    def calls(self, directory: Path) -> list[list[str]]:
        log = directory / "gh.log"
        return [
            json.loads(line)
            for line in log.read_text(encoding="utf-8").splitlines()
        ]

    def write_methods(self, directory: Path) -> list[str]:
        return [
            call[call.index("--method") + 1]
            for call in self.calls(directory)
            if "--method" in call
        ]

    def test_posts_the_first_sha_stamped_section(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)

            result = self.run_publisher(directory)

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(self.write_methods(directory), ["POST"])
            body = (directory / "written.md").read_text(encoding="utf-8")
            self.assertIn("quality-metrics-section:code-quality:start", body)
            self.assertIn(f"_PR head revision: `{HEAD_SHA[:12]}`._", body)

    def test_patches_an_existing_comment_without_dropping_other_sections(self) -> None:
        existing = (
            "<!-- code-quality-coverage-report -->\n\n"
            "<!-- quality-metrics-section:visual-stories:start -->\n"
            "visual\n"
            "<!-- quality-metrics-section:visual-stories:end -->\n"
        )
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)

            result = self.run_publisher(
                directory, comment_ids="99", current_body=existing
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(self.write_methods(directory), ["PATCH"])
            body = (directory / "written.md").read_text(encoding="utf-8")
            self.assertIn("Current metrics", body)
            self.assertIn("visual", body)

    def test_comment_listing_failure_cannot_create_a_duplicate(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)

            result = self.run_publisher(directory, list_failure=True)

            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(self.write_methods(directory), [])
            self.assertIn(
                "Publisher command failed",
                (directory / "summary.md").read_text(encoding="utf-8"),
            )

    def test_duplicate_comment_discovery_updates_the_oldest_comment(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)

            result = self.run_publisher(directory, comment_ids="99\n98")

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("updating canonical comment 98", result.stdout)
            self.assertEqual(self.write_methods(directory), ["PATCH"])
            self.assertTrue(
                any(
                    "repos/owner/repository/issues/comments/98" in call
                    for call in self.calls(directory)
                )
            )

    def test_head_change_before_write_skips_the_stale_report(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)

            result = self.run_publisher(
                directory, comment_ids="99", heads=(HEAD_SHA, STALE_SHA)
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("Skipping stale report", result.stdout)
            self.assertEqual(self.write_methods(directory), [])

    def test_trusted_workflow_resolves_one_pr_for_the_fixed_run_head(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")
        code_quality_workflow = CODE_QUALITY_WORKFLOW.read_text(encoding="utf-8")
        action = ACTION.read_text(encoding="utf-8")
        header = workflow[: workflow.index("\njobs:\n")]

        self.assertIn(
            "workflow_run:\n    workflows: [RC Certification, Code Quality]\n"
            "    types: [completed]",
            header,
        )
        self.assertIn(
            "permissions:\n  actions: read\n  contents: read\n  pull-requests: write",
            header,
        )
        self.assertIn(
            "group: quality-metrics-comment-${{ github.event.workflow_run.head_sha }}",
            workflow,
        )
        self.assertIn(
            "github.event.workflow_run.head_repository.id == "
            "github.event.repository.id",
            workflow,
        )
        self.assertIn("Expected exactly one same-repository PR", workflow)
        self.assertIn(
            "github.event.workflow_run.name == 'Code Quality'", workflow
        )
        self.assertIn(
            "github.event.workflow_run.name == 'RC Certification'", workflow
        )
        self.assertIn(
            "expected-head-sha: ${{ github.event.workflow_run.head_sha }}",
            workflow,
        )
        self.assertIn(
            "ref: ${{ github.event.repository.default_branch }}", workflow
        )
        self.assertIn("run-id: ${{ github.event.workflow_run.id }}", workflow)
        self.assertIn("name: code-quality-coverage-report", workflow)
        self.assertIn("name: verification-acp-integration-linux", workflow)
        self.assertIn(
            "--report acp-metrics/coverage/acp-integration.json", workflow
        )
        self.assertIn("section: acp-integration", workflow)
        self.assertNotIn("pull_requests[0]", workflow)
        self.assertNotIn("inputs.source_sha", workflow)
        self.assertNotIn("\n    continue-on-error: true\n", workflow)
        self.assertNotIn("pull-requests: write", code_quality_workflow)
        self.assertNotIn("Publish Code Quality section", code_quality_workflow)
        self.assertIn(
            '$GITHUB_ACTION_PATH/../../scripts/publish-quality-report.sh',
            action,
        )
        self.assertNotIn("$GITHUB_WORKSPACE", action)


if __name__ == "__main__":
    unittest.main()
