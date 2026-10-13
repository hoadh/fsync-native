# ADR 004 — Serialization
Status: accepted

postcard (serde) for wire frames and the canonical signed DeviceList bytes — compact and deterministic for a given struct. JSON for HTTP APIs. Signed objects carry their exact encoded bytes (`body`) so verification never depends on re-encoding.
