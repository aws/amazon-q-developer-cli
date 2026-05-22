#!/usr/bin/env python3
"""Generate release notes from changelog fragments for nightly builds.

Candidate fragments come from two sources:

1. ``.changes/unreleased/`` - fragments not yet in any stable release.
2. ``.changes/released/v*/`` - fragments that were moved into a stable
   release since the last nightly. Without this, fragments merged to main
   then swept into a stable release before the next nightly would silently
   skip the nightly Slack notification.

Already-announced fragments are excluded by checking their filename against
the most recent ``.changes/nightly-released/<version>/`` directory.
"""

from __future__ import annotations

import json
import shutil
import sys
from pathlib import Path
from typing import Optional


def get_all_previously_announced(nightly_released_dir: Path) -> set[str]:
    """Get all fragment names ever announced in any previous nightly."""
    names: set[str] = set()
    if not nightly_released_dir.exists():
        return names
    for version_dir in nightly_released_dir.iterdir():
        if version_dir.is_dir():
            names.update(f.name for f in version_dir.glob("*.json"))
    return names


def get_latest_nightly_version(nightly_released_dir: Path) -> Optional[str]:
    """Get the name of the second-most-recent nightly-released version dir.
    
    We use the second-most-recent because the latest one is the snapshot
    being built right now. We want to find stable releases that appeared
    since the PREVIOUS nightly.
    """
    if not nightly_released_dir.exists():
        return None
    versions = sorted(
        [d.name for d in nightly_released_dir.iterdir() if d.is_dir()],
    )
    # Return second-to-last (the previous nightly before the current one)
    if len(versions) >= 2:
        return versions[-2]
    return None


def parse_stable_version(name: str) -> tuple:
    """Parse stable version dir name like 'v2.4.0' into comparable tuple."""
    import re
    match = re.match(r"v?(\d+)\.(\d+)\.(\d+)", name)
    if not match:
        return (0, 0, 0)
    return (int(match[1]), int(match[2]), int(match[3]))


def get_new_fragments(
    unreleased_dir: Path,
    released_dir: Path,
    nightly_released_dir: Path,
) -> list[Path]:
    """Find fragments that should be announced in this nightly.

    Candidates: unreleased/ + released/v*/ dirs whose version is >= the
    base version of the previous nightly.
    Exclusions: anything already announced in ANY nightly-released/<ver>/.
    """
    prev_names = get_all_previously_announced(nightly_released_dir)

    # Determine cutoff: stable releases at or above the previous nightly's base
    prev_nightly = get_latest_nightly_version(nightly_released_dir)
    import re
    nightly_base = (0, 0, 0)
    if prev_nightly:
        m = re.match(r"(\d+)\.(\d+)\.(\d+)", prev_nightly)
        if m:
            nightly_base = (int(m[1]), int(m[2]), int(m[3]))

    candidates: list[Path] = []

    # Source 1: unreleased fragments
    if unreleased_dir.exists():
        candidates.extend(unreleased_dir.glob("*.json"))

    # Source 2: fragments in stable releases newer than the last nightly's base
    if released_dir.exists():
        for version_dir in released_dir.iterdir():
            if version_dir.is_dir() and parse_stable_version(version_dir.name) > nightly_base:
                candidates.extend(version_dir.glob("*.json"))

    # Dedupe and exclude already-announced
    seen: set[str] = set()
    out: list[Path] = []
    for f in candidates:
        if f.name in prev_names or f.name in seen:
            continue
        seen.add(f.name)
        out.append(f)
    return out


def generate_release_notes(fragments: list[Path]) -> str:
    """Generate release notes from fragment files."""
    if not fragments:
        return "No new changes in this nightly build."

    notes = []
    for fragment in fragments:
        with open(fragment) as f:
            data = json.load(f)
        notes.append(f"\u2022 [{data['type']}] {data['description']}")

    return "\n".join(notes)


def mark_fragments_released(fragments: list[Path], version_dir: Path) -> bool:
    """Copy new fragments to the version directory. Returns True if any copied."""
    if not fragments:
        return False

    version_dir.mkdir(parents=True, exist_ok=True)
    for fragment in fragments:
        shutil.copy(fragment, version_dir / fragment.name)
    return True


def main():
    if len(sys.argv) < 2:
        print("Usage: nightly-release-notes.py <command> [version]", file=sys.stderr)
        print("Commands: generate, mark", file=sys.stderr)
        sys.exit(1)

    command = sys.argv[1]
    changes_dir = Path(".changes")
    unreleased_dir = changes_dir / "unreleased"
    released_dir = changes_dir / "released"
    nightly_released_dir = changes_dir / "nightly-released"

    new_fragments = get_new_fragments(unreleased_dir, released_dir, nightly_released_dir)

    if command == "generate":
        print(generate_release_notes(new_fragments))

    elif command == "mark":
        if len(sys.argv) < 3:
            print("Usage: nightly-release-notes.py mark <version>", file=sys.stderr)
            sys.exit(1)
        version = sys.argv[2]
        version_dir = nightly_released_dir / version
        if mark_fragments_released(new_fragments, version_dir):
            print(f"Marked {len(new_fragments)} fragments as released in {version}")
        else:
            print("No new fragments to mark")

    else:
        print(f"Unknown command: {command}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
