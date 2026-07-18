# Release-Gated Tour Version Logic Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Gate the application tour on GitHub release versions rather than Cargo.toml version.

**Architecture:** Use `option_env!("EASYMILL_RELEASE")` in the binary — set by CI release builds from `github.ref_name`. Local dev builds fall back to `CARGO_PKG_VERSION`. A new GitHub Actions workflow builds releases on tag push for both Linux and Windows via a build matrix.

**Tech Stack:** Rust, GitHub Actions, iced_tour

---

### Task 1: Update VERSION constant in main.rs

**Files:**
- Modify: `src/main.rs:21`

- [ ] **Step 1: Change VERSION to use build-time env var with fallback**

Change line 21 from:
```rust
const VERSION: &str = env!("CARGO_PKG_VERSION");
```
to:
```rust
const VERSION: &str = option_env!("EASYMILL_RELEASE").unwrap_or(env!("CARGO_PKG_VERSION"));
```

- [ ] **Step 2: Build and verify**

Run: `cargo build`
Expected: Compiles successfully. No warnings.

- [ ] **Step 3: Verify release mode works**

Run: `EASYMILL_RELEASE=v0.2.0 cargo build 2>&1`
Expected: Compiles with no errors. The binary will have VERSION=v0.2.0.

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: resolve VERSION from EASYMILL_RELEASE env var with Cargo.toml fallback"
```

---

### Task 2: Create GitHub Actions release workflow

**Files:**
- Create: `.github/workflows/release.yml`

- [ ] **Step 1: Create workflow file**

```yaml
name: Release

on:
  push:
    tags:
      - 'v*'

env:
  EASYMILL_RELEASE: ${{ github.ref_name }}

jobs:
  release:
    name: Build and Release
    strategy:
      matrix:
        os: [ubuntu-latest, windows-latest]
    runs-on: ${{ matrix.os }}

    steps:
      - uses: actions/checkout@v4

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable

      - name: Build release binary
        run: cargo build --release

      - name: Upload artifact
        uses: actions/upload-artifact@v4
        with:
          name: easymill-${{ matrix.os }}
          path: |
            target/release/easymill
            target/release/easymill.exe

  create-release:
    name: Create Release
    needs: release
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Download all artifacts
        uses: actions/download-artifact@v4

      - name: Create Release
        uses: softprops/action-gh-release@v2
        with:
          files: |
            easymill-ubuntu-latest/easymill
            easymill-windows-latest/easymill.exe
          generate_release_notes: true
```

- [ ] **Step 2: Verify file syntax**

Run: `cat .github/workflows/release.yml`
Expected: Valid YAML, path exists.

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci: add multi-platform release workflow (linux + windows)"
```
