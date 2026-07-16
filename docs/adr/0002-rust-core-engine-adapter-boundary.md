# ADR 0002: Rust Core and Engine Adapter Boundary

## Status

Accepted

## Context

The product needs native distribution, strict resource governance, and multiple document-engine capabilities that may change as evidence improves.

## Decision

Use a Rust-first core. Rust owns CLI/TUI, job lifecycle, policy, limits, diagnostics, provenance, atomic publication, and the plugin protocol. Document engines are adapters behind versioned capability contracts. Do not create a permanent Python/Rust hybrid initially. Permit a Python worker only after an evidence-based spike proves it materially necessary; it must be an isolated, versioned sidecar with no business rules.

## Consequences

- Core policy is testable and consistent regardless of engine choice.
- Engines can be replaced without moving workflow rules into an adapter.
- A worker boundary adds packaging and observability cost, justified only by measured capability gain.

## Alternatives Considered

- Python-first core: rejected for the initial native lifecycle and distribution requirements.
- Permanent mixed-language core: rejected because ownership and policy drift become structural debt.
- Pure Rust engine-only promise: rejected because it could prematurely exclude needed document capabilities.

## Validation/Exit Criteria

- The core can execute a mock adapter using only the versioned contract.
- No adapter controls output paths, resource policy, provenance semantics, or user-facing business rules.
- Any Python sidecar proposal includes comparative corpus results and a separate ADR update.
