use criterion::{criterion_group, criterion_main, Criterion};
use std::time::Duration;
use sfu_core::{MediaRouter, OutgoingPacket};
use bytes::Bytes;
use smallvec::SmallVec;
use std::sync::Arc;

fn bench_route_n(c: &mut Criterion, n: u32) {
    let router = MediaRouter::new();
    let pub_ssrc = 0x11223344;
    router.add_publisher(pub_ssrc);
    for i in 0..n {
        router.add_subscriber(pub_ssrc, 0xdead0000 + i);
    }
    let mut v = Vec::with_capacity(12 + 20);
    v.push(0x80);
    v.push(96);
    v.push(0);
    v.push(1);
    v.extend_from_slice(&1u32.to_be_bytes());
    v.extend_from_slice(&pub_ssrc.to_be_bytes());
    v.extend_from_slice(&[0u8; 20]);
    let pkt = Arc::new(Bytes::from(v));
    let mut out = SmallVec::<[OutgoingPacket; 16]>::new();
    let name = format!("route_{}_subs", n);
    c.bench_function(&name, |b| b.iter(|| { let _ = router.route(pkt.clone(), &mut out); }));
}

fn criterion_bench(c: &mut Criterion) {
    let conf = Criterion::default()
        .measurement_time(Duration::from_secs(10))
        .sample_size(100)
        .configure_from_args();
    *c = conf;
    bench_route_n(c, 1);
    bench_route_n(c, 10);
    bench_route_n(c, 100);
    c.final_summary();
}

criterion_group!(benches, criterion_bench);
criterion_main!(benches);
