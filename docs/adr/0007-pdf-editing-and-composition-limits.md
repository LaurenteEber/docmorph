# ADR 0007: PDF Editing and Page Composition Limits

## Status

Accepted

## Context

PDFs encode positioned graphics and text, not a universal editable document model. Requested editing must be useful without making impossible promises.

## Decision

Support text-content deletion/replacement with replacement font family, size, and color; highlighting; strikethrough; annotations; forms; watermarks; and adding a page with formatted text, font, size, color, alignment, and basic layout. Support page rotation, page reordering, and page deletion as distinct PDF-native structural operations. Page deletion removes selected output pages; it is not text-content deletion or security redaction. Treat composition as authoring a new page. Arbitrary editing of existing PDFs is engine- and document-dependent: unsupported changes must fail clearly or complete with diagnostics, never simulate success.

## Consequences

- Editing UI/CLI options must be capability-gated by adapter/document analysis.
- Replacement may require overlay or document rewrite strategies, each with declared fidelity limits.
- New-page authoring has a smaller, dependable contract than arbitrary reflow.
- Page rotation, reordering, and deletion require registry entries that declare supported page-selection rules, document/adapter constraints, and diagnostics for applied rotation, resulting order, and removed pages.
- Text-content deletion, page deletion, and security redaction remain separate operations with separate result semantics and validation evidence.

## Alternatives Considered

- Claim full word-processor editing for every PDF: rejected because the PDF model and source documents do not support it.
- Limit scope to only page rotation, reordering, and page deletion: rejected because it does not meet product needs.
- Treat visual cover-up as text-content or page deletion: rejected because it violates security semantics.

## Validation/Exit Criteria

- Corpus fixtures cover embedded fonts, scanned PDFs, positioned glyphs, flattened content, unsupported cases, and page-operation cases for rotation, reordering, and deletion of selected pages. These fixtures must distinguish page deletion from text-content deletion.
- Results report the applied strategy and any limitations; page-operation results identify applied rotation, resulting page order, and removed pages.
- Capability checks reject or diagnose unsupported page selections, rotation values, document states, and adapter limits before or during execution.
- New-page output preserves declared formatting in visual regression tests.
