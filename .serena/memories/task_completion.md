# EasyMill — Task Completion

Before claiming work is complete:

1. Verify compilation: `cargo build`
2. Run tests: `cargo test`
3. No formatter/linter commands are specified — skip unless user explicitly asks
4. No type-checker separate from cargo build

## Known Failure Mode
- Project currently **does not compile** due to missing `src/ui/mod.rs`
- Any task involving UI code must create `src/ui/mod.rs` with `pub mod widgets; pub mod palette; pub mod styles;` and implement `palette` and `styles` modules
