# ADR 0010: Visual and Cryptographic Signatures

## Status

Accepted

## Context

Users need to place a signature appearance now, while cryptographic signing introduces separate identity, certificate, integrity, and legal concerns.

## Decision

Initially support insertion of a visual signature image/appearance overlay only. It provides no identity validation, certificate chain, integrity guarantee, or non-repudiation. Defer certificate-backed digital signatures to a later capability with a separate ADR.

## Consequences

- CLI wording, documentation, and diagnostics must not describe visual signatures as validated digital signatures.
- Placement, scaling, and appearance are ordinary PDF composition concerns.
- Future cryptographic signing must define certificate storage, algorithms, validation, timestamps, and tamper evidence independently.

## Alternatives Considered

- Call any image overlay a digital signature: rejected because it is misleading and insecure.
- Build certificate signing immediately: deferred to preserve focus and avoid premature security design.
- Exclude signatures entirely: rejected because visual signing is in scope.

## Validation/Exit Criteria

- Visual signature tests prove placement and preserve the explicit non-cryptographic classification.
- Help text and result metadata state the absence of identity/integrity guarantees.
- A future signing proposal includes threat model and certificate lifecycle design.
