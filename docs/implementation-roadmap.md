# Evidence-Gated Implementation Roadmap

This roadmap turns the accepted architecture into an executable sequence without selecting an engine or claiming unproven capability. It applies to the macOS-first, offline CLI described by the ADRs in this directory.

## Purpose and Scope

Build a Rust-first local document and PDF tool whose engine choices, fidelity promises, resource limits, and security claims are supported by recorded evidence. The roadmap covers repository setup, corpus-driven evaluation, isolated engine spikes, selection governance, and the subsequent delivery phases. It does not authorize a production PDF engine, a Python worker, universal PDF editing, or cryptographic signing.

## Delivery Sequence

| Phase | Objective | Deliverables | Exit criteria |
|---|---|---|---|
| 0. Foundation | Make contracts and execution policy testable before engine work. | Rust workspace, CLI shell, versioned operation/result/provenance schemas, capability-registry interface, safe I/O and atomic publication design. | A mock adapter can run through the contract; policy remains outside adapters; no selected engine is encoded in public contracts. |
| 1. Evidence platform | Create the corpus and reproducible harness used for every evaluation. | Versioned fixtures, manifests, comparison tools, machine/toolchain capture, baseline reports. | A clean checkout can reproduce a named run and retain inputs, versions, commands, outputs, diagnostics, and failures. |
| 2. Engine spikes | Measure 2–3 permissively licensed candidates behind isolated adapters. | One disposable spike per candidate, capability matrix, license and packaging reports, raw benchmark evidence. | Each candidate has comparable evidence for the agreed fixtures and gates; no spike becomes production code by default. |
| 3. Selection | Choose, pin, or reject candidates from recorded evidence. | Decision report, ADR update when a candidate passes, version and acquisition pinning, rollback criteria. | Selection is approved only when all required gates pass; otherwise the ADR remains unselected and the next candidate or scope reduction is recorded. |
| 4. Core delivery | Implement proven structural operations and supported conversions. | Production adapters, registry entries, diagnostics, provenance, contract and regression tests. | Every shipped operation has an evidence-backed support matrix, bounded execution, and explicit fidelity behavior. |
| 5. Advanced capability | Add editing, composition, annotations, forms, watermarks, OCR, and visual signatures only where proven. | Capability-specific fixtures, diagnostics, package evidence, regression suites. | Each operation meets its declared tier; unsupported documents fail or degrade diagnostically. |
| 6. Security and release | Prove redaction safety and release readiness. | Independent redaction verifier, adversarial evidence, Homebrew formula/package validation, supply-chain artifacts. | Redaction passes independent checks; packaging, offline operation, audits, SBOM, and release gates pass. |

## Phase 0: Workspace and Contract Prerequisites

- Initialize the workspace with Rust stable 1.96.0, edition 2024, `rust-version = "1.96"`, and a committed `Cargo.lock`.
- Define typed, versioned requests and results for inputs, options, capability requirements, artifacts, diagnostics, provenance, and bounded execution.
- Define a capability registry that reports adapter/version support and limits before job submission.
- Keep Rust responsible for CLI behavior, lifecycle, policies, resource limits, path validation, output publication, diagnostics, and provenance.
- Establish no-overwrite atomic output publication, canonicalized allowed paths, bounded parsing, and the 200 MB input rejection rule before running native or external engines.
- Treat the proposed time and memory defaults as calibration hypotheses, not release thresholds: 60 seconds for structural operations, 180 seconds for conversion/OCR, and 1.5 GiB per worker.

## Phase 1: Evaluation Corpus and Reproducible Harness

### Corpus

Create a versioned corpus manifest with fixture identifiers, source/license provenance, expected characteristics, sensitivity classification, and allowed distribution. Include representative and adversarial cases:

- Born-digital PDFs with tags, metadata, annotations, forms, embedded and substituted fonts, rotated pages, and varied page sizes.
- PDFs for merge, split, rotation, reordering, selected-page deletion, and text-content deletion; keep those operations semantically distinct.
- Scanned, Spanish, English, mixed-language, skewed, rotated, and low-quality OCR inputs.
- DOCX layouts with tables, headers/footers, lists, images, and font variation; Markdown inputs covering the supported profile under evaluation.
- Editing and composition cases: positioned glyphs, flattened content, unsupported content, replacement fonts, highlights, strikethroughs, watermarks, and formatted added pages.
- Security redaction fixtures containing searchable, extractable, raster-visible, and object-level sensitive values.
- Damaged, malformed, oversized, and path-hostile inputs for failure and resource-policy evaluation.

Do not include private or undistributable documents in a repository corpus. Store their manifest metadata and run instructions separately if they are required for a controlled evaluation.

### Harness

The harness must execute each candidate against the same named corpus revision and emit a machine-readable report plus retained artifacts. Record:

- Candidate name, source, exact version, build flags, dependency/runtime identity, and license inventory.
- Rust toolchain, macOS version and architecture, Homebrew version where applicable, command line, environment constraints, and corpus revision.
- Requested operation/options, declared capabilities, elapsed time, peak memory when measurable, exit status, diagnostics, output hashes, and failure classification.
- Structural comparisons, visual render comparisons, semantic/text comparisons, and determinism checks appropriate to the operation.

Run each determinism case repeatedly under the same pinned environment. Record PDF byte equality separately from visual or structural equality; no byte-determinism claim follows from a visually stable result.

## Phase 2: Isolated Engine Spikes

Spike two or three candidates with permissive licenses. Begin with the candidates named in ADR 0004, including `pdfium-render` 0.9.3 as a pre-1.0 spike candidate and `lopdf`; introduce a third candidate only when it provides a meaningful comparison. Evaluate an isolated Python worker only if Rust/native candidates leave a documented gap worth its packaging and observability cost.

Each spike must:

1. Use a narrow adapter and the common contract, with no policy or public CLI behavior embedded in the candidate.
2. Be independently buildable and removable; do not merge spike-specific assumptions into production architecture.
3. Run the shared harness and publish raw results, including unsupported operations and failures.
4. Validate macOS/Homebrew acquisition and offline execution early, rather than treating packaging as a late release task.
5. Document whether each requested behavior is observed, degraded with diagnostics, unsupported, or failed. Do not infer support from an API surface alone.

## Capability Matrix and Success Criteria

Maintain one evidence-backed matrix per candidate/version. A blank or unmeasured cell is not support.

| Capability area | Evidence required for a pass | Minimum success criterion |
|---|---|---|
| Structural PDF operations | Merge, split, rotate, reorder, and selected-page deletion fixtures; metadata and page-effect inspection. | Requested page effects are correct, diagnostics identify resulting order/rotation/removed pages, and no operation is mislabeled as text deletion or redaction. |
| Conversion | DOCX, Markdown, and PDF fixtures with semantic, visual, and structural comparisons. | The candidate meets a declared fidelity tier for each supported route and reports known loss or unsupported content. |
| PDF editing and composition | Positioned, font, flattened, and unsupported fixtures; visual regression. | Only observed document/engine combinations are exposed; unsupported changes fail or diagnose clearly. |
| Annotations, forms, watermarks, visual signatures | Fixture-specific inspections and visual comparisons. | Output matches the declared non-cryptographic or document-limited semantics. |
| OCR | Pinned engine/language-data run on Spanish, English, and mixed scans. | Offline execution, recorded identity/resource use, and measured quality/limitations support the declared capability. |
| Redaction | Independent text extraction, search, object inspection, and raster checks. | All adversarial checks pass; any verifier failure is an operation failure. |
| Determinism | Repeated same-environment runs and output comparison. | DOCX packaging is deterministic where promised; PDF byte determinism is reported only when measured. |

## Evaluation Metrics and Rejection Gates

| Dimension | Measure | Reject or defer when |
|---|---|---|
| License | Candidate, runtime, transitive, and language-data obligations. | License is non-permissive under current policy, unclear, or cannot be inventoried. |
| macOS/Homebrew packaging | Reproducible acquisition, installation, offline invocation, binary/runtime behavior, package size. | Installation or offline execution is not reproducible, requires unapproved runtime handling, or cannot fit the Homebrew delivery model. |
| Visual and structural fidelity | Per-fixture visual render, semantic/text, metadata, page-tree, tag/form/annotation comparison as applicable. | A required operation corrupts, silently loses, or misreports behavior beyond its declared fidelity tier. |
| Performance and RAM | Elapsed time, peak memory when measurable, timeout/OOM behavior across corpus classes. | It cannot be bounded safely, routinely exceeds calibrated limits, or has unexplained severe regressions. |
| Failures and isolation | Crash, malformed-input, timeout, cancellation, and partial-output behavior. | A failure escapes the boundary, produces a partial published artifact, or lacks actionable classification/evidence. |
| Security redaction | Independent adversarial verification. | Any sensitive value remains recoverable by a required verifier. The redaction capability remains unavailable. |

Gates apply per capability, not only per engine. A candidate may remain viable for a narrow adapter while failing another operation. Do not compensate for a failed security, licensing, or packaging gate with aggregate scoring.

## Selection and ADR Control

ADR 0004 remains the controlling selection decision until evidence supports a specific candidate and version. Update it only after a reviewable evidence package includes the corpus revision, harness version, candidate/version, license inventory, macOS/Homebrew result, capability matrix, performance/RAM data, failure analysis, known limitations, and rollback criteria.

When accepted, pin the selected engine/runtime version and reproducible acquisition method in the ADR update and dependency configuration. Keep unsupported capability cells unavailable in the registry. If results are incomplete, a gate fails, or a candidate is upgraded, retain or restore the unselected state and run the affected evidence again; do not silently broaden the decision.

## Later Implementation and Security Work

After selection, implement only the proven structural operations and conversions first. Add editing, composition, annotations, forms, watermarks, visual signature insertion, and OCR one capability at a time behind registry declarations and regression evidence. A Python worker remains conditional on a separately documented measured advantage and must stay versioned, isolated, and free of business rules.

Build security redaction as a dedicated path, never as an overlay or ordinary text/page deletion. Generate output through the selected mutation path, then verify it through independent extraction, search, object-inspection, and raster-render checks. Store verifier identity and evidence in provenance. Do not expose redaction as successful when any required verification fails.

## Phase Checklists

### Foundation

- [ ] Rust workspace and lockfile are pinned to the accepted baseline.
- [ ] Contract, provenance, registry, I/O, and publication boundaries are defined before engine selection.
- [ ] Mock-adapter execution demonstrates that policy remains in Rust.

### Evidence Platform

- [ ] Corpus manifests identify fixtures, provenance, characteristics, and sensitivity.
- [ ] Harness reports are reproducible from a clean checkout and retain failed cases.
- [ ] Comparison methods are declared per operation and fidelity tier.

### Spikes and Selection

- [ ] Two or three permissively licensed candidates have comparable results.
- [ ] License, Homebrew/macOS, performance/RAM, failure, and fidelity evidence is recorded.
- [ ] Failed gates are visible; unmeasured capabilities are not presented as supported.
- [ ] ADR 0004 is updated and versions pinned only after the evidence review passes.

### Implementation and Release

- [ ] Production capabilities match the selected, evidence-backed matrix.
- [ ] Diagnostics and provenance describe applied strategy, limits, degradation, and verification evidence.
- [ ] Redaction passes independent adversarial checks before release.
- [ ] Homebrew packaging, offline execution, dependency audit/denial policy, SBOM, and license scans are release artifacts.

## References

- [Architecture handoff](README.md)
- [ADR 0002: Rust Core and Engine Adapter Boundary](adr/0002-rust-core-engine-adapter-boundary.md)
- [ADR 0003: Operation Contract and Capability Registry](adr/0003-operation-result-diagnostics-provenance-contract.md)
- [ADR 0004: PDF Engine Selection and Permissive Licensing](adr/0004-pdf-engine-selection-and-licensing.md)
- [ADR 0005: Fidelity and Deterministic Output Tiers](adr/0005-fidelity-and-deterministic-output-tiers.md)
- [ADR 0006: Safe I/O, Resource Policy, and Isolated Workers](adr/0006-safe-io-resource-policy-and-workers.md)
- [ADR 0008: Security Redaction and Independent Verification](adr/0008-security-redaction-verification.md)
- [ADR 0011: Versioning, Supply Chain, and Release Policy](adr/0011-versioning-supply-chain-release-policy.md)
