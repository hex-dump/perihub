<!-- Thanks! The checks below are also run by CI. -->

## What this changes

<!-- What PeriHub does differently, and why. Link the issue: "Fixes #123". -->

## Tested on

<!-- Device(s) and system, or "no device needed". -->

## Checklist

- [ ] `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test --workspace` and `npm run check` pass
- [ ] A line in `CHANGELOG.md` under `## Unreleased` (or this changes nothing users see)
- [ ] Device tables changed: `cargo xtask gen` was run
- [ ] New protocol facts: described in `docs/protocols/` with their sources; no code copied from GPL projects, nothing from decompiled software
