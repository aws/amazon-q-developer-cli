---
name: release-process-sop
description: Kiro CLI Release Process SOP covering pre-release validation, release cadence, hotfix procedures, feature rollout, and rollback verification. Created as corrective action AI#3163736 from COE-393225. Use when planning releases, executing hotfixes, managing feature rollouts, or verifying fallback paths.
---

# Kiro CLI Release Process SOP

## Purpose

This SOP defines the release process for Kiro CLI, covering pre-release validation, release cadence, hotfix procedures, and rollback verification. It was created as a corrective action (AI#3163736) from COE-393225 (Kiro CLI TUI - Internal Rollout Issues), which produced 3 Sev-2 incidents, a recalled hotfix, and a prompt cache bust affecting 58,000 users due to the absence of a standardized release process.

## Scope

This SOP applies to all Kiro CLI releases: regular weekly releases, feature flag rollouts, and hotfixes. It covers both internal (Amazon toolbox) and external (CloudFront) distribution channels simultaneously. Every team member who ships a Kiro CLI version must follow this process.

**Automation**: This process is executed by the oncall agent, which is pointed at the step-by-step SOP in GitHub (`docs/oncall/kiro_cli_release_sop.md`). The oncall engineer's role is to monitor the agent's execution and intervene only if it strays. Refine the agent prompts as needed based on observed behavior.

## Roles and Escalation

- **On-Call Engineer**: Responsible for finalizing the release candidate each Wednesday and shepherding it through beta and prod.
- **Release Owner**: The engineer executing a specific release or hotfix. May be the on-call or another team member.
- **CSM Coordination**: For releases with user-facing feature changes, the release owner must coordinate with the CSM team (current POC: @jayrava) on external documentation updates before the prod release.
- **Escalation**: If a release must be paused or rolled back and the release owner is unavailable, or if there is disagreement on whether to fix forward vs. roll back, escalate to SDM and PMT.

---

## Release Cadence

### Weekly Cycle

| Day | Activity | Channel | Owner |
|-----|----------|---------|-------|
| Daily | All merges to main automatically build and publish to nightly | nightly | on-call |
| Wednesday | On-call finalizes the release candidate (RC) from nightly, pushes to beta | beta | on-call |
| Monday | Beta version is promoted to prod (stable) | stable | on-call |

### Key Rules

- Internal and external users receive the same release simultaneously. There is no internal-only staging period for regular releases.
- The on-call owns the Wednesday RC cut and Monday prod promotion.
- If a blocking issue is found in beta between Wednesday and Monday, the on-call must either fix forward (new RC) or skip the release. Do not promote a broken beta to prod.

### Versioning (Minor vs Patch)

Follow Semantic Versioning:

- **Patch bump** (X.Y.Z → X.Y.Z+1): Bug fixes, performance improvements, and internal changes with no new user-facing functionality.
- **Minor bump** (X.Y.Z → X.Y+1.0): New features, new commands, new flags, or any change that adds user-facing functionality.

This applies regardless of release cadence — a weekly release that contains only bug fixes should still be a patch bump, and a hotfix that introduces a new feature should be a minor bump.

---

## 1. Pre-Release Validation

Every release must pass the following validation steps before shipping. No exceptions for hotfixes.

### 1.1 Smoke Test Suite

Run the smoke test suite against each target environment. The suite must verify all of the following:

| Test | Pass Criteria |
|------|---------------|
| `kiro-cli chat` launches | Produces output within 10 seconds |
| `kiro-cli --classic` launches | Produces classic mode output within 10 seconds |
| `kiro-cli settings chat.ui "classic"` | Switches to classic mode on next launch |
| MCP servers load | All configured MCP servers appear in `/mcp` output |
| `/chat resume` | Resumes a previously saved session |
| Session format compatibility | Sessions saved in TUI can be listed in classic, and vice versa |
| Memory threshold | Process memory stays below 500 MB after 5 minutes of idle |
| Terminal close cleanup | Process exits within 5 seconds of SIGHUP / PTY close, no orphan processes |
| Multiple concurrent sessions | Second session launches or displays a clear error message |

### 1.2 Target Environments

The smoke test suite must pass on all of the following:

- macOS (Universal - Apple Silicon + Intel)
- Linux x86_64 (gnu)
- Linux x86_64 (musl)
- Linux aarch64 (gnu)
- Linux aarch64 (musl)
- Windows x86_64
- Cloud Desktop (uses Linux musl)

### 1.3 Backward Compatibility Check

When a release changes command syntax, flag names, or invocation patterns, the release owner must verify that either (a) the old syntax still works via a backward-compatible alias, or (b) the change has been through a deprecation cycle of at least 2 prior releases with a deprecation warning.

### 1.4 Release Gate

A release is blocked until:

- All smoke tests pass on all target environments
- Backward compatibility check is complete (if applicable)
- The release owner has signed off in the release tracking ticket

---

## 2. Feature Rollout (Gradual Dial-Up)

Gradual rollout via `crates/chat-cli/rollout.json` is used only when a feature requires it (e.g., a new default UI mode, a major architectural change). Most features ship at 100% to all users in the normal weekly release cycle.

### 2.1 When to Use Gradual Rollout

Use gradual rollout when:

- The change alters the default user experience for all users (e.g., switching from classic to TUI)
- The change has high blast radius and limited ability to validate pre-release (e.g., prompt construction changes that affect cache behavior)
- The team explicitly decides a feature needs staged exposure

The feature owner decides the rollout stages and percentages based on the risk profile. There is no fixed stage template — it depends on the feature.

### 2.2 Rollout Mechanism

The rollout is controlled by `crates/chat-cli/rollout.json`. Each feature has a `treatment_percent` (0–100) and a `segment` field (`all` or `internal`). The user's `client_id` is hashed with the feature name to deterministically assign them to TREATMENT or CONTROL.

- **To advance a rollout**: update `treatment_percent`, commit, merge, and release through the normal weekly cycle.
- **To roll back**: set `treatment_percent` to 0 and ship a new release, or recall the affected version via `toolbox-vendor-ops`.

### 2.3 Metrics Gates (for Gradual Rollouts)

When a feature is under gradual rollout, the following metrics must remain within baseline thresholds before advancing to the next stage. Baselines are calculated from the 7-day average prior to rollout start.

| # | Metric | Threshold | Source |
|---|--------|-----------|--------|
| 1 | Crash rate | No more than 1% above baseline | Kibana telemetry |
| 2 | Process memory (p99) | Below 500 MB | Kibana telemetry |
| 3 | Error rate | No more than 10% above baseline | Kibana telemetry |
| 4 | Prompt cache hit rate | Above 90% | CloudWatch (account 243221828482) |
| 5 | Token consumption | No more than 10% of baseline | CloudWatch (account 243221828482) |
| 6 | Model throttling rate | No sustained throttling (>30 min) | CloudWatch (account 243221828482) |

Backend metrics (items 4–6) must be checked per CLI version. A cache hit rate drop below 90% or token consumption spike above 10% of baseline is a rollout blocker.

---

## 3. Hotfix Procedure

A hotfix is any release shipped to address a Sev-2 or higher incident outside the normal weekly cadence.

### 3.1 Hotfix Validation

Hotfixes must pass the same smoke test suite as regular releases (Section 1.1). There are no exceptions. The v1.29.4 recall demonstrated that skipping smoke tests on a hotfix can produce a worse outcome than the original bug.

If the smoke test suite takes too long for the urgency of the incident, the release owner must at minimum verify:

- `kiro-cli chat` launches and produces output.
- `kiro-cli --classic` launches and produces output.
- The specific fix resolves the reported issue.

### 3.2 Hotfix Rollout

Hotfixes bypass the weekly cadence and go directly to prod after validation:

| Stage | Minimum Bake |
|-------|--------------|
| Beta | 4 hours |
| Prod | N/A |

The release owner may compress further for active Sev-2 incidents, but must document the justification in the release ticket.

### 3.3 Recalled Release Procedure

If a hotfix or release must be recalled after shipping:

1. Recall via documented SOP (see "Recalling a Toolbox Version" in `docs/oncall/kiro_cli_release_sop.md`).
2. Post in `#kiro-cli-internal-software-builders` with: what happened, which versions are affected, and the workaround.
3. File a Sev-2 ticket for the regression introduced by the hotfix.
4. The next hotfix attempt must pass the full smoke test suite with no exceptions.

---

## 4. Fallback Path Verification

Every release must maintain a working fallback path for users. Today, the fallback path is the `--classic` flag and the `kiro-cli settings chat.ui "classic"` setting.

### 4.1 Verification Steps

Before every release, the release owner must verify:

1. `kiro-cli --classic` launches a classic session.
2. `kiro-cli settings chat.ui "classic"` persists the setting.
3. The next `kiro-cli chat` invocation uses classic mode after the setting is applied.

These checks are included in the smoke test suite (Section 1.1) but are called out here because a broken fallback path was the primary amplifier of the April 2–7 incident.

### 4.2 Fallback Path Changes

If a release modifies the fallback mechanism (renames the flag, changes the settings key, etc.), the release owner must update this SOP and the smoke test suite before shipping.

---

## 5. Release Tracking

### 5.1 Release Ticket

The on-call must create a tracking ticket in the Amazon Q for CLI queue for each weekly release:

- **Title**: `Kiro CLI <VERSION>`
- **CTI**: `Kiro / CLI / Intake`
- **Severity**: Sev-5
- **Description**: Tracking ticket for the release, including deployment steps and verification results

Document each deployment step (toolbox beta, prod, CloudFront) as comments on this ticket.

### 5.2 Release Tracker File

Create a release tracker at `docs/oncall/releases/vX.Y.Z.md` using the template at `docs/oncall/releases/TEMPLATE.md` to document progress through each step.

---

## 6. Communication

### 6.1 Release Announcements

Every release that changes user-facing behavior must be announced in `#kiro-cli-internal-software-builders` with:

- Version number
- What changed
- How to fall back if something breaks
- Link to the changelog or release notes

### 6.2 External Documentation

For releases with user-facing feature changes, the release owner must coordinate with the CSM team on external documentation updates before the Monday prod promotion.

### 6.3 Incident Communication

During an active incident caused by a release:

- Post an initial update within 1 hour of detection.
- Post follow-up updates at least every 4 hours until resolved.
- Post a final update when the fix is shipped, including the fixed version number and any remaining workarounds.

---

## 7. Post-Release Review

After each Monday prod promotion, the on-call must conduct a brief post-release review within 1 business day. The review covers:

- Were all smoke tests run and passing before beta and prod promotion?
- Were any metrics gates breached?
- Were any user-reported issues missed by the smoke test suite? If so, add them to the suite.
- Were backend metrics (cache hit rate, token consumption) stable throughout?

The review is documented in the release ticket and does not require a meeting.

---

## References

- Existing release SOP (step-by-step commands): `docs/oncall/kiro_cli_release_sop.md`
- Build and release infrastructure: `docs/oncall/build_release_process.md`
- Rollout configuration: `crates/chat-cli/rollout.json`
- COE: COE-393225
- Recall procedure: See "Recalling a Toolbox Version" in `docs/oncall/kiro_cli_release_sop.md`
- Release tracker template: `docs/oncall/releases/TEMPLATE.md`
