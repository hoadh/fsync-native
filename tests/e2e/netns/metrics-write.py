"""Assemble the netns matrix metrics JSON from the per-scenario files the runner leaves in WORK.

usage: metrics-write.py WORK OUT RUN_ID GIT_SHA MIB DURATION_S BUILD_S [STRICT]
Inputs: WORK/rows (id, result, gating, secs; tab separated), WORK/m-<id>.tsv (key, value rows).
"""
import json
import os
import sys

BYTES = {"bytes_relay": "relay", "bytes_wan_a": "wan_a", "bytes_wan_b": "wan_b", "bytes_host_a": "host_a"}
NUMERIC = {"clip_latency_s", "file_secs", "mib_s"}


def num(v):
    try:
        return float(v)
    except ValueError:
        return None


def scenario(work, sid, result, gating, secs):
    s = {"id": sid, "result": result, "gating": gating == "1", "secs": int(secs), "path_expected": None,
         "path_observed": None, "bytes": {}, "clip_latency_s": None, "file_secs": None, "mib_s": None,
         "sha_ok": None, "cli_path": None, "cli_path_tier": None, "notes": []}
    path = os.path.join(work, f"m-{sid}.tsv")
    for line in open(path) if os.path.exists(path) else []:
        k, _, v = line.rstrip("\n").partition("\t")
        if k in BYTES:
            s["bytes"][BYTES[k]] = int(v)
        elif k in NUMERIC:
            s[k] = num(v)
        elif k == "sha_ok":
            s[k] = v == "true"
        elif k == "notes":
            s["notes"].append(v)
        else:
            s[k] = v
    if result in ("SKIP", "QUARANTINED") and not s["notes"]:
        raise SystemExit(f"{sid}: {result} without a reason")
    s["notes"] = "; ".join(s["notes"])
    return s


def main():
    work, out, run_id, sha, mib, dur, build = sys.argv[1:8]
    strict = len(sys.argv) > 8 and sys.argv[8] == "1"
    rows = [l.rstrip("\n").split("\t") for l in open(os.path.join(work, "rows")) if l.strip()]
    doc = {"run_id": run_id, "git_sha": sha, "mib": int(mib),
           "docker": {"cpus": os.cpu_count(), "mem_gb": round(int(open("/proc/meminfo").readline().split()[1]) / 1048576, 1)},
           "duration_s": int(dur), "build_s": int(build),
           "strict": strict, "scenarios": [scenario(work, *r) for r in rows]}
    res = [s["result"] for s in doc["scenarios"]]
    doc["counts"] = {k: res.count(k) for k in ("PASS", "FAIL", "QUARANTINED", "SKIP")}
    doc["quarantined_gating"] = [s["id"] for s in doc["scenarios"] if s["result"] == "QUARANTINED" and s["gating"]]
    with open(out, "w") as f:
        json.dump(doc, f, indent=2)
    print(f"metrics written: {out}")


main()
