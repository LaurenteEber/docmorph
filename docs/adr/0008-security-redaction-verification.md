# ADR 0008: Security Redaction and Independent Verification

## Status

Accepted

## Context

Visual black boxes or overlays can leave searchable, extractable, or recoverable information in a PDF. Security redaction has a stronger meaning than ordinary editing.

## Decision

Implement security redaction only as irreversible removal of existing information. A visual cover-up is never reported as redaction. Require independent verification after generation, using checks that do not rely solely on the mutation path, and record results in provenance.

## Consequences

- Redaction may require selected engines, restrictive fixtures, and a narrower initial support matrix.
- A verification failure makes the operation fail; a visually plausible file is insufficient.
- Product copy must distinguish redaction from annotation, highlight, and overlay.

## Alternatives Considered

- Draw opaque rectangles: rejected because underlying content can remain recoverable.
- Trust the engine's success status alone: rejected because verification must be independent.
- Defer all redaction: rejected because security redaction is in scope, but it remains gated by proof.

## Validation/Exit Criteria

- Adversarial fixtures pass text extraction, search, object inspection, and raster-render checks after redaction.
- Verification is implemented independently of the redaction adapter where practical.
- Provenance identifies the verifier/version and its pass/fail evidence.
