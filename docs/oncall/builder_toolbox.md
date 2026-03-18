# Builder Toolbox

Documentation for Builder Toolbox operations relevant to Kiro CLI, including vending, registry management, and version recalls.

## Reference

Full documentation is available internally:

- [Vending & Registry Management](https://docs.hub.amazon.dev/builder-toolbox/user-guide/vending-registry-management/) — covers publishing, recalling versions, and managing tool registries

Use `ReadInternalWebsites` with the URL above (and any relevant `#anchor`) to look up specific procedures. For example, to look up how to recall a tool version:

```
https://docs.hub.amazon.dev/builder-toolbox/user-guide/vending-registry-management/#recall-a-tool-version
```

## Common Tasks

### Recall a Tool Version

Use `toolbox-vendor-ops recall` to make a version unavailable for download. Customers on the recalled version will auto-update to the most recent non-recalled version (or a specified `--recommended` version) on their next update cycle.

See the full reference link above for command syntax and examples.
