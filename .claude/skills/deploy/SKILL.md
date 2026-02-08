---
description: Deploy to target environment (staging/production)
---

# /deploy — Deploy

Build and deploy to the target environment.

## Steps

1. Run `make dev` (full verification: fmt + clippy + test)
2. Run `make build-release` (optimized build)
3. Run quality score check
4. Deploy to specified environment

## Environments

| Argument | Action |
|----------|--------|
| `staging` | Deploy to staging |
| `production` | Deploy to production (requires confirmation) |

## Safety Checks

- All tests must pass before deploy
- Quality score must be >= 80 (staging) or >= 95 (production)
- Production deploys require explicit user confirmation
