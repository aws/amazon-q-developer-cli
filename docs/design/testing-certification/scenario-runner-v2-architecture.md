# Scenario Runner V2 Architecture

This file remains as a stable path for existing references, but the current design is intentionally collapsed into [`10-lld-scenario-framework.md`](./10-lld-scenario-framework.md).

The implementation direction in this branch is:

- a generic scenario-runner framework with pluggable backends
- deterministic ACP-mock coverage for KAS
- live runtime-aware execution for `v2` and `kas`
- scenario applicability defined in `scenarios.json` via engine and backend allowlists

The next stacked PR extends the same framework with deterministic `v2` support.
