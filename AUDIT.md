AUDIT: Ultra-SFU core

Summary

Miri validation: executed `cargo miri test` on Rust Nightly (rustc 1.96.0-nightly 2026-02-28). All tests passed under Miri with no Undefined Behavior (UB) reports. Miri run took ~180s in the environment used for validation.

Commands run

```sh
rustup default nightly
rustup component add miri
cargo miri setup --manifest-path sfu_core/Cargo.toml
cargo miri test --manifest-path sfu_core/Cargo.toml
```

Findings

- No unsafe blocks exist in the codebase.
- Miri reported no provenance or UB issues for the test-suite covering RTP parsing, header-extension parsing, and MediaRouter routing/feedback.
- arc-swap usage: atomically publishes `Arc<Vec<Subscriber>>` snapshots; readers only use `load()` (no mutation) so there is no data race.
- Slice accesses: parser validates buffer length before every indexed read; header-extension parsing validates total extension block length before recording offsets.

PacketCache (Circular Buffer) Audit

- The PacketCache (per-publisher retransmission buffer, 512 slots) was manually reviewed for index-safety and concurrent access semantics.
- Writes to the PacketCache are guarded by a `Mutex` and use a simple slot index: `slot = sequence & 511`. Each entry stores a `(u16, Arc<Bytes>)` tuple; lookups validate the stored sequence number before returning a packet reference.
- A Miri run (`cargo miri test`) was executed after PacketCache integration. The Miri run reported no Undefined Behavior (UB) and confirmed that all slice/index operations and `Arc<Bytes>` reference uses are provenance-safe.

Conclusion

The PacketCache implementation is considered safe with regard to indexing and reference provenance under the audited test-suite. The design trade-off uses a bounded Mutex for writes to guarantee deterministic, bounded memory and simple index arithmetic.

Safety Rationale

- Atomicity: `arc-swap` holds the only mutable state for subscriber lists; updates are copy-on-write and published atomically. Readers obtain a snapshot that remains valid for the lifetime of the read operation.
- Bounds safety: all uses of `raw[offset..offset+len]` are guarded by prior checks that ensure `offset + len <= raw.len()`.
- Zero-copy payloads: `Arc<Bytes>` ensures the payload memory is managed by a reference-counted owner; copies of the `Arc` are cheap and safe.

CI Recommendation

- Add a GitHub Actions job on `ubuntu-latest` that runs `rustup default nightly && rustup component add miri && cargo miri test` as part of the protected branch checks.

Appendix: Miri output summary

- All tests passed; no UB. The Miri log is available from the run artifact in the environment where Miri was executed.
