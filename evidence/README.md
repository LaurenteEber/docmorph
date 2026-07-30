# Retained Evidence Receipts

This directory retains deterministic receipts for the Phase 0 mock evidence scenarios. Review `success/receipt.json` first to confirm a published artifact, then review `policy-failure/receipt.json` to confirm that policy rejection records a diagnostic without an artifact.

## Governed reproduction

Run the workflow from a clean checkout. It requires the operation manifest, a new receipt directory for each run, the governed catalog, and the repository root.

```bash
first_receipt_dir="$(mktemp -d "${TMPDIR:-/tmp}/docmorph-reproduction-first.XXXXXX")"
second_receipt_dir="$(mktemp -d "${TMPDIR:-/tmp}/docmorph-reproduction-second.XXXXXX")"

cargo run -p docmorph-evidence --locked -- --manifest fixtures/evidence-success-manifest.json --receipt-dir "$first_receipt_dir" --catalog fixtures/corpus-manifest.json --repository-root .
cargo run -p docmorph-evidence --locked -- --manifest fixtures/evidence-success-manifest.json --receipt-dir "$second_receipt_dir" --catalog fixtures/corpus-manifest.json --repository-root .

python3 - "$first_receipt_dir/receipt.json" "$second_receipt_dir/receipt.json" evidence/success/receipt.json <<'PY'
import json
import sys

semantic_hashes = [json.load(open(path))["semantic_sha256"] for path in sys.argv[1:]]
assert semantic_hashes[0] == semantic_hashes[1] == semantic_hashes[2]
print(semantic_hashes[0])
PY
```

Use distinct, fresh receipt directories: an existing directory can contain an artifact from a previous run. The comparison proves that both governed runs have the same `semantic_sha256` as the retained success receipt.

Each successful receipt is schema `1.2`. Its `catalog_id` must be `docmorph-phase1-synthetic-smoke`, and its `catalog_revision_sha256` must match the canonical execution revision validated from `fixtures/corpus-manifest.json`. The retained receipt is the comparison baseline; validate the complete catalog before treating a run as governed.

This workflow asserts semantic-hash equality only. It does not claim byte equality, receipt or artifact path equality, or timestamp equality.

## Retention

| Receipt | Expected result | Retained content |
|---|---|---|
| `success/receipt.json` | `success` | Schema 1.2 receipt with catalog bindings, literal executable/argv, build compiler provenance, hashes, measured elapsed time, and `artifacts/success-output.mock`. |
| `policy-failure/receipt.json` | `failure` | Schema 1.2 receipt with catalog bindings, exact declared policy diagnostic, `fixture_sha256: null`, and `artifact: null`. No output artifact is retained. |

Keep each receipt with its referenced artifact for as long as the corresponding manifest is retained. Regenerate both after changing the evidence schema, manifest fields, mock behavior, contract version, or receipt semantics. Do not treat elapsed time as deterministic identity.

## Metric semantics

| Field | Meaning |
|---|---|
| `elapsed_milliseconds` | A measured wall-clock duration for this individual run; it is intentionally volatile. |
| `peak_memory_bytes` | `unavailable` with `peak_memory_metric_not_supported` when this harness cannot measure peak memory. It never substitutes an estimated or fabricated number. |
| `semantic_sha256` | A SHA-256 identity over deterministic contract, manifest, build compiler, platform, adapter, expected/actual outcomes, diagnostic codes, hashes, lengths, and metric availability. It excludes literal command argv, receipt/artifact paths, diagnostic messages, and elapsed time. |
