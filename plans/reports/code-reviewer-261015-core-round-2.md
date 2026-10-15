# Review: core/round-2 vs origin/main (Track A)

Three passes by the code-reviewer subagent (reviewer wrote findings in chat; saved here by the author).

| Pass | Commit | Score | Critical | Notes |
|---|---|---|---|---|
| 1 | baa46dd..0e92f8c | 7/10 | 0 | H1 close_peer killed healthy conns of other ALPNs and newer conns; H2 ffi-contract.md not updated (lead's file); M1 fragile retry state; M2 unbounded/silent history_item_reps pulls |
| 2 | a0149ef | 8.5/10 | 0 | close_stale scoped to same ALPN + older conns; still closed inbound conns; Instant sub could panic |
| 3 | c79f3f5 | 9/10 | 0 | direction filter; residual: Node::connect conns flagged outbound |
| final | 842a362 | 9.5/10 | 0 | Node::connect uses plain track; nothing blocking |

Open, not mine: docs/ffi-contract.md "Additive since v2" needs history_item_reps and EngineAlreadyRunning (appended last in the error enum, positional).
