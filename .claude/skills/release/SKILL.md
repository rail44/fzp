---
name: release
description: |
  This skill should be used when the user asks to "release", "publish",
  "bump version", "cargo publish", "create a new version", or wants to
  publish fzp to crates.io. Handles the full release workflow: version
  bump, commit, push, and cargo publish.
---

# fzp Release

Publish a new version of fzp to crates.io with synchronized version numbers.

## Workflow

1. Determine the new version (ask the user if not specified)
2. Update version in both `Cargo.toml` and `.claude-plugin/plugin.json`
3. Run `cargo build` to verify
4. Commit version bump with `Cargo.lock`, push to remote
5. Run `cargo publish` — if Cargo.lock is dirtied by index update, commit and retry
6. Report the published version

## Version files

Both files must have matching versions:

| File | Field |
|------|-------|
| `Cargo.toml` | `version = "X.Y.Z"` |
| `.claude-plugin/plugin.json` | `"version": "X.Y.Z"` |

## Version bump guidelines

| Change type | Bump |
|-------------|------|
| Breaking change (config format, CLI flags, env vars) | minor (0.x → 0.x+1) |
| New feature, backward compatible | patch (0.x.y → 0.x.y+1) |
| Bug fix, docs, internal cleanup | patch |

## Cargo.lock handling

`cargo publish` updates the crates.io index which may dirty `Cargo.lock` after the
initial commit. If publish fails with "files in the working directory contain changes",
commit `Cargo.lock`, push, and retry `cargo publish`.

## Checklist before publishing

- All tests pass (`cargo test`)
- README.md reflects current behavior
- SKILL.md files are up to date
- No uncommitted changes beyond the version bump
