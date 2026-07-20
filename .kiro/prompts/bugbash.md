---
description: Create a bug bash document for testing new versions of the CLI
---

# Bug Bash Document Creator

**Ask the user for:**
1. Version number (e.g., 1.25.0)
2. Commit SHA to test

Then create a "Bug Bash" quip document following this format (`{{...}}` content is dynamically substituted):

```markdown
# Bug Bash {{VERSION}}

## Instructions

```sh
toolbox install kiro-cli --channel nightly --force
```

## Change Log

{{
Lists for each of the change types in the changelog fragments for this release. Release changes are the fragments added to `.changes/` between the previous release tag and this version's tag — e.g. `git diff <previous_tag>..<this_tag> --name-only --diff-filter=A -- '.changes/*.json'`. Read each fragment's `type` and use its `description` as the list item content.

For example:

### Added
...

### Changed
...

### Fixed
...
}}

## Bug List

{{
Table with the columns:
Alias, Owner, Description, Reproduce Steps, Proposed Fix, Screenshots, Severity, Status, Fixes

Leave 3 empty rows for users to fill in
}}
```
