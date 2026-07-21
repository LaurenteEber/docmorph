# Retained Evidence Receipts

This directory retains deterministic receipts for the Phase 0 mock evidence scenarios. Review `success/receipt.json` first to confirm a published artifact, then review `policy-failure/receipt.json` to confirm that policy rejection records a diagnostic without an artifact.

## Reproduce

```bash
cargo run -p docmorph-evidence --locked -- --manifest fixtures/evidence-success-manifest.json --receipt-dir evidence/success
cargo run -p docmorph-evidence --locked -- --manifest fixtures/evidence-policy-failure-manifest.json --receipt-dir evidence/policy-failure
```

## Retention

| Receipt | Expected result | Retained content |
|---|---|---|
| `success/receipt.json` | `success` | Receipt schema 1.1, literal executable/argv, build compiler provenance, hashes, measured elapsed time, and `artifacts/success-output.mock`. |
| `policy-failure/receipt.json` | `failure` | Receipt schema 1.1, exact declared policy diagnostic, `fixture_sha256: null`, and `artifact: null`. No output artifact is retained. |

Keep each receipt with its referenced artifact for as long as the corresponding manifest is retained. Regenerate both after changing the evidence schema, manifest fields, mock behavior, contract version, or receipt semantics. Do not treat elapsed time as deterministic identity.

## Metric semantics

| Field | Meaning |
|---|---|
| `elapsed_milliseconds` | A measured wall-clock duration for this individual run; it is intentionally volatile. |
| `peak_memory_bytes` | `unavailable` with `peak_memory_metric_not_supported` when this harness cannot measure peak memory. It never substitutes an estimated or fabricated number. |
| `semantic_sha256` | A SHA-256 identity over deterministic contract, manifest, build compiler, platform, adapter, expected/actual outcomes, diagnostic codes, hashes, lengths, and metric availability. It excludes literal command argv, receipt/artifact paths, diagnostic messages, and elapsed time. |
