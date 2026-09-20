# IR and corpus format

## Validation and normalization

SSA values and blocks have separate ID namespaces. Values have one definition and instructions may use only earlier local definitions or definitions in dominating blocks. Function parameters dominate every block.

Every block must be reachable, and the entry block has no block parameters. Edges pass equal-typed arguments simultaneously. Validation rejects malformed IR without repair.

Normalization numbers blocks in breadth-first edge order from entry and values in parameter/instruction order.

## Canonical source

Source printing uses canonical IDs and hexadecimal constants, and it round-trips to the same normalized IR. General CFGs print with explicit `block`, `jump`, and `branch` syntax so edge bindings and exact step counts are preserved.

## Observations and transport

Corpus values use exact-width `0x` hexadecimal strings, including signed values. Transport encodes each argument in signature order, little-endian at its declared width; results use the same representation.

Corpus identity includes the schema, target implementation fingerprint, signature, contract, and sorted observations. Provenance is separate from the content hash. Only matching completed results count as observations.

## Content hash

The corpus content hash is SHA-256 over:

1. The `gremlin-corpus-content-v1\0` prefix.
2. The u64 little-endian byte length of a canonical JSON `[schema_version, target_identity]` header, followed by that header.
3. A u64 little-endian case count.
4. For each sorted case, its concatenated argument bytes followed by its return bytes.

Argument and result encodings are fixed by the signature, so observation boundaries are unambiguous. Provenance is excluded; whole artifact file hashes are recorded separately.
