# EasyMill

A desktop application for milling PCBs from Gerber files. Parses Gerber RS-274X and Excellon drill files, renders them to raster images, and generates G-code toolpaths for CNC fabrication.

## Pipeline

```
Gerber files (.gbr/.gbl/.gtl/.drl)
  → Tessellated geometry (lyon)
    → 3 aligned PNG layers (traces, drills, outline)
      → Distance transform & contour vectorization
        → G-code (Fanuc-style)
```

## Features

- **Gerber RS-274X parser** — apertures, macros, step-and-repeat, region mode, circular interpolation
- **Excellon drill parser** — M48 format with all common header codes
- **3-layer PNG rendering** — copper traces, drill hits, board outline at configurable DPI
- **G-code generation** — toolpath offsetting, area clearance, ramped entry moves, feed/speed control
- **Native GUI** — built with Iced, file dialogs, live preview
- **Estimates** — cut distance, estimated time, toolpath length

## Getting Started

```bash
# Build and run
cargo run

# Run tests
cargo test
```

## Usage

1. Open Gerber files
2. Configure tool diameter, feed rate, plunge rate, and DPI
3. Run the pipeline — the app renders PNG layers then generates G-code
4. Save the G-code for your CNC machine

## Dependencies

- **iced** 0.14 — GUI framework
- **gerber_parser** / **gerber-types** — Gerber file format
- **lyon** — path tessellation
- **image** — PNG rasterization
- **nalgebra** — linear algebra
- **zip** — compressed input handling
- **rfd** — native file dialogs
- **tokio** — async runtime

## Project Structure

```
src/
├── main.rs          — Iced application entry point
├── lib.rs           — Library root
├── gerber.rs        — Gerber/Excellon parser + rasterizer
├── conversion.rs    — Pipeline (Gerber→PNG→G-code)
├── logging.rs       — Tracing initialization
└── ui/
    └── widgets.rs   — Custom Iced widgets
```

## License

MIT © 2026 Yassin Diab
