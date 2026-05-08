#!/usr/bin/env python3
"""Generate release notes from changelog fragments for nightly builds."""

import json
import shutil
import sys
from pathlib import Path


def get_all_released_fragment_names(changes_dir: Path) -> set[str]:
    """Get all fragment names already included in any previous nightly or stable release."""
    names = set()
    for subdir in ("nightly-released", "released"):
        parent = changes_dir / subdir
        if not parent.exists():
            continue
        for version_dir in parent.iterdir():
            if version_dir.is_dir():
                names.update(f.name for f in version_dir.glob("*.json"))
    return names


def get_new_fragments(unreleased_dir: Path, changes_dir: Path) -> list[Path]:
    """Find fragments in unreleased that haven't been included in any previous nightly or stable release."""
    if not unreleased_dir.exists():
        return []
    
    fragments = list(unreleased_dir.glob("*.json"))
    released_names = get_all_released_fragment_names(changes_dir)
    if not released_names:
        return fragments
    
    return [f for f in fragments if f.name not in released_names]


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
    
    new_fragments = get_new_fragments(unreleased_dir, changes_dir)
    
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
