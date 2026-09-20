# Gremlin

Gremlin searches for small integer programs that match a function’s inputs and outputs. It evolves candidates, tests them against examples, and saves the best program as Gremlin source. Candidates can also be evaluated on CUDA, checked with Z3, or compiled through LLVM.

## Use

Install Rust 1.90.0 and a system linker, then build:

```sh
cargo build --release --locked -p gremlin-cli
```

Run a program or search for one:

```sh
target/release/gremlin run examples/affine.gremlin --args 0x0000000000000005
target/release/gremlin synthesize --config tests/fixtures/composed_u64.toml
```

Search results go into `runs/`: generated source, a JSON report, and a resumable checkpoint. Gremlin source looks like Rust but is a separate language. Functions accept up to four integer arguments and return one integer.

The default build runs on CPU. Binary targets require Linux x86-64 and Bubblewrap; CUDA, Z3, and LLVM are optional. See the [usage guide](docs/usage.md) for configuration, dependencies, CPU limits, and resuming a search.

## Demos

| Demo | What it does |
| --- | --- |
| [Shader Detective](apps/shader-detective/README.md) | Recover a hidden color transform. |
| [Shader Sculptor](apps/shader-sculptor/README.md) | Reconstruct an image as a drawing program. |
| [Tiny Robot](apps/tiny-robot/README.md) | Evolve a controller, then test it in a maze you draw. |
| [Landing Lab](apps/landing-lab/README.md) | Search for a spacecraft landing controller. |
| [Orbit Forge](apps/orbit-forge/README.md) | Evolve a numerical solver and visualize its orbits. |
| [CRC experiment](examples/cksum-crc/README.md) | Try synthesizing part of `cksum`; includes failed searches and a working component. |

Each browser demo documents its setup and recorded playback mode.

## Documentation

[Usage](docs/usage.md) · [Custom scoring](docs/comparators.md) · [Language](docs/semantics.md) · [Supported features](docs/support.md) · [Development](docs/development.md) · [Test results](docs/validation.md)
