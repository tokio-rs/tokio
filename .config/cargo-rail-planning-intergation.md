# cargo-rail Planning Integration

## Intent
This repo uses `cargo rail run` directly (standalone mode).
The `.config/rail.toml` is tuned conservatively for Tokio's matrix-heavy CI.

## Local developer flow
```bash
cargo rail config validate --strict
cargo rail plan --merge-base --explain
cargo rail run --merge-base --profile ci
```

## GitHub Actions integration (cargo-rail-action)
Use `loadingalias/cargo-rail-action@v3` as the planner transport.

```yaml
- uses: loadingalias/cargo-rail-action@v3
  id: rail

- name: Build + Test (targeted)
  if: steps.rail.outputs.build == 'true' || steps.rail.outputs.test == 'true'
  run: cargo rail run --since "${{ steps.rail.outputs.base-ref }}" --profile ci

- name: Docs lane
  if: steps.rail.outputs.docs == 'true'
  run: cargo rail run --since "${{ steps.rail.outputs.base-ref }}" --surface docs
```

## UI output that teams should read
- `steps.rail.outputs.plan-json` for machine parsing
- action summary table (surface status + reasons)
- `cargo rail plan --explain` locally to match CI decisions

## Measured impact (last 20 commits)
- Could skip build: 10%
- Could skip tests: 0%
- Targeted (not full run): 95%
