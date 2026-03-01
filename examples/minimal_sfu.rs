use sfu_core::{MediaRouter, OutgoingPacket};
use bytes::Bytes;
use smallvec::SmallVec;
use std::sync::Arc;

fn main() {
    let router = MediaRouter::new();
    let pub_ssrc = 0x11223344;
    router.add_publisher(pub_ssrc);
    router.add_subscriber(pub_ssrc, 0xdeadbeef);
    let mut buf = Vec::with_capacity(12 + 4);
    buf.push(0x80);
    buf.push(96);
    buf.push(0);
    buf.push(1);
    buf.extend_from_slice(&12345u32.to_be_bytes());
    buf.extend_from_slice(&pub_ssrc.to_be_bytes());
    buf.extend_from_slice(&[1,2,3,4]);
    let pkt = Arc::new(Bytes::from(buf));
    let mut out = SmallVec::<[OutgoingPacket; 16]>::new();
    router.route(pkt, &mut out).unwrap();
    println!("routed {} packets", out.len());
}
