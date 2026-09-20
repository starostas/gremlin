# Landing Lab

Search for a two-axis spacecraft controller, then watch 48 representative flights land (or crash). Each candidate executes a full bounded Gremlin program with a physics loop, delayed thrusters, wind, finite fuel, and touchdown checks.

On the supplied RTX A4000, three repeated comparisons measured **71.7× faster search** than Gremlin's single-thread CPU interpreter: median **78.71 s CPU → 1.10 s GPU**. Including program construction and independent validation, the median speedup was **49.8×**. These are same-host, same-release-executable results, not a comparison against optimized native physics or all CPU cores.

## Run

Python 3 alone is enough for the labeled recorded benchmark:

```sh
python3 apps/landing-lab/server.py
# Open http://127.0.0.1:8788 and choose Replay benchmark.
```

For local CPU execution:

```sh
cargo build --release --locked --manifest-path apps/landing-lab/engine/Cargo.toml
python3 apps/landing-lab/server.py
```

On a CUDA machine, build with `--features cuda` and start the server with `--gpu`. Or use an engine built on an SSH-accessible GPU host:

```sh
python3 apps/landing-lab/server.py --host 0.0.0.0 \
  --ssh user@gpu-host --ssh-port 22 \
  --remote-engine /root/gremlin-validation/apps/landing-lab/engine/target/release/landing-lab
```

Open `http://<server-address>:8788`. The server accepts one job at a time and provides Stop. Remote engine execution is capped at 180 seconds, with a 190-second local watchdog. An explicit final engine event releases an SSH transport that remains open after completion. The standard-library server has no JavaScript packages, build step, or external frontend services.

The **Compare CPU + GPU** option takes about 80 seconds on the measured host. **GPU search** normally takes a few seconds including SSH transport; replay animates the saved comparison without rerunning it. Seed and scenario count are selectable.

## What is actually searched

The physics loop is supplied; Gremlin evaluates an explicitly bounded grammar of **128 distinct controller programs**:

- Eight braking expressions: two linear laws, three velocity-squared laws, and three laws with a velocity lookahead.
- Four braking margins: 0, 64, 256, 768.
- Four steering strategies: position-only, velocity-only, and two position/velocity feedback laws.

All combinations are evaluated. Fitness first maximizes safe landings, then fuel remaining across those successful landings, with stable enumeration order as the final tie-break. The winning controller is not planted as an initial solution. This is exhaustive controller synthesis within a small grammar, not unconstrained invention of the simulation or an evolutionary search.

Each program contains a loop of at most **256 ticks**. A tick is 0.1 simulated seconds; positions use centimeters and velocities use centimeters per tick. The simplified plant has downward acceleration 2, vertical thrust 8, lateral thrust ±2, constant lateral wind −1/0/+1, and a one-tick actuator delay. It starts with 224 fuel units and consumes one per tick commanding either thruster. A landing succeeds only if it reaches the surface within 48 cm of the pad, with lateral speed at most 0.6 m/s and vertical speed at most 1.2 m/s.

The initial conditions vary position, altitude, and both velocity components. Wind is deterministically derived from starting position. The benchmark uses 8,192 unique scenarios per controller; the smaller option uses 2,048. Every execution has a 40,000-instruction budget. Batches contain 16 programs, with a 2 GiB allocation ceiling; the measured peak is **238,388,608 bytes / 227 MiB**. Exhaustion and infrastructure errors are explicit.

A separate native Rust simulator checks all 128 candidate programs on eight scenarios before timing. After the search, the winner is checked through both simulators on **4,096 disjoint holdout scenarios**. Animation uses the native simulator with that winning controller. The code pane exposes the full executable Gremlin source as well as the shorter control-law description. All seeds 1–3 passed every training and holdout flight in the measured 8,192-scenario runs. This is empirical evidence in toy physics, not real flight qualification or universal proof.

## Measurements

The CPU and GPU each run the same 128 programs against the same 8,192 scenarios. For seed 1 that is **1,048,576 complete flight simulations and 8,158,772,480 interpreted instructions** per backend.

| Three-run median, seed 1 | CPU | GPU |
| --- | ---: | ---: |
| Search wall time | 78.71 s | 1.098 s |
| Safe training flights, winner | 8,192 / 8,192 | 8,192 / 8,192 |
| Safe holdout flights, winner | 4,096 / 4,096 | 4,096 / 4,096 |

Hardware: AMD Ryzen 7 5700 and NVIDIA RTX A4000 16 GB. The CPU baseline is **one Gremlin interpreter thread**. The GPU uses the same general Gremlin CUDA interpreter; there is no specialized lander kernel. Results and exact step counts for every candidate/scenario are hashed in order and match between backends in all three runs. Successful runs alone are not used as a proxy for parity.

Search timing includes CUDA context initialization on its first batch, allocation, transfers, evaluation, scoring, result hashing, and preview events. Both backends construct identical candidates and check the independent simulator before timing. Total timing adds this setup and final holdout validation. Neither timing includes process startup, browser rendering, or SSH transport. GPU and CPU runs are sequential, with no concurrent benchmark jobs. No GPU warmup, artificial delay, or instruction padding is used.

The GPU context persists within the engine process; matrices are returned directly to Rust for scoring. No per-case data crosses a worker subprocess boundary. `measurements.json` contains all three full comparison results and additional validation metadata. `sample.json` stores the first measured comparison with deduplicated flight traces, clearly labeled as playback. Small differences between the recording's speedup and the median are expected.

## Checks

```sh
cargo test --release --locked --manifest-path apps/landing-lab/engine/Cargo.toml
cargo clippy --locked --manifest-path apps/landing-lab/engine/Cargo.toml --all-targets -- -D warnings
python3 apps/landing-lab/test_server.py

# On the GPU host:
cargo test --release --locked -p gremlin-cuda --features cuda --test parity
apps/landing-lab/engine/target/release/landing-lab both lander 8192 1
```

The updated CUDA interpreter uses register-major storage, 128-thread tiles, power-of-two shift masking, and 32-bit division for narrow types while retaining a 64-bit path. The 10,000-program parity gate covers all integer widths/operators, control-flow divergence, traps and timeouts. Additional tests exercise loop edge scratch storage and partial tiles around 128 and 256 cases.

Browser checks exercise recorded playback, live GPU execution, final-event cleanup, and a 390-pixel mobile viewport. Screenshots are in the ignored local `runs/landing-lab/` directory.
