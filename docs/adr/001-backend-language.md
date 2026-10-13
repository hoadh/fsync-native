# ADR 001 — Backend language: Rust + axum + sqlx + Postgres
Status: accepted (2026-10-13)

## Decision
Backend is a Rust axum service using sqlx against Postgres, in the same Cargo workspace as core.

## Rationale
- Shares `fsync-proto` with core: server-side DeviceList chain verification and HTTP DTOs cannot drift.
- One toolchain for core and backend owners; small memory footprint for 10k idle WebSockets.
