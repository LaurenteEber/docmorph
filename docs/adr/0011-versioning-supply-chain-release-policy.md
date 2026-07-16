# ADR 0011: Versioning, Supply Chain, and Release Policy

## Status

Accepted

## Context

Native document engines and their runtimes make reproducibility, security, and license compliance release-critical.

## Decision

At project creation, use Rust stable 1.96.0, Rust 2024 edition, and `rust-version = "1.96"`. Commit `Cargo.lock`. Run stable and MSRV CI, Renovate or Dependabot, `cargo-audit`, `cargo-deny`, SBOM generation, and license scans. Update dependencies and engines only through controlled pull requests with validation; never through blind updates. Pin selected engine versions only after capability spikes.

## Consequences

- Toolchain and dependency changes are reviewable and reproducible.
- Supply-chain reports become release artifacts.
- Pre-1.0 candidates require deliberate compatibility scrutiny.

## Alternatives Considered

- Floating dependency/runtime versions: rejected because results and security posture become irreproducible.
- Uncommitted lockfile: rejected because CLI releases need repeatable resolution.
- Automatic unreviewed upgrades: rejected because document engines can change output, licensing, and security behavior.

## Validation/Exit Criteria

- CI enforces stable/MSRV builds, dependency audit, denial policy, SBOM, and license scanning.
- Release records identify Rust, dependency, engine, and runtime versions.
- Every upgrade PR includes applicable corpus, determinism, packaging, and security validation evidence.
