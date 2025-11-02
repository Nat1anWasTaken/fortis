# Claude Code Instructions

## Documentation Research
- Use `docs.rs` directly for Rust library documentation instead of trying to explore the codebase or use agents
- Go to https://docs.rs/[crate]/[version] for quick, accurate API reference

## Code Quality
- Always run `cargo check` to verify compilation before ending work on Rust code
- Do not consider a task complete without running `cargo check` successfully

## Module Organization
- Prefer modern module definitions over legacy `mod.rs` files
- Modern approach: Create a `.rs` file with the same name as the module in the parent directory
  - Example: For a `utils` module, create `utils.rs` in the parent directory instead of `utils/mod.rs`
  - This keeps the module structure flat and easier to navigate
- Avoid creating `mod.rs` files in subdirectories; use the modern approach instead
