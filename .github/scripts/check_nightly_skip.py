#!/usr/bin/env python3
import subprocess
import re
import sys

def get_last_commit_message(branch="nightly"):
    result = subprocess.run(
        ["git", "log", f"origin/{branch}", "-1", "--pretty=%B"],
        capture_output=True,
        text=True
    )
    if result.returncode != 0:
        # Branch doesn't exist, don't skip
        return ""
    return result.stdout.strip()

def is_nightly_build_commit(message):
    pattern = r"Release: Bump version to .* \(nightly\)"
    return bool(re.search(pattern, message))

if __name__ == "__main__":
    commit_message = get_last_commit_message()
    print("true" if is_nightly_build_commit(commit_message) else "false")
