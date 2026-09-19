---
title: gremlin
description: A small, reproducible program-synthesis system for integer functions.
---

gremlin is a Rust project that searches for small integer programs matching examples of a function's behavior. Give it a signature, observations, and a constrained set of operations; it explores candidate programs and records what it found.

## What you can do today

- Check and run small, typed gremlin programs locally.
- Search for compact programs that match a checked-in reference fixture.
- Save a run, inspect its evidence report, and resume it later.

The current implementation is deliberately CPU-first, deterministic, and self-contained. It does not need a network connection to run.

## A useful mental model

gremlin is an experiment in finding a concise implementation from examples. A successful search result is evidence that a candidate matched the examples it was given — it is not automatically a proof that two arbitrary programs are equivalent.

Start with [Getting started](./getting-started/) for the quickest way to build and run the example.
