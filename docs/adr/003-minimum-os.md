# ADR 003 — Minimum OS versions
Status: accepted — confirmed in validation S1. Amended 2026-10-19 (macOS floor raised to 14).

macOS 14+ (was 13+), Windows 10 22H2+ and Windows 11, x64 + ARM64. Win10 is kept for reach; the cost is a larger test matrix (phase 15).

## Amendment 2026-10-19: macOS 14+
- Why: desktop UI redesign needs `openSettings`, `ContentUnavailableView`, `symbolEffect` (all macOS 14). `MenuBarExtra` (13) is still used.
- Windows floor unchanged.
- Consequence: macOS 13 users stay on the last macOS 13-compatible build; no further updates for them. Release note in plan phase 12.
- Newer-OS visuals (e.g. Liquid Glass) are adopted automatically by system controls only; no custom use.
