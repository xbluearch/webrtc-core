SIMULATED CONVENTIONAL COMMITS (for PR history)

feat: implement zero-copy RTP parsing and header-extension mapping

feat: add lock-free subscriber routing via arc-swap

feat: typed TWCC extension reader and ExtensionMapBuilder (SDP-driven)

perf: zero-allocation hot path; use Arc<Bytes> and SmallVec for headers

perf: add bench harness and percentile runner for latency distributions

test: add deterministic concurrency tests and Miri verification

ci: add GitHub Actions workflow (check, test, clippy)

docs: add Technical Manifesto (README) and AUDIT with Miri results

chore: polish examples and add bench/percentiles runner
