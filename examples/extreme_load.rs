use std::sync::Arc;
use bytes::Bytes;
use std::time::Instant;
use std::hint::black_box;
use sfu_core::{MediaRouter, OutgoingPacket};
use smallvec::SmallVec;

#[inline]
fn try_rdtsc() -> Option<u64> {
    // Only attempt runtime detection on non-MSVC x86_64 targets (CI uses Linux).
    #[cfg(all(target_arch = "x86_64", not(target_env = "msvc")))]
    {
        if is_x86_feature_detected!("rdtscp") {
            unsafe {
                let mut aux: u32 = 0;
                return Some(core::arch::x86_64::_rdtscp(&mut aux));
            }
        } else if is_x86_feature_detected!("rdtsc") {
            unsafe { return Some(core::arch::x86_64::_rdtsc()); }
        }
        None
    }
    #[cfg(not(all(target_arch = "x86_64", not(target_env = "msvc"))))]
    {
        None
    }
}

fn make_rtp(ssrc: u32, seq: u16, ts: u32, payload: &[u8]) -> Arc<Bytes> {
    let mut v = Vec::with_capacity(12 + payload.len());
    v.push(0x80);
    v.push(96);
    v.push((seq >> 8) as u8);
    v.push((seq & 0xff) as u8);
    v.extend_from_slice(&ts.to_be_bytes());
    v.extend_from_slice(&ssrc.to_be_bytes());
    v.extend_from_slice(payload);
    Arc::new(Bytes::from(v))
}

fn run_load(subscribers: usize, iterations: usize) {
    let router = MediaRouter::new();
    let pub_ssrc = 0x1234_5678u32;
    router.add_publisher(pub_ssrc);
    for i in 0..subscribers {
        router.add_subscriber(pub_ssrc, 0x8000_0000u32 + (i as u32));
    }
    let pkt = make_rtp(pub_ssrc, 1, 1000, b"payload");
    // warm-up
    for _ in 0..10_000 { let mut out = SmallVec::<[OutgoingPacket; 16]>::new(); let _ = router.route(pkt.clone(), &mut out); }

    let start_cycles = try_rdtsc();
    let start = Instant::now();
    for i in 0..iterations {
        black_box({
            let mut out = SmallVec::<[OutgoingPacket; 16]>::new();
            let _ = router.route(pkt.clone(), &mut out);
            out
        });
        if i % 1_000_000 == 0 { std::hint::black_box(i); }
    }
    let elapsed = start.elapsed();
    let end_cycles = try_rdtsc();

    let total_packets = iterations as f64;
    let secs = elapsed.as_secs_f64();
    let pps = if secs > 0.0 { total_packets / secs } else { 0.0 };

    if let (Some(s), Some(e)) = (start_cycles, end_cycles) {
        let total_cycles = e.saturating_sub(s);
        let cycles_per_packet = if total_packets > 0.0 { (total_cycles as f64) / total_packets } else { 0.0 };
        println!("Subscribers: {} | Total Packets: {} | Total Time: {:?} | Cycles/Packet: {:.2} | Throughput (PPS): {:.2}",
            subscribers, iterations, elapsed, cycles_per_packet, pps);
    } else {
        // Fallback: report nanoseconds per packet when cycle counter unavailable
        let ns_per_packet = if total_packets > 0.0 { (secs * 1e9) / total_packets } else { 0.0 };
        println!("Subscribers: {} | Total Packets: {} | Total Time: {:?} | ns/Packet: {:.2} | Throughput (PPS): {:.2}",
            subscribers, iterations, elapsed, ns_per_packet, pps);
    }
}

fn main() {
    // Use a moderate scenario so CI completes in reasonable time
    let scenarios = vec![(1usize, 5_000_000usize), (100, 2_000_000), (1000, 500_000)];
    for (subs, iters) in scenarios {
        println!("-- Running scenario: {} subscribers, {} iterations --", subs, iters);
        run_load(subs, iters);
    }
}
