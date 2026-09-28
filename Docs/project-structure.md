# Project structure

This document describes the structure of the `matsutake` repository. 

## 1. Layout

```
matsutake/
  Cargo.toml                  the workspace root
  Cargo.lock
  rust-toolchain.toml         the Rust version of the workspace
  .gitignore
  crates/
    jpdag/                    the core library
    ingest/                   the binary that writes content.db
  app/                        the Flutter project
  Docs/
```