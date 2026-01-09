#!/usr/bin/env python3
import re
import sys

def bump_version(current_version: str, increment: str, release_quality: str) -> str:
    """Bump version based on increment type and release quality."""
    
    if release_quality == "nightly":
        # Handle nightly pre-release versioning
        nightly_match = re.match(r'^(\d+\.\d+\.\d+)-nightly\.(\d+)$', current_version)
        if nightly_match:
            # Already a nightly version - increment nightly counter
            base = nightly_match.group(1)
            counter = int(nightly_match.group(2)) + 1
            return f"{base}-nightly.{counter}"
        else:
            # Coming from stable/insider version - bump patch then add nightly.1
            base_match = re.match(r'^(\d+)\.(\d+)\.(\d+)', current_version)
            if base_match:
                major = int(base_match.group(1))
                minor = int(base_match.group(2))
                patch = int(base_match.group(3))
                
                # Bump patch version first
                new_patch = patch + 1
                return f"{major}.{minor}.{new_patch}-nightly.1"
            raise ValueError(f"Invalid version format: {current_version}")
    
    # Standard semver bump for stable/insider - strip any pre-release identifier
    match = re.match(r'^(\d+)\.(\d+)\.(\d+)', current_version)
    if not match:
        raise ValueError(f"Invalid version format: {current_version}")
    
    major, minor, patch = int(match.group(1)), int(match.group(2)), int(match.group(3))
    
    if increment == "major":
        return f"{major + 1}.0.0"
    elif increment == "minor":
        return f"{major}.{minor + 1}.0"
    elif increment == "patch":
        return f"{major}.{minor}.{patch + 1}"
    else:
        raise ValueError(f"Invalid increment: {increment}")

if __name__ == "__main__":
    if len(sys.argv) != 4:
        print("Usage: bump_version.py <current_version> <increment> <release_quality>")
        sys.exit(1)
    
    current = sys.argv[1]
    increment = sys.argv[2]
    quality = sys.argv[3]
    
    try:
        new_version = bump_version(current, increment, quality)
        print(new_version)
    except ValueError as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)
