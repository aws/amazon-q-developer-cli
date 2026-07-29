#!/usr/bin/env python3
"""Fold a kr-review-run output dir into ONE self-contained review HTML.

Reads manifest.tsv (scenario titles/refs/checks/frames) plus the knight-rider
frame .txt captures and inlines everything as <pre> blocks — no iframes, no
external refs, works offline (lesson from the debug-0722 report kit).

Usage: python3 gen-review-report.py <outdir>  ->  <outdir>/review-report.html
"""
import html
import json
import re
import sys
from pathlib import Path


def main() -> int:
    out = Path(sys.argv[1] if len(sys.argv) > 1 else ".")
    manifest = out / "manifest.tsv"
    frames_dir = out / "frames"
    if not manifest.exists():
        print(f"no manifest.tsv in {out}", file=sys.stderr)
        return 1

    # scenario id -> {title, ref, checks: [(status, text)], frames: [label]}
    scenarios: dict[str, dict] = {}
    order: list[str] = []
    for line in manifest.read_text().splitlines():
        if not line.strip():
            continue
        sid, kind, status, text = line.split("\t", 3)
        if sid not in scenarios:
            scenarios[sid] = {"title": "", "ref": "", "checks": [], "frames": [], "notes": []}
            order.append(sid)
        s = scenarios[sid]
        if kind == "title":
            s["title"] = text
        elif kind == "ref":
            s["ref"] = text
        elif kind == "check":
            s["checks"].append((status, text))
        elif kind == "frame":
            s["frames"].append(text)
        elif kind == "note":
            s["notes"].append(text)

    # frame label -> captured text (KR writes "<idx>-<safeLabel>.txt")
    frame_text: dict[str, str] = {}
    if frames_dir.exists():
        for f in sorted(frames_dir.glob("*.txt")):
            m = re.match(r"\d+-(.+)\.txt$", f.name)
            if m:
                frame_text[m.group(1)] = f.read_text(errors="replace")
    listing = out / "frames-listing.txt"

    def esc(s: str) -> str:
        return html.escape(s)

    def safe(label: str) -> str:
        # Mirror knight-rider's sanitizer exactly (knight-rider.ts:197):
        # each non [a-zA-Z0-9_-] char becomes '_' (per-char, not collapsed).
        return re.sub(r"[^a-zA-Z0-9_-]", "_", label)

    # Reviewer-facing catalog (WHAT each scenario is / HOW it is validated),
    # maintained next to this script; folded into each section when present.
    catalog_path = Path(__file__).parent / "scenario-catalog.json"
    catalog: dict = {}
    invariant: dict = {}
    if catalog_path.exists():
        cat = json.loads(catalog_path.read_text())
        catalog = cat.get("scenarios", {})
        invariant = cat.get("invariant", {})
        skipped = cat.get("skipped", {})

    total_pass = sum(1 for s in scenarios.values() for st, _ in s["checks"] if st == "PASS")
    total_fail = sum(1 for s in scenarios.values() for st, _ in s["checks"] if st == "FAIL")

    parts: list[str] = []
    parts.append(f"""<!doctype html><html><head><meta charset="utf-8">
<title>Cloud-sandbox scenario review</title>
<style>
 body {{ font: 14px -apple-system, sans-serif; margin: 0; background:#0f1117; color:#e6e6e6; }}
 header {{ padding: 20px 32px; background:#161a23; border-bottom:1px solid #2a2f3a; position:sticky; top:0; }}
 h1 {{ margin:0 0 4px; font-size:20px; }}
 .sum {{ color:#9aa4b2; font-size:13px; }}
 .sum b.p {{ color:#4ade80; }} .sum b.f {{ color:#f87171; }}
 nav {{ padding: 10px 32px; background:#12151d; border-bottom:1px solid #2a2f3a; font-size:13px; line-height:1.9; }}
 nav a {{ color:#7dd3fc; text-decoration:none; margin-right:14px; white-space:nowrap; }}
 section {{ padding: 22px 32px; border-bottom:1px solid #2a2f3a; }}
 h2 {{ font-size:16px; margin:0 0 2px; }}
 .ref {{ color:#9aa4b2; font-size:12px; margin-bottom:10px; }}
 ul.checks {{ list-style:none; padding:0; margin:8px 0 14px; }}
 ul.checks li {{ padding:2px 0; font-size:13px; }}
 li.PASS::before {{ content:"✓ "; color:#4ade80; font-weight:bold; }}
 li.FAIL::before {{ content:"✗ "; color:#f87171; font-weight:bold; }}
 .note {{ color:#9aa4b2; font-size:12px; font-style:italic; }}
 details {{ margin:10px 0; }}
 summary {{ cursor:pointer; color:#7dd3fc; font-size:13px; }}
 pre {{ background:#0a0c10; border:1px solid #2a2f3a; border-radius:6px; padding:12px;
        overflow-x:auto; font: 11px/1.35 "SF Mono", Menlo, monospace; color:#d0d6df; }}
 .missing {{ color:#f59e0b; font-size:12px; }}
 .card {{ background:#141823; border:1px solid #2a2f3a; border-left:3px solid #7dd3fc;
          border-radius:6px; padding:10px 14px; margin:8px 0; font-size:13px; line-height:1.5; }}
 .card b {{ color:#7dd3fc; }}
 .card.limits {{ border-left-color:#f59e0b; }} .card.limits b {{ color:#f59e0b; }}
 .card.inv {{ border-left-color:#4ade80; }} .card.inv b {{ color:#4ade80; }}
</style></head><body>
<header><h1>Cloud-sandbox scenario review</h1>
<div class="sum">{len(scenarios)} scenarios · <b class="p">{total_pass} checks passed</b> ·
<b class="f">{total_fail} failed</b> · hermetic mock-BFF run · dark-shipped feature
(KIRO_TEST_MODE=1 = internal-nightly view)</div></header><nav>""")
    for sid in order:
        parts.append(f'<a href="#{sid}">{sid} {esc(scenarios[sid]["title"][:38])}</a>')
    parts.append("</nav>")

    if invariant:
        parts.append('<section id="invariant">'
                     f'<h2>{esc(invariant.get("title", "Global invariant"))}</h2>'
                     f'<div class="card inv"><b>What:</b> {esc(invariant.get("what", ""))}</div>'
                     f'<div class="card inv"><b>How:</b> {esc(invariant.get("how", ""))}</div>'
                     "</section>")

    for sid in order:
        s = scenarios[sid]
        parts.append(f'<section id="{sid}"><h2>{sid} — {esc(s["title"])}</h2>')
        parts.append(f'<div class="ref">Covers: {esc(s["ref"])}</div>')
        entry = catalog.get(sid, {})
        if entry.get("what"):
            parts.append(f'<div class="card"><b>What this scenario is:</b> {esc(entry["what"])}</div>')
        if entry.get("how"):
            parts.append(f'<div class="card"><b>How we validate it:</b> {esc(entry["how"])}</div>')
        if entry.get("limits"):
            parts.append(f'<div class="card limits"><b>Limits:</b> {esc(entry["limits"])}</div>')
        parts.append('<ul class="checks">')
        for status, text in s["checks"]:
            parts.append(f'<li class="{status}">{esc(text)}</li>')
        parts.append("</ul>")
        for n in s["notes"]:
            parts.append(f'<div class="note">{esc(n)}</div>')
        for label in s["frames"]:
            txt = frame_text.get(safe(label))
            if txt is None:
                parts.append(f'<div class="missing">frame missing: {esc(label)}</div>')
            else:
                open_attr = " open" if len(s["frames"]) == 1 else ""
                parts.append(f"<details{open_attr}><summary>frame: {esc(label)}</summary>"
                             f"<pre>{esc(txt)}</pre></details>")
        if sid == "S14" and listing.exists():
            parts.append("<details open><summary>headless --list-sessions output</summary>"
                         f"<pre>{esc(listing.read_text(errors='replace'))}</pre></details>")
        parts.append("</section>")

    if skipped:
        parts.append('<section id="skipped"><h2>Deliberately skipped (known gaps)</h2>')
        for key, entry in skipped.items():
            parts.append(f'<h2 style="font-size:14px;margin-top:14px">{esc(key)}</h2>')
            if entry.get("covers"):
                parts.append(f'<div class="ref">Would cover: {esc(entry["covers"])}</div>')
            if entry.get("why_skipped"):
                parts.append(f'<div class="card limits"><b>Why skipped:</b> {esc(entry["why_skipped"])}</div>')
        parts.append("</section>")

    parts.append("</body></html>")
    report = out / "review-report.html"
    report.write_text("\n".join(parts))
    print(f"wrote {report} ({report.stat().st_size // 1024} KiB)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
