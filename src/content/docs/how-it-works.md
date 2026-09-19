---
title: How it works
description: An approachable overview of gremlin's program-search loop.
---

gremlin works in three connected stages.

## 1. Describe a small program

The gremlin language is intentionally narrow: functions take up to four integer arguments, use explicit types, bind immutable values with `let`, and finish with a `return`. Arithmetic wraps at the chosen integer width, so a `u8` program behaves like an eight-bit machine value.

## 2. Observe behavior

For the current project scope, checked-in Rust fixtures provide known input/output examples. These examples form a corpus: the target behavior that candidate programs are trying to match.

## 3. Search and check

The search engine creates and mutates short candidate programs, runs them against the corpus, and ranks them by correctness before size or cost. When a candidate matches the corpus, gremlin also checks it against a separately generated holdout set.

Every run records the configuration, observed data, best candidate, and a report. This makes the result reproducible: rerunning with the same tool version, configuration, and seed follows the same search path.

## Why the limits matter

The current source language and search are straight-line. That keeps the first version understandable and makes runs predictable, but it also means gremlin is not a general binary decompiler or a proof system.
