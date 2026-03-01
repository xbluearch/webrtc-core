use crate::rtp::{Error as RtpError, RtpPacket};
use bytes::Bytes;
use arc_swap::ArcSwap;
use parking_lot::{RwLock, Mutex};
use smallvec::SmallVec;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

#[derive(Debug)]
pub enum Error {
    Rtp(RtpError),
    UnknownPublisher,
}

impl From<RtpError> for Error {
    fn from(e: RtpError) -> Self {
        Error::Rtp(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use std::sync::Arc;
    use std::thread;

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

    #[test]
    fn concurrent_add_subscriber_and_route() {
        let router = Arc::new(MediaRouter::new());
        let pub_ssrc = 0x11223344;
        router.add_publisher(pub_ssrc);
        let router_clone = router.clone();
        let handle = thread::spawn(move || {
            for i in 0..1000u16 {
                let pkt = make_rtp(pub_ssrc, i, i as u32 * 3000, b"payload");
                let mut out = SmallVec::<[OutgoingPacket; 16]>::new();
                let res = router_clone.route(pkt, &mut out);
                assert!(res.is_ok());
            }
        });
        for i in 0..10u32 {
            router.add_subscriber(pub_ssrc, 0xdead0000 + i);
        }
        handle.join().unwrap();
        let pkt = make_rtp(pub_ssrc, 42, 42000, b"end");
        let mut out = SmallVec::<[OutgoingPacket; 16]>::new();
        router.route(pkt, &mut out).unwrap();
        assert!(!out.is_empty());
    }

    struct CounterCallback {
        cnt: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    impl FeedbackLoop for CounterCallback {
        fn on_twcc(&self, _publisher_ssrc: u32, _seq: u16) { self.cnt.fetch_add(1, std::sync::atomic::Ordering::SeqCst); }
        fn on_pli(&self, _publisher_ssrc: u32) { }
    }

    #[test]
    fn feedback_callback_invoked() {
        let router = Arc::new(MediaRouter::new());
        let pub_ssrc = 0x5555_6666;
        let mut builder = crate::rtp::ExtensionMapBuilder::new();
        builder = builder.with_pairs(vec![(3u8, "urn:ietf:params:rtp-hdrext:transport-wide-cc")]);
        let map = builder.build();
        router.add_publisher_with_extensions(pub_ssrc, map);
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let cb = Arc::new(CounterCallback { cnt: counter.clone() });
        router.set_feedback_callback(pub_ssrc, cb);
        router.add_subscriber(pub_ssrc, 0x1111);
        let mut pkt = Vec::with_capacity(12 + 4 + 4);
        pkt.push(0x90);
        pkt.push(96);
        pkt.extend_from_slice(&1u16.to_be_bytes());
        pkt.extend_from_slice(&1u32.to_be_bytes());
        pkt.extend_from_slice(&pub_ssrc.to_be_bytes());
        pkt.push(0xBE); pkt.push(0xDE); pkt.push(0); pkt.push(1);
        pkt.push((3 << 4) | (2 - 1)); pkt.push(0x12); pkt.push(0x34); pkt.push(0);
        let pkt = Arc::new(Bytes::from(pkt));
        let mut out = SmallVec::<[OutgoingPacket; 16]>::new();
        router.route(pkt, &mut out).unwrap();
        assert!(counter.load(std::sync::atomic::Ordering::SeqCst) >= 1);
    }
}

pub struct OutgoingPacket {
    pub header: SmallVec<[u8; 64]>,
    pub payload: Arc<Bytes>,
    pub payload_range: Range<usize>,
    pub dest_ssrc: u32,
    pub transport_cc: Option<u16>,
}

pub trait FeedbackLoop: Send + Sync {
    fn on_twcc(&self, publisher_ssrc: u32, seq: u16);
    fn on_pli(&self, publisher_ssrc: u32);
}

#[derive(Clone)]
struct Subscriber {
    rewrite_ssrc: u32,
    last_in_seq: Option<u16>,
    last_out_seq: Option<u16>,
    last_in_ts: Option<u32>,
    last_out_ts: Option<u32>,
}

struct PublisherState {
    subscribers: ArcSwap<Vec<Subscriber>>,
    extensions: crate::rtp::ExtensionMap,
    feedback_cb: Option<Arc<dyn FeedbackLoop>>,
    feedback_sender: Option<crossbeam_channel::Sender<(u32,u16)>>,
    pli_sender: Option<crossbeam_channel::Sender<u32>>,
    retrans_cache: Mutex<Vec<Option<(u16, Arc<Bytes>)>>>,
}

pub struct MediaRouter {
    inner: RwLock<HashMap<u32, PublisherState>>,
}

impl MediaRouter {
    pub fn new() -> Self {
        MediaRouter {
            inner: RwLock::new(HashMap::new()),
        }
    }

    pub fn add_publisher(&self, ssrc: u32) {
        let mut w = self.inner.write();
        w.entry(ssrc).or_insert_with(|| PublisherState { subscribers: ArcSwap::from_pointee(Vec::new()), extensions: crate::rtp::ExtensionMap::new(), feedback_cb: None, feedback_sender: None, pli_sender: None, retrans_cache: Mutex::new(vec![None; 512]) });
    }

    pub fn add_publisher_with_extensions(&self, ssrc: u32, map: crate::rtp::ExtensionMap) {
        let mut w = self.inner.write();
        w.entry(ssrc).or_insert_with(|| PublisherState { subscribers: ArcSwap::from_pointee(Vec::new()), extensions: map, feedback_cb: None, feedback_sender: None, pli_sender: None, retrans_cache: Mutex::new(vec![None; 512]) });
    }

    pub fn set_feedback_callback(&self, ssrc: u32, cb: Arc<dyn FeedbackLoop>) {
        let mut w = self.inner.write();
        if let Some(pubstate) = w.get_mut(&ssrc) { pubstate.feedback_cb = Some(cb); }
    }

    pub fn set_feedback_sender(&self, ssrc: u32, sender: crossbeam_channel::Sender<(u32,u16)>) {
        let mut w = self.inner.write();
        if let Some(pubstate) = w.get_mut(&ssrc) { pubstate.feedback_sender = Some(sender); }
    }

    pub fn set_pli_sender(&self, ssrc: u32, sender: crossbeam_channel::Sender<u32>) {
        let mut w = self.inner.write();
        if let Some(pubstate) = w.get_mut(&ssrc) { pubstate.pli_sender = Some(sender); }
    }

    pub fn add_subscriber(&self, publisher_ssrc: u32, rewrite_ssrc: u32) {
        let readers = self.inner.read();
        if let Some(pubstate) = readers.get(&publisher_ssrc) {
            let current = pubstate.subscribers.load_full();
            let mut next = Vec::with_capacity(current.len() + 1);
            next.extend(current.iter().cloned());
            next.push(Subscriber { rewrite_ssrc, last_in_seq: None, last_out_seq: None, last_in_ts: None, last_out_ts: None });
            pubstate.subscribers.store(next.into());
        }
    }

    pub fn handle_nack(&self, publisher_ssrc: u32, lost_seqs: &[u16]) -> Result<Vec<Arc<Bytes>>, Error> {
        let readers = self.inner.read();
        let pubstate = readers.get(&publisher_ssrc).ok_or(Error::UnknownPublisher)?;
        let mut resp = Vec::new();
        let cache = pubstate.retrans_cache.lock();
        for &seq in lost_seqs {
            let slot = (seq as usize) & 511usize;
            if let Some((stored_seq, data)) = &cache[slot] {
                if *stored_seq == seq { resp.push(data.clone()); }
            }
        }
        Ok(resp)
    }

    pub fn route(&self, buf: Arc<Bytes>, out: &mut SmallVec<[OutgoingPacket; 16]>) -> Result<(), Error> {
        let pkt = RtpPacket::try_from(buf.as_ref()).map_err(Error::from)?;
        let pub_ssrc = pkt.ssrc;
        let readers = self.inner.read();
        let pubstate = readers.get(&pub_ssrc).ok_or(Error::UnknownPublisher)?;
        {
            let mut cache = pubstate.retrans_cache.lock();
            let slot = (pkt.sequence_number as usize) & 511usize;
            cache[slot] = Some((pkt.sequence_number, buf.clone()));
        }
        let list = pubstate.subscribers.load();
        let refs = list.as_ref();
        let map = &pubstate.extensions;
        out.clear();
        for sub in refs.iter() {
            let in_seq = pkt.sequence_number;
            let in_ts = pkt.timestamp;
            let delta_seq = if let Some(last_in) = sub.last_in_seq { in_seq.wrapping_sub(last_in) } else { 0 };
            let out_seq = sub.last_out_seq.map(|o| o.wrapping_add(delta_seq)).unwrap_or(in_seq);
            let seq_delta = (out_seq as i32) - (in_seq as i32);
            let delta_ts = if let Some(last_in_ts) = sub.last_in_ts { in_ts.wrapping_sub(last_in_ts) } else { 0 };
            let out_ts = sub.last_out_ts.map(|t| t.wrapping_add(delta_ts)).unwrap_or(in_ts);
            let ts_delta = (out_ts as i64) - (in_ts as i64);
            let header = pkt.build_rewrite_header(sub.rewrite_ssrc, seq_delta, ts_delta);
            let transport_cc = pkt.get_extension::<crate::rtp::TransportCc>(map);
            let payload_range = pkt.payload_offset..(pkt.payload_offset + pkt.payload_len);
            let op = OutgoingPacket { header, payload: buf.clone(), payload_range: payload_range.clone(), dest_ssrc: sub.rewrite_ssrc, transport_cc: transport_cc };
            if let Some(twcc) = transport_cc {
                if let Some(cb) = &pubstate.feedback_cb { cb.on_twcc(pub_ssrc, twcc); }
                if let Some(s) = &pubstate.feedback_sender { let _ = s.try_send((pub_ssrc, twcc)); }
            }
            out.push(op);
        }
        Ok(())
    }
}
