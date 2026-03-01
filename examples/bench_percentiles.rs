use sfu_core::{MediaRouter, OutgoingPacket};
use bytes::Bytes;
use smallvec::SmallVec;
use std::sync::Arc;
use std::time::Instant;

fn run_collect(n: u32, iters: usize) -> Vec<f64> {
    let router = MediaRouter::new();
    let pub_ssrc = 0x11223344;
    router.add_publisher(pub_ssrc);
    for i in 0..n { router.add_subscriber(pub_ssrc, 0xdead0000 + i); }
    let mut v = Vec::with_capacity(12 + 20);
    v.push(0x80); v.push(96); v.push(0); v.push(1);
    v.extend_from_slice(&1u32.to_be_bytes()); v.extend_from_slice(&pub_ssrc.to_be_bytes());
    v.extend_from_slice(&[0u8; 20]);
    let pkt = Arc::new(Bytes::from(v));
    let mut out = SmallVec::<[OutgoingPacket; 16]>::new();
    let mut samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let start = Instant::now();
        let _ = router.route(pkt.clone(), &mut out);
        let dt = start.elapsed();
        samples.push(dt.as_nanos() as f64);
    }
    samples
}

fn percentile(mut v: Vec<f64>, p: f64) -> f64 {
    v.sort_by(|a,b| a.partial_cmp(b).unwrap());
    let idx = ((p/100.0) * (v.len() as f64 - 1.0)).round() as usize;
    v[idx]
}

fn summarize(n: u32, iters: usize) {
    let samples = run_collect(n, iters);
    let mean = samples.iter().sum::<f64>() / (samples.len() as f64);
    let p95 = percentile(samples.clone(), 95.0);
    let p99 = percentile(samples.clone(), 99.0);
    let p999 = percentile(samples, 99.9);
    println!("subs={}: mean_ns={:.2} p95_ns={:.2} p99_ns={:.2} p99.9_ns={:.2}", n, mean, p95, p99, p999);
}

fn main() {
    summarize(1, 200_000);
    summarize(10, 100_000);
    summarize(100, 50_000);
}
