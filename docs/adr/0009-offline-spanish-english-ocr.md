# ADR 0009: Offline Spanish and English OCR

## Status

Accepted

## Context

OCR is required for Spanish and English but is resource-intensive and quality-sensitive. Online services violate the local/offline product boundary.

## Decision

Provide OCR as a capability, not an implicit fallback. It must use an explicitly selected offline engine, pinned version, pinned Spanish and English language data, declared preprocessing, and enforced resource limits. OCR output must identify confidence/limitations when the engine can provide them.

## Consequences

- The distribution must account for engine and language-data licensing, size, and installation.
- OCR jobs use the conversion/OCR proposed limit defaults until calibrated.
- OCR-derived text is diagnostically distinct from text extracted from a born-digital PDF.

## Alternatives Considered

- Cloud OCR: rejected because processing must work offline.
- Unpinned system OCR: rejected because output cannot be reproduced or supported.
- Silent OCR fallback: rejected because it masks changed content provenance.

## Validation/Exit Criteria

- Corpus benchmarks include Spanish, English, mixed-language, low-quality scan, rotated, and skewed samples.
- Results capture engine/version/language-data identity and resource use.
- Release packaging proves offline installation and operation.
