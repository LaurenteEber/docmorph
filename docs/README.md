# Product Architecture Handoff

This package is the portable starting point for a production, offline-first local document and PDF tool. Keep this directory in the repository before implementation. It is self-contained and does not require access to repository history or Engram.

## Quick Start

1. Keep `docs/` in the repository root.
2. Read this file, then ADRs `0001` through `0011` in order.
3. Create the repository baseline only after the mandatory spikes have produced recorded evidence.
4. Treat Accepted ADRs as constraints; resolve Proposed and Deferred items through controlled ADR updates.

## Decision Inventory

| State | Decisions |
|---|---|
| Accepted | Offline-first local product; macOS first; CLI first; Homebrew distribution; Rust-first core; engine adapters; permissive licensing; 200 MB input limit; Spanish/English offline OCR; 2024 Rust edition and `rust-version = "1.96"`; committed lockfile; visual signature images only; irreversible redaction with verification. |
| Proposed | Default job limits: 60 seconds for structural operations, 180 seconds for conversion/OCR, and 1.5 GiB hard memory per worker. These require corpus calibration before release. |
| Deferred | TUI, certificate-backed digital signatures, final PDF engine, Python worker, broader platforms, and production acceptance of a Markdown-to-PDF engine. |

The current prototype is evidence, not a codebase to migrate blindly. Preserve its useful findings, but reimplement production behavior against these contracts and spike results.

## Architecture At A Glance

```text
User / scripts
      |
   Rust CLI -------------------- optional Rust TUI (later)
      |
Job lifecycle, capability registry, policy, limits, diagnostics,
provenance, atomic publication, plugin protocol
      |
      +-------------------+--------------------+
      |                   |                    |
Rust engine adapter   isolated worker      local asset store
(selected by spike)   (only if justified)  (bounded, safe paths)
      |                   |                    |
PDF/DOCX/OCR engine   versioned sidecar    inputs and artifacts
      |
Operation result: artifacts + diagnostics + provenance
```

Rust owns business rules and all policy. An engine adapter executes a narrowly defined capability and returns structured evidence. A Python worker is not part of the initial architecture; it may be introduced only when a measured spike proves it materially necessary, and then only as a versioned isolated sidecar without business rules.

## Capability Map

| Area | Initial capability | Boundary |
|---|---|---|
| Conversion | DOCX to Markdown, PDF to Markdown, Markdown to DOCX, Markdown to PDF | Use a semantic document AST where applicable; report loss rather than inventing fidelity. |
| PDF structure | Merge, split, rotate, reorder, and delete pages | Structural operations must preserve documented metadata and report changes. Page deletion removes selected pages; it is distinct from text-content deletion and security redaction. |
| PDF interaction | Annotations, forms, watermarks | Engine-dependent capabilities exposed through the registry. |
| PDF editing | Delete or replace text content; replacement font family, size, and color; highlight; strikethrough | Arbitrary PDF editing is limited by the source document and engine; unsupported changes must fail or degrade diagnostically. Text-content deletion never removes a page. |
| Composition | Add a page with formatted text, font, size, color, alignment, and basic layout | This is authoring, not a promise of arbitrary existing-PDF reflow. |
| Security | Irreversible redaction of existing information | A visual cover-up is not redaction; independent verification is mandatory. |
| Signatures | Insert a visual signature image/appearance | No identity validation, certificates, integrity guarantee, or non-repudiation. |
| OCR | Offline Spanish and English recognition | Pin engine, version, language data, and resource limits. |

## Boundaries And Non-Goals

The product is a local tool, with no cloud processing requirement. The first distribution target is Homebrew on macOS. The CLI is the product interface; a TUI is optional later.

Out of scope for the initial release: a permanent Python/Rust hybrid, AGPL or commercial engines without an explicit future exception, claims of universal PDF editability, certificate-backed signing, unbounded inputs, overwrite-by-default output behavior, and byte-identical PDF output promises.

## Initial Performance And Security Constraints

| Constraint | Status | Rule |
|---|---|---|
| Input size | Accepted | Reject inputs larger than 200 MB before processing. |
| Structural job time | Proposed | Default 60 seconds; calibrate against a representative corpus. |
| Conversion/OCR job time | Proposed | Default 180 seconds; calibrate against a representative corpus. |
| Worker memory | Proposed | Hard 1.5 GiB per worker; calibrate against a representative corpus. |
| I/O | Accepted | Canonicalize and bound local paths, reject unsafe assets, publish no-overwrite artifacts atomically. |
| Redaction | Accepted | Verify independently after output generation and retain verification evidence in provenance. |

## ADR Index

| ADR | Title | Status |
|---|---|---|
| [0001](adr/0001-product-boundary-platform-distribution.md) | Product Boundary, Platform, and Distribution | Accepted |
| [0002](adr/0002-rust-core-engine-adapter-boundary.md) | Rust Core and Engine Adapter Boundary | Accepted |
| [0003](adr/0003-operation-result-diagnostics-provenance-contract.md) | Operation Contract and Capability Registry | Accepted |
| [0004](adr/0004-pdf-engine-selection-and-licensing.md) | PDF Engine Selection and Permissive Licensing | Accepted |
| [0005](adr/0005-fidelity-and-deterministic-output-tiers.md) | Fidelity and Deterministic Output Tiers | Accepted |
| [0006](adr/0006-safe-io-resource-policy-and-workers.md) | Safe I/O, Resource Policy, and Isolated Workers | Accepted |
| [0007](adr/0007-pdf-editing-and-composition-limits.md) | PDF Editing and Page Composition Limits | Accepted |
| [0008](adr/0008-security-redaction-verification.md) | Security Redaction and Independent Verification | Accepted |
| [0009](adr/0009-offline-spanish-english-ocr.md) | Offline Spanish and English OCR | Accepted |
| [0010](adr/0010-visual-and-cryptographic-signatures.md) | Visual and Cryptographic Signatures | Accepted |
| [0011](adr/0011-versioning-supply-chain-release-policy.md) | Versioning, Supply Chain, and Release Policy | Accepted |

## Mandatory Spikes

1. Build a representative corpus containing tagged PDFs, scans, forms, annotations, damaged files, varied fonts, DOCX layouts, redaction fixtures, and page-operation fixtures that exercise rotation, reordering, and deletion of selected pages separately from text-content deletion.
2. Compare `pdfium-render` 0.9.3 as a pre-1.0 spike candidate with alternatives such as `lopdf` and, only if necessary, an isolated Python worker. Measure capability coverage, including page rotation, reordering, and deletion; license obligations, macOS/Homebrew packaging, failure isolation, performance, and redaction support.
3. Establish conversion fidelity baselines for DOCX/Markdown/PDF and prove deterministic DOCX packaging. The prototype PDF extraction is baseline evidence only; PDF byte determinism was not proven.
4. Prove Markdown-to-PDF deterministic output before accepting any ReportLab prototype or alternative as production-ready.
5. Validate OCR with explicitly pinned offline engine/version/language data for Spanish and English under the proposed limits.
6. Prove destructive redaction and independent post-output verification against adversarial fixtures, including extracted text, raster renderings, object inspection, and search.

## Phased Build Order

1. Establish the Rust workspace, CLI shell, contracts, capability registry, provenance schema, safe I/O, and no-overwrite atomic publisher.
2. Implement corpus harnesses and mandatory engine, fidelity, OCR, resource, and redaction spikes.
3. Select and pin engines through ADR updates; implement and validate PDF merge, split, page rotation, page reordering, page deletion, and supported conversions.
4. Add diagnostics-driven editing, composition, annotations, forms, watermarks, and visual signature insertion.
5. Harden release, Homebrew packaging, supply-chain checks, and benchmark gates.
6. Consider a TUI and certificate-backed digital signatures only after their own ADRs and capability evidence.

## First-Session Checklist

- [ ] Keep this directory in the repository without changing its contents.
- [ ] Read every Accepted ADR before choosing dependencies.
- [ ] Create a corpus and test harness before selecting a PDF engine.
- [ ] Record measured outputs, versions, licenses, machine details, and failures for each spike.
- [ ] Turn each selected engine/version and calibrated limit into an ADR update and controlled pull request.
- [ ] Keep all policy in Rust; verify any worker remains a narrow adapter.
- [ ] Do not claim redaction, fidelity, or determinism beyond recorded validation evidence.

## Open Questions

- Which permissively licensed engine meets the corpus requirements and packages reliably through Homebrew?
- Which structural metadata, tags, forms, and annotations are guaranteed per operation and engine version?
- What benchmark corpus and hardware profile set release thresholds and calibrate proposed time/memory limits?
- Which Markdown profile and PDF rendering route can prove acceptable visual fidelity and deterministic behavior?
- Is any isolated Python worker materially necessary after Rust and native-engine spikes?
- What certificate/signing standard and key-management model should govern the later digital-signature capability?
