# Release-Gated Tour Version Logic

## Problem

The application tour auto-starts by comparing `CARGO_PKG_VERSION` against a `seen-version` file. Since `Cargo.toml` is bumped manually, the tour never triggers between GitHub releases — it only changes incrementally during local development.

## Solution

Use a build-time environment variable (`EASYMILL_RELEASE`) injected by the GitHub Actions release workflow. The app resolves its version as:

```
VERSION = EASYMILL_RELEASE  if set (CI release build)
       || CARGO_PKG_VERSION fallback (local dev)
```

### Version resolution (src/main.rs)

- `VERSION` constant: `option_env!("EASYMILL_RELEASE").unwrap_or(env!("CARGO_PKG_VERSION"))`
- The `seen-version` file stores whatever `VERSION` resolves to
- `is_new_version()` compares stored vs. current — unchanged logic

| Build context | `EASYMILL_RELEASE` | `VERSION` |
|---|---|---|
| GA release (`v0.2.0` tag) | `v0.2.0` | `v0.2.0` |
| Local `cargo build` | not set | `0.1.0` |

### Release workflow (.github/workflows/release.yml)

Trigger: `push` on tags matching `v*`. Pipeline:

1. Checkout repository
2. Install Rust toolchain
3. `cargo build --release` with `EASYMILL_RELEASE=${{ github.ref_name }}`
4. Create GitHub Release
5. Upload `target/release/easymill` as release asset

The `github.ref_name` is the full tag string (e.g., `v0.2.0`), which gets compiled into the binary verbatim.

### What stays the same

- `seen-version` file path: `dirs::data_dir()/easymill/seen-version`
- `is_new_version()` / `mark_version_seen()` functions
- Tour steps, tour manager, overlay rendering
- `is_new_version()` is only checked in `AppState::default()` on startup

### Files changed

| File | Change |
|---|---|
| `src/main.rs` | 1 line (VERSION const) |
| `.github/workflows/release.yml` | New file (~35 lines) |
