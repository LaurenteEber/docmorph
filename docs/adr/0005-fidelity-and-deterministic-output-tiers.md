# ADR 0005: Fidelity and Deterministic Output Tiers

## Status

Accepted

## Context

Conversions cannot honestly guarantee identical structure, layout, and bytes across all document formats. The prototype established useful baselines but did not prove PDF byte determinism.

## Decision

Promise faithful readable content, visual layout, and structural preservation where supported, not universal or byte-identical output. Define per-operation fidelity tiers and emit diagnostics for loss, approximation, or unsupported source features. Require deterministic DOCX packaging. Treat PDF byte determinism as unproven. Do not accept a ReportLab Markdown-to-PDF prototype as production-ready until deterministic-output evidence passes.

## Consequences

- Product messaging remains tied to measured capability tiers.
- Regression suites compare semantic, visual, structural, and deterministic evidence as appropriate.
- Diagnostics are a normal successful-result channel, not merely errors.

## Alternatives Considered

- Universal round-trip guarantee: rejected as technically false for heterogeneous document formats.
- Silent best effort: rejected because users need explicit degradation evidence.
- Byte-identical PDFs as a baseline promise: rejected because it is unproven and often engine-dependent.

## Validation/Exit Criteria

- Each conversion declares its fidelity tier and comparison method.
- Corpus tests prove deterministic DOCX packaging and record PDF determinism separately.
- Known loss cases produce stable, actionable diagnostics.
