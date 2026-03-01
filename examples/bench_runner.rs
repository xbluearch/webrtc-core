use sfu_core::MediaRouter;
use bytes::Bytes;
use smallvec::SmallVec;
use sfu_core::OutgoingPacket;
use std::sync::Arc;
use std::time::Instant;

fn run_n(n: u32, iters: usize) -> f64 {
    let router = MediaRouter::new();
    let pub_ssrc = 0x11223344;
    router.add_publisher(pub_ssrc);
    for i in 0..n { router.add_subscriber(pub_ssrc, 0xdead0000 + i); }
    let mut v = Vec::with_capacity(12 + 20);
    v.push(0x80); v.push(96); v.push(0); v.push(1);
    v.extend_from_slice(&1u32.to_be_bytes());
    v.extend_from_slice(&pub_ssrc.to_be_bytes());
    v.extend_from_slice(&[0u8; 20]);
    let pkt = Arc::new(Bytes::from(v));
    let mut out = SmallVec::<[OutgoingPacket; 16]>::new();
    let start = Instant::now();
    for _ in 0..iters { let _ = router.route(pkt.clone(), &mut out); }
    let dur = start.elapsed();
    let ns = dur.as_nanos() as f64 / (iters as f64);
    ns
}

fn main() {
    let n1 = run_n(1, 200_000);
    println!("mean ns per route (1 sub): {:.2}", n1);
    let n10 = run_n(10, 100_000);
    println!("mean ns per route (10 subs): {:.2}", n10);
    let n100 = run_n(100, 50_000);
    println!("mean ns per route (100 subs): {:.2}", n100);
}
