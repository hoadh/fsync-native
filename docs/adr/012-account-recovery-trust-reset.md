# ADR 012 — Account recovery = trust reset + re-pair
Status: accepted — validation S1. Implementation note (2026-10-14): the explicit trust-reset confirmation API (review item M5) is deferred; logout clears pinned trust.

If all devices are lost, the user logs in and creates a new seq-0 DeviceList (re-genesis). Old devices still holding the previous chain see the same account with a different root and show a "Trust reset" warning that requires explicit confirmation. No recovery key in MVP; history is local so nothing server-side is lost.
