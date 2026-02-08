---
description: Parallel agent team implementation with adversarial review
---

# /team-implement — Parallel Implementation with Adversarial Review

Spawn implementation team with separate implementers and critics.

## Steps

1. Read the approved plan from `quality_reports/plans/`
2. Partition work by crate (file ownership)
3. Spawn team:
   - Implementers: one per independent crate/module
   - Critic: reviews each implementer's work independently
4. Implementers work in parallel on their assigned files
5. Critic reviews each implementer's output
6. Loop: implementer fixes → critic re-reviews (max 5 rounds)
7. Lead runs `make dev` on combined result
8. Present final summary

## Arguments

- `/team-implement` — use most recent approved plan
- `/team-implement quality_reports/plans/2026-02-08_feature.md` — specific plan

## File Ownership Rules

- Each teammate owns specific crate(s): `Teammate A owns crates/ha-config/`
- Root config files (Cargo.toml, Makefile): only lead modifies
- No shared file edits between teammates

## Requirements

- Agent teams must be enabled
- A plan must exist and be approved
