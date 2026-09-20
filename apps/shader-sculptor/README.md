# Shader Sculptor

A visual GPU search that reconstructs an image as a layered drawing program and exports an equivalent Gremlin pixel function. Use a supplied image or upload your own, watch the approximation emerge, replay its construction, and download the source.

This app deliberately uses a **specialized CUDA evaluator for a restricted graphics language**. It does not run the general Gremlin interpreter for every candidate. Exported Gremlin code implements the selected drawing and is checked against the result. It is bounded graphics-program search, not unconstrained shader or algorithm synthesis.

## Run

Python 3 alone runs the labeled recorded example and construction replay:

```sh
python3 apps/shader-sculptor/server.py
# Open http://127.0.0.1:8790
```

For live search, build on a CUDA machine:

```sh
cargo build --release --locked --features cuda --manifest-path apps/shader-sculptor/engine/Cargo.toml
python3 apps/shader-sculptor/server.py --gpu
```

Or use an existing GPU host:

```sh
python3 apps/shader-sculptor/server.py --host 0.0.0.0 \
  --ssh user@gpu-host --ssh-port 22 \
  --remote-engine /root/gremlin-validation/apps/shader-sculptor/engine/target/release/shader-sculptor
```

The browser has three presets and local image upload. Resolution and Layers are independent controls. Targets can be 128, 256, 512, 1,024, or 2,048 pixels per side; uploads are resized from the original image to the selected resolution before search. Presets are generated directly at that resolution. Download image saves a full-resolution PNG. The target pixels are sent to the configured engine when Discover drawing is clicked. Recorded playback restores the recorded orbital target. There are no frontend packages, external fonts, or image services.

## Why this is a GPU workload

Each round proposes 4,096 possible new drawing layers. The GPU tests each candidate within its conservative bounding rectangle at the selected resolution, skips pixels outside that rectangle because their error cannot change, reduces squared RGB-error deltas on the device, and returns one signed 64-bit score per candidate. The target, working canvas, candidate storage, and score buffers stay allocated for the complete search.

Only the winning layer is painted. Occasional previews are copied back; the candidate-by-pixel matrix is never materialized or serialized. Working device buffers use `width × height × 8 + 163840` bytes: 288 KiB at 128², approximately 32.16 MiB at 2048², excluding CUDA context and driver overhead. Host mirrors of the target and canvas support proposal generation and independent validation.

The recording and original measurements below predate bounding-rectangle scoring and use 128×128 pixels. The recorded refined run tests **2,097,152 proposed layers / 34,359,738,368 candidate-pixel pairs**. Its scoring kernels took approximately **881 ms** on an RTX A4000. The complete search took **1.30 s**, with **1.40 s** through final source validation. Its first accepted layer arrived at **0.22 s**, including GPU startup. These are actual measured timings, not construction-replay durations; they exclude SSH launch and browser latency.

The earlier 128-layer probe finished through validation in 0.45 s. The refined recording reduces squared color error by 99.6% from the initial mean-color canvas, with 3.3/255 RMS error per color channel. Error reduction is not the percentage of pixels that match exactly. More detailed or unsuitable input images can retain substantially more error.

Ultra mode completed all 2,048 layers on each of the three presets (seed 1) in 4.75–4.85 seconds including export validation on the RTX A4000. Each run scored 8,388,608 candidate layers across 137,438,953,472 candidate-pixel evaluations. GPU working buffers remain 288 KiB, excluding the CUDA context. The orbital image reached 0.65/255 RMS color error; exported source is approximately 1 MB.

With native-resolution search and bounding-rectangle scoring, seed 1 of the orbital preset completed 256² / 128 layers in 0.55 s, 512² / 512 layers in 1.97 s, 1024² / 128 layers in 1.80 s, and 2048² / 128 layers in 6.28 s, including export validation. The combined 2048² / 2,048-layer run completed in 59.42 s through export validation (67.98 s including browser transport and replay verification), with 3.46/255 RMS color error. `measurements-resolution.json` stores these results separately from the original 128² measurements. Browser transport adds time. All final pixels were checked against the native renderer and JavaScript construction replay.

At 512², the added 4,096- and 8,192-layer modes completed in 14.11 s and 28.44 s respectively, including export validation (orbital preset, seed 1, RTX A4000). Both full-resolution construction replays matched every output pixel, and both Gremlin downloads were checked.

There is no CPU speed comparison in this demo.

## The language and search

A program starts with a solid background and adds opaque or translucent ellipses, rectangles, and diamonds. Each layer chooses a center, two radii, axis-aligned or diagonal orientation, RGB color, and opacity. These primitives are ordinary integer geometry and compositing operations, not embedded target-image lookups.

Greedy search chooses the best improving proposal each round. Proposal centers are biased toward current image residuals; colors come from target observations with optional jitter. Primitive type, orientation, size, and opacity vary. Existing layers are retained, so this is not a global optimizer or a claim of finding the shortest program. The supplied target is pixels only; its generating code and shape list are not supplied to the search.

At 128×128, Sketch mode permits up to 128 rounds with a 1.5-second search budget. Refined mode permits up to 512 rounds with a 3-second budget. Ultra mode permits up to 2,048 rounds with a 12-second budget. The 4,096- and 8,192-layer options have base budgets of 24 and 48 seconds. For larger resolutions, those budgets scale with pixel count and are capped at 120 seconds for the original options, or 360 seconds for 4,096 and 8,192 layers. The layer limit is independent of resolution. Preview images are limited to 512 pixels per side; the final image and downloaded PNG contain every pixel. The wall-clock budget is checked between GPU batches, so one batch can cross the deadline. Final validation and transport occur afterward. A fixed seed determines the proposal stream; a deadline can change the number of completed rounds across machines.

Geometry uses the selected coordinate grid, with maximum radius half the image width. Ellipse arithmetic uses 64-bit integers to avoid overflow at larger resolutions. Alpha blending uses exact rounded integer arithmetic. The exported function `pixel(x:i64,y:i64)->i32` returns packed RGB for pixel coordinates from zero through width minus one. The older recording uses i32 coordinates. Its geometry and layer order reproduce the discovered drawing without loading the input image.

The scalar Gremlin source can be much larger than its compact shape list. The Gremlin source parser accepts up to 8 MB so that 8,192-layer exports remain executable; larger inputs are still rejected. The download is the executable Gremlin pixel function, not Rust source or a general-purpose GPU shader.

## Verification and measurements

For every accepted layer, the winning GPU score is checked against a separate native renderer. Preview and final GPU canvases are compared pixel-for-pixel with that renderer. The Gremlin export is parsed and executed at 261 sampled pixel positions, including the corners and center. This is sampled export validation, not a formal proof or an exhaustive exported-program render.

CUDA tests check all returned scores for 441 randomized proposals across 128², 512², and 2048² canvases and exact painting parity. Large-geometry export tests cover ellipse products that exceed 32-bit range. Maximum-detail exports of all three primitive types are parsed to check the source-size limit. Native tests check exported programs containing multiple randomized layers. Browser checks compare construction replay with all 16,384 recorded output pixels and exercise live GPU search, downloads, upload, and mobile layout.

`sample.json` contains the refined orbital run. `measurements.json` contains checks across presets, seeds, and detail levels. Timings include proposal generation, GPU initialization/transfers/scoring, reference checks, and preview emission. Total time adds source validation. The independent GPU tests and measurement runs are sequential.

```sh
cargo test --release --locked --manifest-path apps/shader-sculptor/engine/Cargo.toml
cargo clippy --locked --manifest-path apps/shader-sculptor/engine/Cargo.toml --all-targets -- -D warnings
python3 apps/shader-sculptor/test_server.py

# On a CUDA host:
cargo test --release --locked --features cuda --manifest-path apps/shader-sculptor/engine/Cargo.toml
# Engine reads {"target":[16384 packed RGB integers],"seed":1,"budget_ms":3000} on stdin.
apps/shader-sculptor/engine/target/release/shader-sculptor < request.json
```

One web job runs at a time. The HTTP server validates pixel count, channel range, seed, budget, and request size. Remote execution is capped at 180 seconds with a 200-second web watchdog for the original options, or 420 seconds with a 440-second watchdog for 4,096 and 8,192 layers. GPU errors are explicit; the engine does not silently substitute another backend. Screenshots from browser validation are in the ignored local `runs/shader-sculptor/` directory.
