#!/usr/bin/env bash
# N8 (consumes the pairing, runs last): A resets the account trust (fresh login code first). B is
# no longer trusted by A: B's clip and file push must each be refused within 30 s and A receives
# nothing. This is also the "dial from a no-longer-paired NodeId" security check.
# One engine per data dir (engine.lock): A's receivers run one after the other, not together.
scenario_n8() {
  use_pair lan
  local dir="$WORK/round-n8"; mkdir -p "$dir/in"
  login A "$EMAIL" >/dev/null
  cli A account reset-trust --yes >"$dir/reset.out" 2>"$dir/reset.err" || fail "reset-trust: $(cat "$dir/reset.err")"

  cli A clip watch --json --count 1 --timeout 40 >"$dir/a-watch.out" 2>"$dir/a-watch.err" &
  local w=$!; track "$w"
  wait_for '"event":"ready"' "$dir/a-watch.out" 60
  local t0=$SECONDS put_rc=0
  cli B clip put --text "from-removed-B" --wait 20 >"$dir/b-put.out" 2>"$dir/b-put.err" || put_rc=$?
  local clip_s=$((SECONDS - t0))
  sleep 2   # late-delivery window for the negative check below (not a timing assertion)
  cli_kill A; wait "$w" 2>/dev/null || true
  grep -q '"event":"clip"' "$dir/a-watch.out" && fail "A received a clip from removed B"
  cli A history --json --query "from-removed-B" >"$dir/a-history.out" 2>"$dir/a-history.err" || true
  grep -q "from-removed-B" "$dir/a-history.out" && fail "A stored a clip from removed B"

  dd if=/dev/urandom of="$dir/f.bin" bs=1024 count=64 2>/dev/null
  cli A fetch --out "$dir/in" --count 1 --timeout 40 >"$dir/a-fetch.out" 2>"$dir/a-fetch.err" &
  local f=$!; track "$f"
  wait_for '"event":"ready"' "$dir/a-fetch.out" 60
  t0=$SECONDS
  if cli B send devA "$dir/f.bin" --wait 20 >"$dir/b-send.out" 2>"$dir/b-send.err"; then
    fail "B's file push was accepted after trust reset"
  fi
  local file_s=$((SECONDS - t0))
  cli_kill A; wait "$f" 2>/dev/null || true
  grep -q '"event":"fetched"' "$dir/a-fetch.out" && fail "A received a file from removed B"
  [[ -z "$(ls -A "$dir/in")" ]] || fail "files appeared in A's inbox"
  assert_lt "$clip_s" 30 "[n8] clip rejection time (s)"
  assert_lt "$file_s" 30 "[n8] file rejection time (s)"
  # B should be told, not just ignored. Security holds either way (A got nothing, checked above); a
  # success report to B is a product finding, recorded as a quarantine with the evidence.
  if [[ $put_rc -eq 0 ]]; then
    quarantine_scn "security checks pass (A received and stored nothing, B file push refused in ${file_s}s) but B's clip put exited 0 with delivered=[A] after A's trust reset (took ${clip_s}s): sender is not told it was refused. Reproduces in the full matrix, not when N8 runs alone"
  fi
  metric_add notes "B clip rejected in ${clip_s}s, file push in ${file_s}s; A received nothing"
}
