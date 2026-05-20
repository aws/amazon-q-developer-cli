# kiro-help Bot — Implementation Roadmap

**Status:** Active
**Date:** 2026-05-19
**Spec:** [docs/superpowers/specs/2026-05-13-kiro-help-bot-design.md](../specs/2026-05-13-kiro-help-bot-design.md)

This is the index for the multi-phase implementation of the kiro-help Slack bot. The spec is wide; the work is split into independently mergeable phases. Each phase has its own plan document with checkbox tasks.

## Phase status

| Phase | Document | Status | Outcome |
|---|---|---|---|
| 1 | [2026-05-13-kiro-knowledge-mcp.md](2026-05-13-kiro-knowledge-mcp.md) | ✅ done | Standalone `kiro-knowledge-mcp` crate w/ `--stub` and Bedrock-backed modes |
| Bridge | [2026-05-14-kiro-help-bot-minimal-deploy.md](2026-05-14-kiro-help-bot-minimal-deploy.md) | ✅ done | Single-task ECS Fargate deploy (Iteration 1 of the minimal deploy plan) |
| 2 | [2026-05-19-phase-2-knowledge-mcp-wireup.md](2026-05-19-phase-2-knowledge-mcp-wireup.md) | code complete (smoke test pending deploy) | RAG-grounded Q&A end-to-end: KB + image carrying `kiro-knowledge-mcp` + agent wired |
| 3 | [2026-05-19-phase-3-ingest-pipeline.md](2026-05-19-phase-3-ingest-pipeline.md) | code complete (deploy pending) | Hourly Lambda keeps the corpus current — live source impls + S3 writer + StartIngestionJob shipped |
| 4 | [2026-05-19-phase-4-cross-task-coordinator.md](2026-05-19-phase-4-cross-task-coordinator.md) | code complete (HA verify pending) | DynamoDB-backed leases + transcripts; 2-task HA — DynamoCoordinator + axum dispatch_server shipped |
| 5 | [2026-05-19-phase-5-eval-and-release-automation.md](2026-05-19-phase-5-eval-and-release-automation.md) | code complete (deploy + smoke pending) | Eval suite + GH Actions + CodeBuild → no more local `finch push` |
| 6 | [2026-05-19-phase-6-production-hardening.md](2026-05-19-phase-6-production-hardening.md) | code complete (deploy pending) | Tools (kiro_cli_help built-in + kiro-github-mcp), 👍/👎 feedback persistence + nightly metrics Lambda, alarms, beta+prod split, runbooks all shipped |

## Recommended execution order

Phase 2 first — it unblocks the headline feature and surfaces wiring bugs cheaply.

After Phase 2, Phases 3, 4, and 5 are mostly independent and can run in parallel:

- **Phase 3** (ingest) is a pure additive Lambda + EventBridge change. Touches CDK + new kiro-cli crate; doesn't depend on coordinator or release automation.
- **Phase 4** (coordinator) is a kiro-bot Rust refactor + DynamoDB. Doesn't depend on ingest. Slow to test but bounded scope.
- **Phase 5** (eval + CI) is GH Actions + CodeBuild. The eval gate is more meaningful once Phase 3's ingest has run a few cycles, but the workflow scaffolding can be built earlier.

Phase 6 depends on all of the above being in place.

## What's in flight today

| Repo | Branch | State |
|---|---|---|
| `kiro-team/kiro-cli` | `feature/kiro-bot` | Phase 1 + bridge config dir merged. `kiro_help` built-in agent does not yet load `search_kiro_knowledge`. |
| `Kiro-botCDK` | `mainline` | Single-task Alpha stack deployed. Three local commits ahead of origin (ECS Exec + entrypoint fix + ARM64), plus uncommitted edits to `entrypoint.sh` and `stack.ts`. **Sync those before starting Phase 2.** |

## Spec coverage by phase

| Spec section | Phase | Notes |
|---|---|---|
| 1. kiro-knowledge-mcp crate | 1 ✅ | Crate done. Phase 2 wires it into the runtime. |
| 2. eval crate | 5 | Eval is most useful once corpus is automated (Phase 3). |
| 3. coordinator | 4 | Refactor + DDB. |
| 4. kiro-help agent mode | 6 | Phase 2 piggy-backs on built-in `kiro_help`; Phase 6 splits them. |
| 5. bot config dir | bridge ✅ | Already in repo. Phase 2 + 4 + 6 progressively extend it. |
| 6. tools table | 2 (1 of 8) → 6 (8 of 8) | `search_kiro_knowledge` in Phase 2; rest in Phase 6. |
| Storage (Bedrock KB) | 2 | KB provisioned, manually seeded. |
| Storage (DDB tables) | 4 | All four tables wired. |
| Knowledge pipeline | 2 (manual) → 3 (automated) | |
| Deployment / source split | 5 | Today: bundled binaries + local `finch`. Phase 5: GH Actions + CodeBuild + binary-carrier ECR + digest-pinned runtime. |
| Security | 6 | Channel allowlist + scoped PATs. |
| Observability | 6 | All 8 alarms + dashboards + nightly metrics Lambda. |
| Capacity & cost | 4 | desiredCount: 2, 10 workers/task. |
| Testing | 5 | Eval gate. |
| Rollout | 6 | Beta + prod stacks; manual promotion. |
| Backout | 6 | Runbooks. |

## Things this roadmap deliberately defers (post-v1)

- Per-tenant Slack-app distribution (multi-workspace install).
- `search_dedupe_clusters` integration with the issue-dedupe bot.
- Per-user kiro-cli compute sandboxes (the long-vision goal).
- SSM-backed live channel-allowlist updates.

## Tracking and updates

When a phase task is completed, check the box in the per-phase plan document. When a phase is fully done, update the row in the **Phase status** table at the top of this file.

If during execution a plan turns out to be wrong (SDK version mismatch, IAM surface different than expected, etc.), update the per-phase document in place — it's the live source of truth, not the spec.
