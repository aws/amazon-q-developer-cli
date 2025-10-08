# Q CLI Repository Maintenance

## Initial Setup After Fork

### 1. Add upstream remote to track mainline
```bash
git remote add upstream https://github.com/aws/amazon-q-developer-cli.git
```

### 2. Create feature branch
```bash
git checkout -b your-large-scale-feature
```

### 3. Stay synchronized with mainline
```bash
# Regular sync (do this frequently)
git fetch upstream
git checkout main
git merge upstream/main
git push origin main

# Rebase your feature branch
git checkout your-large-scale-feature
git rebase main
```

## Before Starting Large Changes
- Open GitHub issue to discuss significant work
- Get team approval for large-scale features
- Coordinate with maintainers to avoid wasted effort
