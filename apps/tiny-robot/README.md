# Tiny Robot

A GPU program-search demo with an editable maze. A small, initially random controller learns to find a key, pass a locked door, and reach an exit using only three local wall sensors and one bit of controller memory. Inspect every decision, draw a new room, and see whether the discovered program escapes.

The UI contains no CPU performance comparison. It emphasizes the learned behavior, generalization, and failure cases.

## Run

Python 3 alone provides recorded discovery, the saved brain, fresh maze generation, and the room editor:

```sh
python3 apps/tiny-robot/server.py
# Open http://127.0.0.1:8789
```

For live training on a CUDA machine:

```sh
cargo build --release --locked --features cuda --manifest-path apps/tiny-robot/engine/Cargo.toml
python3 apps/tiny-robot/server.py --gpu
```

Or use a GPU engine over existing SSH authentication:

```sh
python3 apps/tiny-robot/server.py --host 0.0.0.0 \
  --ssh user@gpu-host --ssh-port 22 \
  --remote-engine /root/gremlin-validation/apps/tiny-robot/engine/target/release/tiny-robot
# Open http://<server-address>:8789
```

Choose **Evolve a brain** for a fresh seeded search or **Watch recorded discovery** for labeled playback. Run the saved brain immediately, generate a fresh maze, or draw your own. Step mode highlights the exact rule and memory transition used at each action. The source panel contains the actual discovered decision program in Gremlin syntax.

## What is learned

The brain is a two-state finite-state controller. Each of its 16 rules corresponds to three binary sensors (open ahead, left, right) and the current memory bit. The rule chooses one of four actions—forward, turn left, turn right, turn around—and the next memory bit. Turns consume an action without moving. Moving into a wall consumes an action without changing position.

All 48 decision bits begin random. Mutation changes arbitrary rules, not constants in a hand-selected navigation strategy. The search supplies no wall-following algorithm, route, map, target coordinates, or pathfinder to the brain. The complete class contains 2^48 rule tables, although this bounded search explores only a small fraction of it. A deterministic decision tree implements the learned table; all 16 possible observations are checked against that source program.

The environment and genome representation are supplied. Movement, local sensor calculations, automatic key pickup, and door mechanics are fixed. The robot's physical position and heading are simulator state; only its extra one-bit memory is controlled by the brain. A collected key changes whether the door blocks movement, but the brain does not receive a key flag or object coordinates.

Search uses a population of 256, retains 16 elites, samples parents from the top 48, mutates 1–3 randomly chosen rules, and inserts a random genome with probability 1/20. Ranking first rewards escapes, then keys collected, unique cells visited, and fewer actions in successful episodes. Numeric genome order is the final deterministic tie-break. There is no neural model, pretrained controller, or seeded solution.

The larger setting uses a curriculum: solve 128 training rooms, then re-evaluate the entire population against all 512 training rooms and continue if necessary. This decision uses training results only. Starting directly on 512 rooms stalled one prototype seed at partial success; the staged search avoids imposing the harder objective immediately. This is a search heuristic, not a completeness guarantee.

## Maps, limits, and evidence

Maps are 8×8 with a closed border and a 6×6 editable interior. Training maps are random connected branching mazes without cycles. The generator places the door on the route to the exit, with the key reachable before opening it. Starting location and orientation vary. Training rooms are unique; all 512 holdout rooms are explicitly disjoint from training and never influence selection.

The editor also accepts rooms outside that distribution. Large open spaces and loops can defeat the discovered controller. It checks that the key is geometrically reachable while the door is locked and that the exit can be reached afterward; that check is not used to choose the robot's actions. Unsolved runs visibly stop after 256 actions. A successful test set does not prove every user-drawn room solvable by that brain.

Each population/room pair runs as a complete Gremlin program in the general CUDA interpreter, including the simulation loop and state updates. The brain is packed into a constant for GPU lookup; the inspectable decision-tree program is independently checked to implement exactly the same decisions. No robot-specific GPU kernel is substituted.

Budgets: initial population plus at most 80 evolutionary rounds, 256 robot actions per episode, 80,000 interpreted steps per execution, and a 1 GiB device allocation ceiling. Remote jobs have a 180-second timeout and the web process has a 190-second watchdog. The server accepts one training job at a time. A search can exhaust a budget or encounter an infrastructure error; neither is reported as a successful discovery.

Every GPU episode's packed result (escape, key, visited-cell count, and action count) is checked against a separate native Rust simulator. After selection, all 512 holdout episodes are replayed through both the Gremlin interpreter and that simulator. Browser simulation is separately checked against the recorded frame-by-frame traces, so editing a room executes the same controller semantics locally.

## Recorded results

On the supplied RTX A4000, the default recorded run (seed 1, 128 training rooms) improved from **1/128 escapes** to **128/128** at generation 12, then passed **512/512 unseen rooms**. It evaluated **425,984 episodes** in **11.23 seconds**, executing about **15.94 billion Gremlin instructions**, with **148 MiB** peak reported device allocation.

The 128-room setting passed all training and holdout rooms for seeds 1, 2, and 3, in approximately 11.23, 4.28, and 11.46 seconds respectively. These are individual observed runs, not guaranteed runtimes. Search timing includes program construction, evaluation, independent result checks, scoring, and preview events; final holdout replay is outside that timer. GPU context startup and transfers are included, while browser playback and SSH launch latency are not.

With the 128→512 curriculum, seeds 1–3 passed all 512 training and 512 holdout rooms in **13.84, 7.41, and 14.68 seconds**, respectively. Peak device allocation was **584 MiB**. The extra round re-evaluates the full population on the expanded training corpus.

`measurements.json` records the measured runs, including the larger curriculum checks. `sample.json` contains the first run's real progress stream and example trajectories. Playback is labeled throughout, and its timings remain the measured values rather than animation durations.

## Checks

```sh
cargo test --release --locked --manifest-path apps/tiny-robot/engine/Cargo.toml
cargo clippy --locked --manifest-path apps/tiny-robot/engine/Cargo.toml --all-targets -- -D warnings
python3 apps/tiny-robot/test_server.py

# Live GPU search:
apps/tiny-robot/engine/target/release/tiny-robot gpu robot 128 1
apps/tiny-robot/engine/target/release/tiny-robot gpu robot 512 1
```

Tests check controller-source equivalence, interpreter/simulator agreement, unique acyclic maps, key-before-door reachability, HTTP request bounds, and final-event cleanup for lingering transports. Headless browser checks cover recorded trace parity, maze solving, painting, bounded failure on an open room, replay, and mobile layout. Local screenshots live in the ignored `runs/tiny-robot/` directory.
