---
title: Getting started
description: Build gremlin and run the included example.
---

## What you need

Install the Rust toolchain named in `rust-toolchain.toml` and a system C linker. The project is a standard Cargo workspace and does not require a GPU, solver, or external service.

## Build and test

From the repository root, run:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Run the example

The included affine program is a small typed function over `u64` values. First, validate the source:

```sh
cargo run -p gremlin-cli -- check examples/affine.gremlin
```

Then execute it with one hexadecimal input:

```sh
cargo run -p gremlin-cli -- run examples/affine.gremlin --args 0x0000000000000005 --max-steps 256
```

The command returns structured JSON. A completed result includes the value and the number of interpreter steps used.

## Try a search

For a complete search run, use one of the checked-in fixture configurations:

```sh
cargo run --release -p gremlin-cli -- synthesize --config tests/fixtures/composed_u64.toml
```

Searches write their results under `runs/`. That directory is intentionally ignored by Git so you can experiment without changing the repository.
