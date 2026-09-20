# Orbit Forge

Evolve a bounded numerical algorithm for **Kepler’s equation**, `E − e sin(E) = M`, then watch its orbital positions, explore an error map, and download the actual Gremlin program.

This uses the **general Gremlin CUDA interpreter**. There is no specialized CUDA orbital solver and no polynomial coefficient search. The supplied fixed-point sine/cosine routine has fixed coefficients. Search mutates the algorithm around it: initial guess, Newton/Halley/damped-Newton/fixed-point update schedule, loop bound, early stopping threshold, optional bracket fallback, and step cap. These are supplied numerical building blocks, not an assertion that Gremlin invented Newton’s method or unrestricted numerical algorithms.

## Run

```sh
cargo build --release --locked --features cuda --manifest-path apps/orbit-forge/engine/Cargo.toml
python3 apps/orbit-forge/server.py --gpu
# http://127.0.0.1:8791
```

Or run the Python server with `--host 0.0.0.0 --ssh user@gpu-host --ssh-port PORT --remote-engine /path/to/orbit-forge`. The local server uses `/usr/bin/clang-18` for optional native compilation/benchmarking. GPU execution is remote; native runtime measurements are on the web-server CPU, whose model is recorded. Without a live engine, the server provides an explicitly labeled recorded result. No frontend packages or external assets are required.

## Search and accuracy

64 candidate programs compete over 32 generations. Eight elites survive; offspring mutate one to three choices and occasional fresh programs enter. The initial population is randomly generated. Each program has a bounded loop and branches, and runs against an initial 512 orbit conditions. Ranking minimizes failures against **one tenth of** the requested error tolerance, then VM instruction count among passing candidates. This margin helps generalization; it is not a proof.

Every fourth generation, the current winner is challenged on a 256×256 grid. Up to 32 failures feed back into the next generation. Every completed GPU result is compared against an independently written integer evaluator. CPU interpreter comparisons also cover boundary cases. The final winner is checked against a double-precision bisection reference on all 65,536 grid points and 4,096 disjoint seeded holdout points, then replayed through the GPU. The browser’s BigInt implementation is checked against the complete grid-output hash.

The domain is `0 ≤ M ≤ π`, `0 ≤ e ≤ 0.95`. Inputs `m,e` and output `E` are signed 64-bit integers scaled by `2^28`. The UI mirrors the other half-period for animation. Error is absolute angular error in radians at the actual fixed-point input values. Results cover the checked inputs, not every representable input or the continuous domain. Validation failures remain visible; no approximate result is labeled formally verified.

The downloadable program contains all the selected control flow and fixed-point arithmetic. It does not load an answer table.

## Native performance

The winner is lowered by `gremlin-codegen::lower` and compiled from actual LLVM IR with Clang 18 `-O3`, without fast-math. Its entire grid-output hash must match the Gremlin interpreter before timings can be reported.

The baseline is the **faster of safeguarded double-precision Newton and Halley implementations**, both stopping at the requested error tolerance. Baselines are independently checked against bisection on the grid. Every generated benchmark input is also accuracy-checked for the candidate before timing. Two batch sizes (16,384 and 262,144) and three distributions (whole domain, low eccentricity, high eccentricity near periapsis) receive nine alternating-order trials each. Function-call and integer input/output conversion costs are included for both implementations. Inputs are generated before the timer and outputs are accumulated in a consumed checksum.

A speedup badge is shown only when every timed comparison wins and all six groups’ median ratios exceed 1.05. The displayed ratio is the smallest group median ratio. Otherwise the speedup is omitted. Complete measurements remain in the recorded result for audit. This is specific scalar throughput on the recorded CPU versus these implementations, not a claim against the fastest published Kepler algorithm, SIMD libraries, every CPU, or every input distribution. Compilation, GPU search, browser transport, and rendering are excluded from native runtime timings.

The GPU’s role is accelerating **candidate program evaluation**, separately from the discovered program’s native speed. A same-host check of 32 distinct random population batches produced identical results for 1,048,576 executions: 5.85 s in the single-thread Gremlin CPU interpreter and 0.525 s through CUDA, including cold GPU setup and result transfer. Program construction was excluded from both; this is not total application speed. The GPU was an RTX A4000. A single cold population did not amortize GPU startup and was slower; the repeated-population workload does.

## Recorded results

The recorded 10⁻⁴-radian run finished GPU search plus numerical validation in 1.64 s. Its worst error on the grid and holdout was 4.39e-06 radians. Its compiled winner also passed accuracy checks on all 835,584 benchmark inputs. No consistent native speedup was measured, so the UI omits that badge. These native timings are separate from GPU search.

`sample.json` stores the recorded program, error map, search history, and raw native trials. `measurements.json` stores all nine seed/tolerance checks plus the backend comparison. All nine runs passed final validation in 1.51–1.68 s; run-to-run timing varies.

## Checks

```sh
cargo test --release --locked --manifest-path apps/orbit-forge/engine/Cargo.toml
cargo clippy --release --locked --manifest-path apps/orbit-forge/engine/Cargo.toml --all-targets -- -D warnings
python3 apps/orbit-forge/test_server.py
# Same-program backend experiment, on the CUDA host:
cargo test --release --features cuda --manifest-path apps/orbit-forge/engine/Cargo.toml -- --ignored --nocapture
```

Source/native tests cover 5,120 random program/input combinations and all supported holdout seeds. HTTP tests cover input validation, unavailable-engine replay, origin checks, and cleanup of an SSH-style process that lingers after its final event. Browser checks cover all 65,536 replay outputs, a live GPU search, compiled parity, source download, and mobile layout. Screenshots are under the ignored `runs/orbit-forge/` directory.

The server accepts one job at a time, bounds request size, limits the remote engine to 180 seconds, and uses a 240-second job watchdog. Native compile/benchmark subprocesses have separate timeouts. GPU errors are explicit; there is no silent CPU search fallback.

## Why this use case

Kepler inversion appears in repeatedly evaluating orbital positions. Unlike fitting a scalar polynomial, candidates must make numerical-control decisions whose failures concentrate in difficult parts of a two-dimensional domain. Other natural extensions are Lambert-W inversion, adaptive quadrature stopping policies, and ray/intersection root solvers; they are ideas, not implemented demos here.

References: [JPL Kepler equation documentation](https://naif.jpl.nasa.gov/pub/naif/toolkit_docs/FORTRAN/spicelib/kepleq.html) and [numerical Kepler solver research](https://www.aanda.org/articles/aa/pdf/2022/02/aa41423-21.pdf). Expert algorithms are context, not measured competitors in this demo.
