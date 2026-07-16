# ADR 0003: Operation Contract and Capability Registry

## Status

Accepted

## Context

Document processing has uneven support and unavoidable degradation. Fixed dispatch based only on input/output extensions cannot express options, limitations, or evidence.

## Decision

Define versioned domain operations with typed inputs, requested options, declared capability requirements, bounded execution, artifacts, diagnostics, and provenance. Maintain a capability registry that states which adapter/version supports each operation and its limits. Use a semantic document AST for content conversions where applicable; keep PDF-native structural page operations distinct from content editing. Diagnostics and a fidelity policy are required outputs.

## Consequences

- Callers can discover support before execution and interpret degradation afterward.
- Provenance records engine/version, options, input/output identities, limits, and verification results.
- Registry entries and results for page rotation, reordering, and deletion must declare page-selection, ordering, rotation, and adapter/document constraints; page deletion must remain distinct from text-content deletion.
- UI-owned policy and fixed extension-pair dispatch are explicitly rejected.

## Alternatives Considered

- Extension-pair command routing: rejected because it hides capability and loss semantics.
- Adapter-specific public APIs: rejected because engine choice would leak into the product contract.
- UI-owned validation: rejected because CLI and future TUI would diverge.

## Validation/Exit Criteria

- A result schema can represent success with warnings, unsupported requests, failures, and generated artifacts.
- Registry queries identify supported options and engine constraints before job submission.
- Contract tests distinguish text-content deletion from page deletion and represent the applied page-operation effects.
- Golden contract tests cover schema compatibility and provenance completeness.
