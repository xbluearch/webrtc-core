use sfu_core::{MediaRouter, OutgoingPacket};
use bytes::Bytes;
use smallvec::SmallVec;
use std::sync::Arc;
use crossbeam_channel::{unbounded, Receiver};

fn spawn_feedback_printer(rx: Receiver<(u32,u16)>) {
    std::thread::spawn(move || {
        while let Ok((ssrc, seq)) = rx.recv() {
            println!("TWCC from publisher {:#x}: {}", ssrc, seq);
        }
    });
}

fn main() {
    let extmap_lines = vec![(3u8, "http://www.ietf.org/id/draft-holmer-rmcat-transport-wide-cc-extensions-01")];
    let builder = sfu_core::rtp::ExtensionMapBuilder::new().with_pairs(extmap_lines);
    let extension_map = builder.build();
    let router = MediaRouter::new();
    let publisher_ssrc = 0xfeed_f00d;
    router.add_publisher_with_extensions(publisher_ssrc, extension_map);
    let (tx, rx) = unbounded();
    spawn_feedback_printer(rx);
    router.set_feedback_sender(publisher_ssrc, tx);
    router.add_subscriber(publisher_ssrc, 0xdead_beef);
    let mut packet = Vec::with_capacity(12 + 8);
    packet.push(0x90);
    packet.push(96);
    packet.extend_from_slice(&1u16.to_be_bytes());
    packet.extend_from_slice(&1u32.to_be_bytes());
    packet.extend_from_slice(&publisher_ssrc.to_be_bytes());
    packet.extend_from_slice(&[0xBE, 0xDE, 0x00, 0x01]);
    packet.extend_from_slice(&[(3 << 4) | (2 - 1), 0x01, 0x02, 0x00]);
    let packet = Arc::new(Bytes::from(packet));
    let mut out = SmallVec::<[OutgoingPacket; 16]>::new();
    let _ = router.route(packet, &mut out);
    println!("routed {} packets", out.len());
}
