# ADR 0004: PDF Engine Selection and Permissive Licensing

## Status

Accepted

## Context

No candidate has yet proved sufficient for the full PDF scope. Engine choice affects editing limits, redaction safety, licensing, performance, and Homebrew packaging.

## Decision

Select no production PDF engine yet. Run corpus-based spikes for PDFium through `pdfium-render` and alternatives such as `lopdf`; assess an isolated Python worker only when required by evidence. `pdfium-render` 0.9.3 is a pre-1.0 spike candidate only. Use permissive alternatives only. Do not accept AGPL or commercial engines without a future explicit exception.

## Consequences

- The adapter boundary remains mandatory until evidence selects an engine/version.
- Packaging of native runtimes is part of the engine decision.
- License review is a functional release criterion, not a procurement afterthought.

## Alternatives Considered

- Select PDFium now: rejected because the candidate is pre-1.0 and evidence is incomplete.
- Select a low-level Rust library now: rejected because capability coverage is unproven.
- Accept AGPL/commercial engines: rejected under the current licensing policy.

## Validation/Exit Criteria

- Each candidate has corpus results for scope coverage, failures, performance, redaction, licensing, and macOS/Homebrew packaging; scope coverage includes fixtures for page rotation, reordering, and deletion that are distinct from text-content deletion.
- A selected version has a documented license inventory and reproducible runtime acquisition.
- Selection is recorded by an ADR update with rollback criteria.
