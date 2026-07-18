# EasyMill — Suggested Commands

## Build / Run / Test
- `cargo build` — compile
- `cargo run` — run GUI app
- `cargo test` — run all `#[cfg(test)]` tests (in `gerber.rs` and `conversion.rs`)

## Logging
- `RUST_LOG=easymill=debug cargo run` — debug-level logging
- Default: `easymill=info,warn`

## Standalone test binary
- `rustc test_gerber_direct.rs --crate-type bin` — standalone Gerber render test (not a cargo test)

## Note
Tests are inline `#[cfg(test)]` modules, not integration tests. No specific test runner config needed beyond `cargo test`.
