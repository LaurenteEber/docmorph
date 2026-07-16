# ADR 0006: Safe I/O, Resource Policy, and Isolated Workers

## Status

Accepted

## Context

Local document inputs can be malformed, large, or path-hostile. Native engines and optional workers need containment so failures do not corrupt outputs or escape policy.

## Decision

Reject inputs larger than 200 MB. Use bounded parsing, safe local assets, canonicalized allowed paths, and no-overwrite atomic publication. Proposed defaults are 60 seconds for structural operations, 180 seconds for conversion/OCR, and a 1.5 GiB hard per-worker memory limit; calibrate all on a representative corpus. Run external engines and any Python capability in isolated versioned workers under the Rust-owned protocol.

## Consequences

- Jobs are cancellable and failures leave no partial published artifact.
- Limits are recorded in provenance and become benchmark-managed configuration.
- Worker isolation has startup and packaging overhead but contains unsafe/native failures.

## Alternatives Considered

- Unbounded local processing: rejected for reliability and denial-of-service risk.
- Direct writes to requested outputs: rejected because crashes can corrupt existing files.
- In-process optional workers: rejected because isolation and lifecycle ownership would weaken.

## Validation/Exit Criteria

- Tests cover oversized input, traversal attempts, malformed files, timeouts, memory termination, and output collision.
- Publication either creates a complete new artifact or leaves the destination unchanged.
- Corpus benchmarks approve, revise, or reject each proposed default before release.
