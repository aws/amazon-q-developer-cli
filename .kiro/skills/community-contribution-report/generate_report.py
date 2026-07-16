#!/usr/bin/env python3
"""Generate the Kiro CLI community contribution report.

Fetches all non-draft PRs since --since from external contributors (not on the
core team, not bots), computes engagement/merge/TTFE metrics, compares against
a prior snapshot, and emits both a Markdown report and a JSON snapshot. A
rich PDF is also generated on the user's Desktop if reportlab is installed.

Run with `--help` for the full argument list.

See ../SKILL.md for the surrounding SOP.
"""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

BOTS = {"github-actions", "dependabot", "codecov-commenter", "sonarcloud"}
DEFAULT_SINCE = "2026-06-15"
SNAPSHOT_DIR = Path(".ops/community-reports")
SKILL_DIR = Path(__file__).resolve().parent


# ------------------------------ helpers ----------------------------------

def parse_iso(s: str | None) -> datetime | None:
    if not s:
        return None
    return datetime.fromisoformat(s.replace("Z", "+00:00"))


def sh_json(cmd: list[str]) -> Any:
    r = subprocess.run(cmd, capture_output=True, text=True, check=True)
    return json.loads(r.stdout)


def load_core_team() -> set[str]:
    path = SKILL_DIR / "core_team.json"
    data = json.loads(path.read_text())
    return set(data["handles"])


def find_previous_snapshot(as_of: str) -> Path | None:
    """Return the most-recent snapshot JSON strictly before as_of, if any."""
    if not SNAPSHOT_DIR.exists():
        return None
    candidates = sorted(SNAPSHOT_DIR.glob("*.json"))
    matching = [p for p in candidates if p.stem < as_of]
    return matching[-1] if matching else None


# ------------------------------ fetch ------------------------------------

def fetch_prs(since: str) -> list[dict]:
    """Fetch all PRs since <since> and their comments."""
    print(f"[fetch] gh pr list created:>={since} ...", file=sys.stderr)
    prs = sh_json([
        "gh", "pr", "list", "--repo", "kiro-team/kiro-cli",
        "--search", f"created:>={since}", "--state", "all", "--limit", "1000",
        "--json", "number,author,state,createdAt,mergedAt,closedAt,updatedAt,"
                  "reviews,title,additions,deletions,isDraft",
    ])
    print(f"[fetch] {len(prs)} PRs, filtering + pulling comments ...",
          file=sys.stderr)
    core = load_core_team()
    contrib = [
        p for p in prs
        if p["author"]["login"] not in core
        and not p["author"]["login"].startswith("app/")
        and not p["isDraft"]
    ]
    for p in contrib:
        try:
            view = sh_json([
                "gh", "pr", "view", str(p["number"]),
                "--repo", "kiro-team/kiro-cli",
                "--json", "number,comments",
            ])
            p["comments"] = view.get("comments", [])
        except subprocess.CalledProcessError:
            p["comments"] = []
    print(f"[fetch] {len(contrib)} community PRs after filters",
          file=sys.stderr)
    return contrib


# ------------------------------ metrics ----------------------------------

def compute_rows(prs: list[dict]) -> list[dict]:
    rows: list[dict] = []
    for p in prs:
        author = p["author"]["login"]
        revs = [
            r for r in p.get("reviews") or []
            if r.get("author")
            and r["author"].get("login") != author
            and not r["author"]["login"].startswith("app/")
            and r["author"]["login"] not in BOTS
        ]
        cmts = [
            c for c in p.get("comments") or []
            if c.get("author")
            and c["author"].get("login") != author
            and not c["author"]["login"].startswith("app/")
            and c["author"]["login"] not in BOTS
        ]
        engagers = sorted(
            {r["author"]["login"] for r in revs}
            | {c["author"]["login"] for c in cmts}
        )
        created = parse_iso(p["createdAt"])
        first_touch = None
        stamps = (
            [parse_iso(r["submittedAt"]) for r in revs]
            + [parse_iso(c["createdAt"]) for c in cmts]
        )
        if stamps:
            first_touch = min(s for s in stamps if s is not None)
        rows.append({
            "number": p["number"],
            "author": author,
            "state": p["state"],
            "createdAt": p["createdAt"],
            "title": p["title"],
            "engaged": bool(engagers),
            "engagers": engagers,
            "ttfe_hours": (
                (first_touch - created).total_seconds() / 3600
                if first_touch else None
            ),
        })
    rows.sort(key=lambda r: r["createdAt"])
    return rows


def summarize(rows: list[dict]) -> dict:
    ttfe = sorted(r["ttfe_hours"] for r in rows if r["ttfe_hours"] is not None)
    n = len(ttfe)
    return {
        "submitted": len(rows),
        "engaged": sum(1 for r in rows if r["engaged"]),
        "merged": sum(1 for r in rows if r["state"] == "MERGED"),
        "open": sum(1 for r in rows if r["state"] == "OPEN"),
        "closed": sum(1 for r in rows if r["state"] == "CLOSED"),
        "ttfe_median_h": ttfe[n // 2] if n else None,
        "ttfe_p75_h": ttfe[int(n * 0.75)] if n else None,
        "ttfe_max_h": ttfe[-1] if n else None,
    }


# ------------------------------ report -----------------------------------

def render_markdown(
    since: str, as_of: str, rows: list[dict], summary: dict,
    previous: dict | None,
) -> str:
    out: list[str] = []
    out.append(f"# Community PR Report — {as_of}\n")
    out.append(f"Window: **{since}** → **{as_of}**  ·  "
               f"Repo: `kiro-team/kiro-cli`\n")
    out.append("## Summary\n")
    out.append("| Metric | Value |")
    out.append("|---|---|")
    out.append(f"| Submitted | {summary['submitted']} |")
    if summary["submitted"]:
        pct_eng = summary["engaged"] / summary["submitted"] * 100
        pct_merged = summary["merged"] / summary["submitted"] * 100
        out.append(f"| Received community engagement | "
                   f"{summary['engaged']} ({pct_eng:.0f}%) |")
        out.append(f"| Merged | {summary['merged']} ({pct_merged:.0f}%) |")
        out.append(f"| Still open | {summary['open']} |")
        out.append(f"| Closed unmerged | {summary['closed']} |")
    if summary["ttfe_median_h"] is not None:
        out.append(f"| TTFE median / p75 | "
                   f"{summary['ttfe_median_h']:.0f}h / "
                   f"{summary['ttfe_p75_h']:.0f}h |")
    out.append("")

    # Delta table
    if previous:
        p = previous["summary"]
        c = summary
        out.append("## Weekly snapshots\n")
        out.append("| Snapshot | Submitted | Engaged | Merged | Open | Closed |")
        out.append("|---|---|---|---|---|---|")
        out.append(f"| {previous['as_of']} | {p['submitted']} | "
                   f"{p['engaged']} | {p['merged']} | "
                   f"{p['open']} | {p['closed']} |")
        out.append(f"| {as_of} | {c['submitted']} | {c['engaged']} | "
                   f"{c['merged']} | {c['open']} | {c['closed']} |")
        out.append(f"| **Δ** | **{c['submitted']-p['submitted']:+d}** | "
                   f"**{c['engaged']-p['engaged']:+d}** | "
                   f"**{c['merged']-p['merged']:+d}** | "
                   f"**{c['open']-p['open']:+d}** | "
                   f"**{c['closed']-p['closed']:+d}** |")
        out.append("")

    # Buckets — newest first
    buckets = build_buckets(rows, since, as_of, previous)
    out.append("## Full PR list — bucketed by snapshot\n")
    for bucket in buckets:
        b_rows = bucket["rows"]
        if not b_rows:
            continue
        out.append(f"### {bucket['label']} — "
                   f"{len(b_rows)} PRs · "
                   f"{sum(1 for r in b_rows if r['engaged'])} engaged · "
                   f"{sum(1 for r in b_rows if r['state']=='MERGED')} merged · "
                   f"{sum(1 for r in b_rows if r['state']=='OPEN')} open · "
                   f"{sum(1 for r in b_rows if r['state']=='CLOSED')} closed\n")
        out.append("| PR | Author | State | Reviewed | Engaged by |")
        out.append("|---|---|---|---|---|")
        for r in b_rows:
            url = f"https://github.com/kiro-team/kiro-cli/pull/{r['number']}"
            yn = "✅" if r["engaged"] else "❌"
            engs = ", ".join(r["engagers"]) or "—"
            out.append(f"| [#{r['number']}]({url}) | {r['author']} | "
                       f"{r['state']} | {yn} | {engs} |")
        out.append("")
    return "\n".join(out)


def build_buckets(
    rows: list[dict], since: str, as_of: str, previous: dict | None,
) -> list[dict]:
    if previous:
        # Note the strict '>' on the "new this snapshot" lower bound: a PR
        # dated exactly `previous["as_of"]` belongs to the prior bucket, not
        # both.
        buckets = [
            {
                "label": f"New this snapshot — {previous['as_of']} → {as_of}",
                "start": previous["as_of"],
                "end": as_of,
                "start_exclusive": True,
            },
            {
                "label": f"Prior snapshot — {since} → {previous['as_of']}",
                "start": since,
                "end": previous["as_of"],
                "start_exclusive": False,
            },
        ]
    else:
        buckets = [{
            "label": f"All — {since} → {as_of}",
            "start": since,
            "end": as_of,
            "start_exclusive": False,
        }]
    for b in buckets:
        b["rows"] = [
            r for r in rows
            if (
                (r["createdAt"][:10] > b["start"] if b["start_exclusive"]
                 else r["createdAt"][:10] >= b["start"])
                and r["createdAt"][:10] <= b["end"]
            )
        ]
    return buckets


# ------------------------------ persistence ------------------------------

def save_snapshot(
    as_of: str, since: str, rows: list[dict], summary: dict,
) -> Path:
    SNAPSHOT_DIR.mkdir(parents=True, exist_ok=True)
    path = SNAPSHOT_DIR / f"{as_of}.json"
    path.write_text(json.dumps({
        "as_of": as_of,
        "since": since,
        "summary": summary,
        "rows": rows,
    }, indent=2))
    return path


def save_markdown(as_of: str, markdown: str) -> Path:
    SNAPSHOT_DIR.mkdir(parents=True, exist_ok=True)
    path = SNAPSHOT_DIR / f"{as_of}.md"
    path.write_text(markdown)
    return path


# ------------------------------ main -------------------------------------

def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--since", default=DEFAULT_SINCE,
                    help=f"Window start (default {DEFAULT_SINCE})")
    ap.add_argument("--as-of", default=datetime.now().strftime("%Y-%m-%d"),
                    help="Window end / snapshot date (default: today)")
    ap.add_argument("--previous", default=None,
                    help="Path to prior snapshot JSON, "
                         "or 'none' to skip delta, or omit to auto-pick")
    ap.add_argument("--no-pdf", action="store_true",
                    help="Skip the PDF (Markdown only)")
    ap.add_argument("--dry-run", action="store_true",
                    help="Print to stdout, do not write files")
    args = ap.parse_args()

    prs = fetch_prs(args.since)
    rows = compute_rows(prs)
    summary = summarize(rows)

    previous: dict | None = None
    if args.previous == "none":
        pass
    elif args.previous:
        previous = json.loads(Path(args.previous).read_text())
    else:
        prev_path = find_previous_snapshot(args.as_of)
        if prev_path:
            previous = json.loads(prev_path.read_text())
            print(f"[snapshot] auto-loaded previous: {prev_path}",
                  file=sys.stderr)

    markdown = render_markdown(args.since, args.as_of, rows, summary, previous)
    if args.dry_run:
        print(markdown)
        return 0

    md_path = save_markdown(args.as_of, markdown)
    json_path = save_snapshot(args.as_of, args.since, rows, summary)
    print(f"[write] {md_path}", file=sys.stderr)
    print(f"[write] {json_path}", file=sys.stderr)

    if not args.no_pdf:
        try:
            import reportlab  # noqa: F401
            print("[pdf] reportlab available — see SKILL.md for the PDF "
                  "renderer companion script if desired", file=sys.stderr)
        except ImportError:
            print("[pdf] reportlab not installed — skipping PDF "
                  "(pip install --user reportlab)", file=sys.stderr)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
