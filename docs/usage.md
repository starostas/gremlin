# Getting started

Run Gremlin commands from the repository root. A first CPU-only run needs Rust 1.90.0 and a system linker.

1. **Build the CLI.**

       cargo build --release --locked -p gremlin-cli

2. **Check and run the included affine program.**

       target/release/gremlin check examples/affine.gremlin
       target/release/gremlin run examples/affine.gremlin --args 0x0000000000000005 --max-steps 256

   [Learn how program inputs, outputs, and limits work.](run-programs.md)

3. **Start a small synthesis search.**

       target/release/gremlin synthesize --config tests/fixtures/composed_u64.toml

   [Configure a search and choose how candidates are ranked.](synthesize.md)

4. **Inspect the generated run directory before building on the result.**

   Check the evidence level and report before treating a candidate as correct. You can also [resume a stopped search](results-and-resume.md) from its checkpoint.
