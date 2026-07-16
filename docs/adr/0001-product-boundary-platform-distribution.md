# ADR 0001: Product Boundary, Platform, and Distribution

## Status

Accepted

## Context

The product is a local document/PDF tool rather than a cloud service. It needs a narrow initial delivery target while retaining a path to wider support.

## Decision

Build an offline-first tool for macOS first, with a CLI as the initial interface and Homebrew as the initial distribution channel. A TUI is optional and later. The initial scope includes DOCX/Markdown/PDF conversion, PDF merge/split/page rotation/page reordering/page deletion, annotations, forms, visual signature-image insertion, watermarks, bounded content editing, formatted-page composition, security redaction, and Spanish/English OCR.

## Consequences

- Local execution, packaging, and failure handling are first-class requirements.
- macOS/Homebrew compatibility is a release gate, not an afterthought.
- The CLI contract must remain suitable for scripts and future UI clients.

## Alternatives Considered

- Cloud-first processing: rejected because local/offline operation is the product boundary.
- GUI/TUI first: rejected because a CLI gives the smallest automation-friendly surface.
- Multi-platform launch: deferred to avoid diluting initial packaging and corpus validation.

## Validation/Exit Criteria

- Homebrew installation and offline execution work on supported macOS versions.
- Every scoped operation has a documented CLI capability and result contract.
- A later TUI consumes the CLI/core contract rather than defining policy.
