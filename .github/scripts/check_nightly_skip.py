#!/usr/bin/env python3
import subprocess
import re

def get_last_commit_message():
    result = subprocess.run(
        ["git", "log", "-1", "--pretty=%B"],
        capture_output=True,
        text=True,
        check=True
    )
    return result.stdout.strip()

def is_nightly_build_commit(message):
    # Check if commit message matches nightly build pattern
    pattern = r"Release: Bump version to .* \(nightly\)"
    return bool(re.search(pattern, message))

if __name__ == "__main__":
    commit_message = get_last_commit_message()
    
    if is_nightly_build_commit(commit_message):
        print("true")
    else:
        print("false")
