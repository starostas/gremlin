# Shader Detective

A local browser application that learns a hidden packed-color transform, shows its evolving image and recovered Gremlin program, and compares CPU with GPU on the same deterministic search.

## Quick start: replay or CPU

Python 3 is sufficient for the recorded demo:

```sh
python3 apps/shader-detective/server.py
# Open http://127.0.0.1:8787 and choose “Play recorded GPU demo”.
```

The recording is labeled as playback throughout. Its numbers are actual measurements from an RTX A4000 / Ryzen 7 5700 comparison, not browser animation timings. Without a built engine, the live-run button is disabled.

To enable live CPU runs:

```sh
cargo build --release --locked --manifest-path apps/shader-detective/engine/Cargo.toml
python3 apps/shader-detective/server.py
```

To enable live GPU runs on a machine with NVCC and an NVIDIA GPU:

```sh
cargo build --release --locked --features cuda --manifest-path apps/shader-detective/engine/Cargo.toml
python3 apps/shader-detective/server.py --gpu
```

The Rust application is a small independent Cargo workspace using the existing Gremlin crates. It does not change the repository's default CPU build. There are no JavaScript packages, frontend build step, external fonts, or runtime web services.

## Supplied remote GPU

The app engine has also been built on the supplied GPU machine. Run the browser server locally and have it launch the engine over SSH:

```sh
python3 apps/shader-detective/server.py \
  --ssh user@gpu-host \
  --ssh-port 22
```

The default remote engine is `/root/gremlin-validation/apps/shader-detective/engine/target/release/shader-detective`. Override it with `--remote-engine` if needed. Existing SSH authentication is used; no credentials are embedded in the application. The server defaults to loopback. Add `--host 0.0.0.0` to listen on all network interfaces, then open `http://<server-address>:8787` from another machine. Alternatively, forward port 8787 over SSH. Another port can be selected with `--port`.

## What the demo does

- **Aurora:** learns a two-operation rotate/XOR transform.
- **Afterglow:** learns a three-operation rotate/XOR/add transform.
- Inputs and outputs are `u32` values; the canvas displays their low 24 RGB bits. Training and validation compare the entire 32-bit result.
- The search receives only labeled observations, an operator pool (`rotl`, `xor`, `add`) and the hints `8`, `0x0055aa33`, `0x00102030`. The arrangement is not supplied as an initial candidate.
- Population 256, at most 12 instructions, 64 execution steps and 80 generations. Generic depth-three chain enumeration contributes 192 proposals per generation; the remaining children come from mutation. Both target expressions lie in this explicitly bounded search space.
- Choose 8,192 or 32,768 training colors and seed 1, 2 or 3. After discovery, the CPU interpreter checks 4,096 disjoint holdout colors and the complete image preview. Results are **tested**, not formally proved for every input.
- “Compare CPU + GPU” runs the same corpus and seed sequentially on the same machine and compares the complete search-state hashes. The two searches perform the same work.

The visual shader is deliberately short; the workload comes from hundreds of millions of independent candidate/color observations. It is complex enough to show real discovery without asking the search to invent an unconstrained algorithm or reconstruct a full graphics application.

## Why the GPU helps here

The app embeds `gremlin-cuda` and retains its CUDA context for the duration of one run. Search uses the compact fitness interface, so it does not serialize a per-input outcome matrix between processes. It is a separate integration from the general CLI's fresh worker per batch. GPU allocations and results are still rebuilt for each batch; this is not a claim of persistent device-side corpus storage or a kernel-side reduction.

Each GPU batch has a 2 GiB device allocation budget. The example uses only total straight-line operators, caps semantic steps, and fails explicitly on missing CUDA/device/memory errors. It does not silently fall back to CPU. The browser permits one run at a time, exposes Stop, and limits jobs to 190 seconds locally; remote commands additionally use a 180-second timeout. These limits also bound an unhealthy live demo.

## Measured result

Afterglow, seed 1, 32,768 training colors, release build, Ryzen 7 5700 single-thread CPU versus RTX A4000:

| Measurement | CPU | GPU |
| --- | ---: | ---: |
| Search time | 32.30 s | 11.61 s |
| Generations | 72 | 72 |
| Candidate/color evaluations | 585,367,552 | 585,367,552 |
| Fresh-color mismatches | 0 / 4,096 | 0 / 4,096 |
| Image preview | Exact match | Exact match |

GPU speedup: **2.78×**. Complete search-state hashes match. Maximum reported device allocation: **653,598,048 bytes** (about 623 MiB). Search timing includes initialization, generations and preview/progress emission; corpus construction and final independent validation are outside the timed interval. This is a measured workload, not a promise for every GPU or program.

Both presets also passed seeds 1–3 on the GPU with 8,192 training colors. The compact results are in [measurements.json](measurements.json). The browser's [sample.json](sample.json) preserves the full recorded generation stream with deduplicated RGB image frames.

## Engine and validation commands

```sh
# Emits JSON events; useful for reproducible runs without a browser.
apps/shader-detective/engine/target/release/shader-detective both afterglow 32768 1

cargo clippy --locked --manifest-path apps/shader-detective/engine/Cargo.toml --all-targets -- -D warnings
python3 apps/shader-detective/test_server.py
```

A real headless Chromium run also exercised recorded playback, live GPU synthesis, exact canvas equality, cancellation, and a 390-pixel mobile viewport with no JavaScript errors or horizontal overflow. Screenshots from that check are in the ignored local `runs/shader-detective/` directory.
