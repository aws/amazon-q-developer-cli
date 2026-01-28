#!/usr/bin/env python3
"""Generate release notes from changelog fragments for nightly builds."""

import json
import re
import shutil
import sys
from pathlib import Path


def parse_semver(version: str) -> tuple:
    """Parse semver string like 1.24.2-nightly.3 into comparable tuple."""
    match = re.match(r"(\d+)\.(\d+)\.(\d+)-nightly\.(\d+)", version)
    if not match:
        return (0, 0, 0, 0)
    return (int(match[1]), int(match[2]), int(match[3]), int(match[4]))


def get_previous_version_dir(nightly_released_dir: Path) -> Path | None:
    """Get the most recent version directory in nightly-released."""
    if not nightly_released_dir.exists():
        return None
    
    versions = sorted(
        [d for d in nightly_released_dir.iterdir() if d.is_dir()],
        key=lambda d: parse_semver(d.name),
        reverse=True
    )
    return versions[0] if versions else None


def get_new_fragments(unreleased_dir: Path, prev_version_dir: Path | None) -> list[Path]:
    """Find fragments in unreleased that aren't in the previous version."""
    if not unreleased_dir.exists():
        return []
    
    fragments = list(unreleased_dir.glob("*.json"))
    if not prev_version_dir:
        return fragments
    
    prev_names = {f.name for f in prev_version_dir.glob("*.json")}
    return [f for f in fragments if f.name not in prev_names]


def generate_release_notes(fragments: list[Path]) -> str:
    """Generate release notes from fragment files."""
    if not fragments:
        return "No new changes in this nightly build."
    
    notes = []
    for fragment in fragments:
        with open(fragment) as f:
            data = json.load(f)
        notes.append(f"• [{data['type']}] {data['description']}")
    
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
    nightly_released_dir = changes_dir / "nightly-released"
    
    prev_version_dir = get_previous_version_dir(nightly_released_dir)
    new_fragments = get_new_fragments(unreleased_dir, prev_version_dir)
    
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
