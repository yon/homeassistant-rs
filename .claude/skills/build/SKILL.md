---
description: Build the project (compile all workspace crates)
---

# /build — Build the Project

Run the project build and report results.

## Steps

1. Run `make build`
2. Report: success/failure, warnings count, build time
3. If failed, show the error output and suggest fixes

## Variants

- `/build` — debug build (default)
- `/build release` — `make build-release` (optimized)
- `/build wheel` — `make build-wheel` (Python extension)

## Success Criteria
- Exit code 0
- Zero errors
- Note any new warnings
