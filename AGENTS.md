# EasyMill — Agent Instructions

## Commands
- `cargo build` / `cargo run` / `cargo test`
- `RUST_LOG=easymill=debug` (default `easymill=info,warn`)
- No CI, no pre-commit hooks, no formatter/clippy config — only `cargo build/test`
- No dedicated test runner; tests are inline `#[cfg(test)]` in `gerber.rs` and `conversion.rs`

## Standalone Binary
- `test_gerber_direct.rs` at repo root is a `fn main()` binary, NOT a cargo test
- Written against an **older API** — calls `easymill::gerber_inputs_to_png` with 3 args, but current signature takes 7
- Needs manual `rustc` compilation (and won't compile until the crate does)

## Architecture

| Path | Role |
|------|------|
| `src/main.rs` | Iced 0.14 app, `mod ui` (broken — see below) |
| `src/lib.rs` | Re-exports `PngLayerResults` from `conversion` |
| `src/gerber.rs` | Gerber RS-274X + Excellon parser, lyon tessellation, rasterizer (1560 lines) |
| `src/conversion.rs` | Full pipeline: Gerber→3 PNGs → distance transform → contour vectorization → offset toolpaths → G-code (1404 lines) |
| `src/logging.rs` | Tracing init via `OnceLock` (idempotent) |
| `src/ui/widgets.rs` | All UI widgets; imports `super::palette` / `super::styles` |

## Pipeline
```
.gbr/.gtl/.gbl/.drl → lyon tessellation → 3 aligned PNGs (traces/drills/outline)
  → distance transform → contour vectorization → offset toolpaths → G-code (Fanuc-style)
```

## Key Defaults (conversion.rs)
1000 DPI, 0.4mm tool, 300mm/min feed, 120mm/min plunge, 12000 RPM, 25M pixel cap.

## State Machine
`StepState`: `Idle → Ready → Running → Complete`. Progress via `Arc<AtomicU32>` + 100ms poll.

## Known Issues
- `test_gerber_direct.rs` uses outdated API — won't compile with current `conversion.rs`

## Non-obvious
- `repos/` is gitignored vendored JS repos (unrelated to Rust app)
- `docs/` only has agent skill config, not project docs
- `lib.rs` re-exports only `PngLayerResults`; actual pipeline entry points live in `easymill::conversion`
- Two main public pipeline functions:
  `gerber_inputs_to_png(copper, outline, drill, output_dir, stem, settings, on_progress) -> PngLayerResults`
  `png_to_gcode(png_path, settings, on_progress) -> GcodeResult`
