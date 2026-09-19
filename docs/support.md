# Supported v1 contract

All signatures have 0–4 explicitly typed integer arguments and one integer result. Integer widths are 8/16/32/64, signed or unsigned. Bool is internal. Memory, pointers, floats, external calls, allocation, concurrency and observable I/O are outside the language/target contract.

| Feature | CPU/source | CUDA | Formal candidate model | LLVM artifact |
| --- | --- | --- | --- | --- |
| All 27 documented operators | Yes | Yes | Yes | Yes |
| Wrapping, modulo shifts, eager select, division traps | Yes | Parity tested | Explicit bit vectors/trap reasons | Guarded lowering; native tested |
| Branches and loops | Yes | Parity tested | Unsupported | Yes, with configured step budget |
| Calls/recursion | Explicit frames, step/depth limits | Unsupported | Unsupported | Unsupported |
| General CFG round trips | Explicit block syntax | Same IR | Single return block only | Same IR |
| Candidate mutation/selection | Deterministic CPU; optional CFG mutations/enumeration | Evaluation only | Not a search backend | Selected candidates only |

The binary oracle supports Linux x86-64 ELF shared-library functions under an explicitly supplied SysV integer ABI. It runs in an empty mount/network/user/PID namespace with resource limits and a default-deny seccomp filter. Constructors run inside that boundary. Every required runtime dependency is read-only and fingerprinted. Unsupported dependencies, namespace policy failures, forbidden operations, crashes, timeouts and nondeterministic observations are errors. No reduced-isolation fallback exists.

Binary E4 is narrower: ordinary unambiguous STT_FUNC exports in constructor/dependency/relocation-free ELF; at most 256 decoded instructions and 4096 symbol bytes; one final single-byte near RET; MOV/ADD/SUB/IMUL/XOR/AND/OR/NOT/NEG over 32/64-bit caller-saved registers and immediates, plus 64-bit-address LEA. Register reads must be defined. Loads/stores, stack changes, callee-saved registers, calls, branches, flag consumers, address-size overrides, versioned/resolver symbols and uncovered code are Unsupported. Symbol/code and dynamic-table bytes must match load mappings, and GNU/SysV hash lookup must resolve the same modeled entry. Readable 4 KiB-aligned PT_LOAD segments must have equal file/memory sizes; BSS extension is unsupported. E4 is relative to the explicitly recorded ABI, immutable-code, lifter and solver assumptions. A reference-model proof never becomes binary evidence.

E1/E2 results and native artifact checks are TESTED. Native output does not inherit the source candidate's E4 proof. CUDA is optional, and measured small-workload end-to-end performance is slower than CPU. A CUDA toolkit is required only for feature-enabled builds; a usable device is required for GPU tests. The provided GPU container passed parity/performance tests but cannot create binary-isolation namespaces; that combined deployment gate was not run there. A CUDA-enabled executable was also used successfully for the full binary/proof/native pipeline on the local namespace-capable host.

libFuzzer is external and opt-in. It mutates integer transport bytes and uses the isolated oracle bridge. Coverage is limited to the harness; target coverage is unavailable for uninstrumented binaries. Exports/imports are replayed and provenance-preserving observations, not equivalence or coverage guarantees. Campaigns are single-use and versioned.
