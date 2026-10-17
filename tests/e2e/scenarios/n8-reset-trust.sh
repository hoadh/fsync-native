#!/usr/bin/env bash
# N8 (consumes the pairing, runs last): A resets the account trust (fresh login code first). B is
# no longer trusted by A: B's clip and file push must each be refused within 30 s with the typed
# refusal ("the other device removed this one", never delivered, never offline) and A receives nothing.
# This is also the "dial from a no-longer-paired NodeId" security check.
# One engine per data dir (engine.lock): A's receivers run one after the other, not together.
scenario_n8() {
  use_pair lan
  local dir="$WORK/round-n8"; mkdir -p "$dir/in"
  login A "$EMAIL" >/dev/null
  cli A account reset-trust --yes >"$dir/reset.out" 2>"$dir/reset.err" || fail "reset-trust: $(cat "$dir/reset.err")"

  cli A clip watch --json --count 1 --timeout 90 >"$dir/a-watch.out" 2>"$dir/a-watch.err" &
  local w=$!; track "$w"
  wait_for '"event":"ready"' "$dir/a-watch.out" 60
  # B does not know it was removed, so it keeps trying. Every put must be told it was refused
  # ("the other device removed this one", the typed refusal), never delivered and never "peer is offline".
  local t0 put_rc k clip_s=0
  for k in 1 2 3; do
    put_rc=0; t0=$SECONDS
    cli B clip put --text "from-removed-B-$k" --wait 5 >"$dir/b-put-$k.out" 2>"$dir/b-put-$k.err" || put_rc=$?
    [[ $put_rc -eq 0 ]] && fail "B's clip put $k was reported delivered after A reset trust (N8 regression): $(head -c 300 "$dir/b-put-$k.out")"
    grep -q "the other device removed this one" "$dir/b-put-$k.err" || fail "B's put $k: expected the refusal 'the other device removed this one', got: $(head -c 300 "$dir/b-put-$k.err")"
    [[ $((SECONDS - t0)) -gt $clip_s ]] && clip_s=$((SECONDS - t0))   # slowest single put
  done
  sleep 2   # late-delivery window for the negative check below (not a timing assertion)
  cli_kill A; wait "$w" 2>/dev/null || true
  grep -q '"event":"clip"' "$dir/a-watch.out" && fail "A received a clip from removed B"
  # the history query itself must succeed, otherwise "nothing found" proves nothing
  cli A history --json --query "from-removed-B" >"$dir/a-history.out" 2>"$dir/a-history.err" \
    || fail "A history query failed: $(head -c 300 "$dir/a-history.err")"
  grep -q "from-removed-B" "$dir/a-history.out" && fail "A stored a clip from removed B"

  dd if=/dev/urandom of="$dir/f.bin" bs=1024 count=64 2>/dev/null
  cli A fetch --out "$dir/in" --count 1 --timeout 40 >"$dir/a-fetch.out" 2>"$dir/a-fetch.err" &
  local f=$!; track "$f"
  wait_for '"event":"ready"' "$dir/a-fetch.out" 60
  t0=$SECONDS
  if cli B send devA "$dir/f.bin" --wait 20 >"$dir/b-send.out" 2>"$dir/b-send.err"; then
    fail "B's file push was accepted after trust reset"
  fi
  grep -q "the other device removed this one" "$dir/b-send.err" || fail "B's file push: expected the refusal 'the other device removed this one', got: $(head -c 300 "$dir/b-send.err")"
  local file_s=$((SECONDS - t0))
  cli_kill A; wait "$f" 2>/dev/null || true
  grep -q '"event":"fetched"' "$dir/a-fetch.out" && fail "A received a file from removed B"
  [[ -z "$(ls -A "$dir/in")" ]] || fail "files appeared in A's inbox"
  assert_lt "$clip_s" 30 "[n8] clip rejection time (s)"
  assert_lt "$file_s" 30 "[n8] file rejection time (s)"
  metric_add notes "N8: every B clip put (3 of 3) and the file push were refused with the typed error; A received and stored nothing"
  metric_add notes "slowest B clip put ${clip_s}s, file push refused in ${file_s}s"
}
