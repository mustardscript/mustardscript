# Benchmark Findings

This document summarizes the latest checked-in benchmark evidence from:

- workload suite: `benchmarks/results/2026-04-13T20-03-16-697Z-workloads.json`
- release smoke suite: `benchmarks/results/2026-04-13T20-03-39-876Z-smoke-release.json`
- dev smoke suite: `benchmarks/results/2026-04-13T20-03-42-585Z-smoke-dev.json`

Machine and environment:

- CPU: Apple M4, 10 cores
- OS: `darwin 25.2.0`
- Arch: `arm64`
- Node: `v24.12.0`
- Git SHA in artifacts: `a7ae162`
- Workload fixture version: `5`
- Smoke fixture version: `2`

## Headline Results

### 1. The sidecar now reuses compiled programs, suspended snapshots, and policy metadata inside a live session

`mustard-sidecar` no longer requires the hot path to resend every large opaque
blob on each resume.

The current protocol now does both of the following:

- `compile` returns `program_base64` plus a session-local `program_id`
- `start` accepts that cached `program_id` and reuses the shared validated
  program inside the same sidecar process
- suspended steps now return `snapshot_id` plus a session-local `policy_id`
- `resume` can use cached `snapshot_id` / `policy_id` handles plus fresh auth
  metadata instead of resending full `snapshot_base64` and the full
  capability/limits policy on every hop

The protocol and lifecycle contract were updated in
`docs/SIDECAR_PROTOCOL.md`, and the new path is covered by:

- Rust protocol-state coverage for cached `program_id` starts
- Rust protocol-state coverage for cached `snapshot_id` / `policy_id` resumes
- real-process sidecar protocol tests
- Node sidecar equivalence tests
- package smoke coverage for the published sidecar binary

### 2. The workload impact is now broader on hot sidecar paths

Compared with the previously checked-in release workload artifact
`2026-04-13T19-50-59-853Z-workloads.json`, current sidecar medians moved by:

| Workload | Previous | Current | Delta |
| --- | ---: | ---: | ---: |
| `warm_run_small` | `1.45 ms` | `1.39 ms` | `-4.4%` |
| `warm_run_code_mode_search` | `0.86 ms` | `0.82 ms` | `-5.0%` |
| `programmatic_tool_workflow` | `18.81 ms` | `17.20 ms` | `-8.6%` |
| `host_fanout_50` | `3.67 ms` | `3.46 ms` | `-5.7%` |
| `host_fanout_100` | `6.68 ms` | `6.41 ms` | `-4.0%` |
| `suspend_resume_20` | `1.26 ms` | `1.19 ms` | `-5.6%` |
| `cold_start_small` | `8.22 ms` | `6.17 ms` | `-24.9%` |
| `cold_start_code_mode_search` | `5.78 ms` | `6.28 ms` | `+8.6%` |

This is now a meaningful hot-path win even though the cold side still stays
noisy:

- the best direct signal is the larger workflow and host-fanout surfaces, where
  skipping repeated snapshot/policy resend cuts real cross-process overhead
- the new cached resume path also improves the sidecar-only
  `transport_resume_only` phase directly
- cold startup is still dominated by process and environment variance rather
  than hot request framing alone, and `cold_start_code_mode_search` stayed worse
  on this run

### 3. The sidecar phase metrics still point at the remaining bottlenecks

Current sidecar phase medians:

| Sidecar Phase | Median | p95 |
| --- | ---: | ---: |
| `startup_only` | `9.45 ms` | `268.73 ms` |
| `execution_only_small` | `1.39 ms` | `1.53 ms` |
| `transport_resume_only` | `0.10 ms` | `0.12 ms` |

Those numbers reinforce the current diagnosis:

- startup variance is still a major cold-path cost
- the cached resume path is now materially cheaper than before, but the
  sidecar/addon gap is still much larger than `1.20x` on workflow and large
  fanout workloads
- the next sidecar gains still likely require the remaining wire-shape work:
  binary framing, binary program/snapshot transport, and stronger protocol
  hardening around those new message forms

### 4. Smoke budgets still pass on the refreshed artifact set

Release smoke medians:

| Metric | Current |
| --- | ---: |
| Startup | `0.05 ms` |
| Compute | `0.38 ms` |
| Host-call median ratio | `4.40x` |
| Host-call p95 ratio | `4.92x` |
| Snapshot median ratio | `6.83x` |
| Snapshot p95 ratio | `4.11x` |

Dev smoke also stayed inside budget with startup `0.28 ms`, compute
`2.46 ms`, host-call median ratio `3.28x`, and snapshot median ratio `8.49x`.

## Conclusions

1. The Milestone 7 session-statefulness item is now materially farther along:
   sidecar sessions reuse `program_id`, `snapshot_id`, and `policy_id` inside a
   live process, and hot resumes no longer need to resend the same full blobs.
2. The benchmark evidence is good enough to keep. The strongest wins are on
   sidecar `programmatic_tool_workflow -8.6%`, `transport_resume_only -12.1%`,
   `suspend_resume_20 -5.6%`, `host_fanout_100 -4.0%`, and `warm_run_small
   -4.4%` versus the prior checked-in sidecar baseline.
3. The remaining open sidecar work is still the bigger transport redesign:
   binary framing, keeping program/snapshot bytes binary end to end, and the
   protocol-hardening/versioning work needed to land that safely.
