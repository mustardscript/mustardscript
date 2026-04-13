# Benchmark Findings

This document summarizes the latest checked-in benchmark evidence from:

- workload suite: `benchmarks/results/2026-04-13T19-50-59-853Z-workloads.json`
- release smoke suite: `benchmarks/results/2026-04-13T19-52-38-896Z-smoke-release.json`
- dev smoke suite: `benchmarks/results/2026-04-13T19-52-38-994Z-smoke-dev.json`

Machine and environment:

- CPU: Apple M4, 10 cores
- OS: `darwin 25.2.0`
- Arch: `arm64`
- Node: `v24.12.0`
- Git SHA in artifacts: `cbf9093`
- Workload fixture version: `5`
- Smoke fixture version: `2`

## Headline Results

### 1. The sidecar now reuses compiled programs inside a live session through `program_id`

`mustard-sidecar` no longer requires every `start` request to resend and
re-decode the full compiled program blob after a successful `compile`.

The current protocol now does both of the following:

- `compile` returns `program_base64` plus a session-local `program_id`
- `start` accepts that cached `program_id` and reuses the shared validated
  program inside the same sidecar process

The protocol and lifecycle contract were updated in
`docs/SIDECAR_PROTOCOL.md`, and the new path is covered by:

- Rust protocol-state coverage for cached `program_id` starts
- real-process sidecar protocol tests
- Node sidecar equivalence tests
- package smoke coverage for the published sidecar binary

### 2. The workload impact is targeted and strongest on larger warm starts

Compared with the previously checked-in release workload artifact
`2026-04-13T19-39-37-535Z-workloads.json`, current sidecar medians moved by:

| Workload | Previous | Current | Delta |
| --- | ---: | ---: | ---: |
| `warm_run_code_mode_search` | `1.41 ms` | `0.86 ms` | `-38.9%` |
| `host_fanout_1` | `0.14 ms` | `0.11 ms` | `-20.4%` |
| `host_fanout_100` | `6.76 ms` | `6.68 ms` | `-1.1%` |
| `programmatic_tool_workflow` | `18.91 ms` | `18.81 ms` | `-0.5%` |
| `suspend_resume_20` | `1.33 ms` | `1.26 ms` | `-4.9%` |
| `warm_run_small` | `1.42 ms` | `1.45 ms` | `+2.1%` |
| `cold_start_small` | `6.82 ms` | `8.22 ms` | `+20.5%` |
| `cold_start_code_mode_search` | `5.44 ms` | `5.78 ms` | `+6.4%` |

This is a real but narrow win:

- the large warm code-mode fixture benefits substantially because the sidecar
  no longer re-deserializes that larger program on every `start`
- host-fanout and workflow surfaces move in the right direction, but only
  modestly, because repeated protocol crossings still dominate those paths
- tiny warm starts are effectively flat, and cold paths remain noisy and still
  pay compile-plus-process-start cost

### 3. The sidecar phase metrics still point at the remaining bottlenecks

Current sidecar phase medians:

| Sidecar Phase | Median | p95 |
| --- | ---: | ---: |
| `startup_only` | `8.29 ms` | `265.20 ms` |
| `execution_only_small` | `1.45 ms` | `1.60 ms` |
| `transport_resume_only` | `0.11 ms` | `0.13 ms` |

Those numbers reinforce the current diagnosis:

- startup variance is still a major cold-path cost
- cached compiled-program reuse helps a larger warm start materially, but it
  does not by itself close the sidecar gap on host-heavy workloads
- minimal authenticated resume transport is still cheap relative to workflow and
  large fanout totals, so the next sidecar gains likely still require protocol
  shape changes and more session-side state reuse

### 4. Smoke budgets still pass on the refreshed artifact set

Release smoke medians:

| Metric | Current |
| --- | ---: |
| Startup | `0.03 ms` |
| Compute | `0.26 ms` |
| Host-call median ratio | `4.44x` |
| Host-call p95 ratio | `4.93x` |
| Snapshot median ratio | `7.04x` |
| Snapshot p95 ratio | `5.02x` |

Dev smoke also stayed inside budget with startup `0.14 ms`, compute
`1.85 ms`, host-call median ratio `3.59x`, and snapshot median ratio `9.64x`.

## Conclusions

1. The Milestone 7 compiled-program reuse item is now closed. Sidecar sessions
   keep a bounded program cache and `start` can reuse `program_id` instead of
   re-decoding the compiled blob every time.
2. The benchmark evidence is good enough to keep, but it is not a universal
   sidecar win. The strongest improvement is `warm_run_code_mode_search
   -38.9%`; workflow and large host fanout move only slightly; cold paths stay
   noisy or worse.
3. The next open sidecar work is still the bigger transport redesign:
   binary framing, binary snapshot/program transport, cached `snapshot_id` /
   compact policy metadata, and the protocol-hardening changes required to land
   those safely.
