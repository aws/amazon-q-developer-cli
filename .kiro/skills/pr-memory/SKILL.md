# PR Review Memory Skill

Search historical PR reviews for similar patterns, reviewer expertise, and known issues.
Uses semantic similarity (S3 Vectors) + knowledge graph (Neptune) for high-quality retrieval.

## When to use

- Before reviewing a PR: find what reviewers said about similar code
- When suggesting reviewers: find who has expertise in the changed files
- When implementing: find patterns the team has flagged in this area before

## How to use

### Search similar reviews

Invoke Lambda `KiroCLIReviewerQuery` in `us-east-1`, account `551670267384`:

```json
{
  "diff": "<the diff or code snippet being reviewed>",
  "file_path": "<file path being changed, e.g. crates/agent/src/agent/mod.rs>",
  "repo": "kiro-team/kiro-cli",
  "scope": "repo",
  "top_k": 5
}
```

**Response fields:**

- `results[]` — similar past PRs: `pr_number`, `reviewer`, `file_path`, `chunk_type`
- `suggested_reviewers[]` — experts for this file from the knowledge graph
- `known_patterns[]` — patterns previously flagged in this file area

### Ingest a new PR (after merge)

Invoke Lambda `KiroCLIReviewerIterator` in `us-east-1`:

```json
{
  "repo": "kiro-team/kiro-cli",
  "batch_size": 10
}
```

## Auth

Requires `lambda:InvokeFunction` on `KiroCLIReviewerQuery` via `KiroCLIReviewerQueryRole`.

**Who can call this:**
- GitHub Actions (`repo:kiro-team/*`) — via OIDC, assumes `KiroCLIReviewerQueryRole` directly
- Reviewer bot / developers — assume `ReviewerRole` via `ada credentials update --account 551670267384 --role ReviewerRole`, then assume `KiroCLIReviewerQueryRole`

**Reviewer bot flow:**
```python
import boto3
sts = boto3.client('sts')
creds = sts.assume_role(
    RoleArn='arn:aws:iam::551670267384:role/KiroCLIReviewerQueryRole',
    RoleSessionName='reviewer-bot'
)['Credentials']
```

## Example output

```
results:
  PR #1287 | reviewer: brandonskiser | file: crates/agent/src/agent/mod.rs
  PR #659  | reviewer: brandonskiser | file: crates/agent/src/agent/tools/web_fetch.rs
  PR #506  | reviewer: erbenmo      | file: crates/agent/src/agent/mod.rs

suggested_reviewers:
  { reviewer: brandonskiser, review_count: 47 }
  { reviewer: kensave,       review_count: 31 }

known_patterns:
  bare-unwrap
  unsafe-string-slice
```

## Interpreting results

- If `suggested_reviewers` is populated → assign those reviewers
- If `known_patterns` contains `bare-unwrap` → flag any `.unwrap()` in the diff
- If `known_patterns` contains `unsafe-string-slice` → flag any `&s[..N]` byte slicing
- Cross-reference `results` PR numbers with GitHub for full context
