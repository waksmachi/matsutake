# Project structure

This document describes the structure of the `matsutake` repository. 

## 1. Layout

```
matsutake/
  Cargo.toml                  the workspace root
  Cargo.lock
  rust-toolchain.toml         the Rust version of the workspace
  .fvmrc                      the Flutter version of the app, for fvm
  .gitignore
  .github/workflows/ci.yml    the GitHub Actions workflow
  .vscode/settings.json       VS Code settings for rust-analyzer and the Flutter SDK
  crates/
    jpdag/                    core library: normalization, the schema, and the dictionary
    jpdag-ffi/                bridge that exposes jpdag to Dart
    ingest/                   binary that writes content.db
  app/                        Flutter project
    pubspec.yaml
    flutter_rust_bridge.yaml  bridge codegen settings
    rust_builder/             Cargokit plugin that compiles jpdag-ffi
    lib/                      Dart code of the app
    lib/src/rust/             generated Dart bindings
    assets/content.db         output of ingest, which git ignores
    assets/fonts/             fonts of the app, and their licences
    tool/                     script that makes the fallback font
  Docs/
    ingest-design.md          how ingest builds content.db
    jpdag-dictionary-design.md  the dictionary component of jpdag
    app-design.md             the Flutter app and the bridge to jpdag
    future-work.md            the work that the designs leave out
```
