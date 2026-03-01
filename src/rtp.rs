use bytes::Bytes;
use smallvec::SmallVec;
use std::convert::TryInto;

#[derive(Debug)]
pub enum Error {
    Truncated,
    InvalidVersion,
    ExtensionOverflow,
}

pub struct RtpPacket<'a> {
    raw: &'a [u8],
    pub version: u8,
    pub padding: bool,
    pub extension: bool,
    pub csrc_count: u8,
    pub marker: bool,
    pub payload_type: u8,
    pub sequence_number: u16,
    pub timestamp: u32,
    pub ssrc: u32,
    pub csrcs: SmallVec<[u32; 4]>,
    pub payload_offset: usize,
    pub payload_len: usize,
    pub extensions: SmallVec<[HeaderExtension; 4]>,
}

#[derive(Clone, Copy, Debug)]
pub struct HeaderExtension {
    pub id: u8,
    pub offset: usize,
    pub len: usize,
}

#[derive(Copy, Clone)]
pub enum ExtensionKind {
    TransportCc = 0,
}

pub struct ExtensionMap {
    ids: [u8; 256],
}

impl ExtensionMap {
    pub fn new() -> Self {
        ExtensionMap { ids: [0u8; 256] }
    }
    pub fn set(&mut self, kind: ExtensionKind, id: u8) {
        self.ids[kind as usize] = id;
    }
    pub fn get_id(&self, kind: ExtensionKind) -> Option<u8> {
        let v = self.ids[kind as usize];
        if v == 0 { None } else { Some(v) }
    }
}

pub struct ExtensionMapBuilder {
    map: ExtensionMap,
}

impl ExtensionMapBuilder {
    pub fn new() -> Self { ExtensionMapBuilder { map: ExtensionMap::new() } }
    pub fn with_pairs<I, S>(mut self, pairs: I) -> Self where I: IntoIterator<Item=(u8,S)>, S: AsRef<str> {
        for (id, uri) in pairs {
            let s = uri.as_ref();
            if s.contains("transport-wide-cc") { self.map.set(ExtensionKind::TransportCc, id); }
        }
        self
    }
    pub fn build(self) -> ExtensionMap { self.map }
}

pub trait ExtensionType {
    type Output;
    const KIND: ExtensionKind;
    fn parse(raw: &[u8]) -> Option<Self::Output>;
}

pub struct TransportCc;
impl ExtensionType for TransportCc {
    type Output = u16;
    const KIND: ExtensionKind = ExtensionKind::TransportCc;
    #[inline]
    fn parse(raw: &[u8]) -> Option<u16> {
        if raw.len() < 2 { return None }
        Some(((raw[0] as u16) << 8) | (raw[1] as u16))
    }
}

impl<'a> RtpPacket<'a> {
    pub fn parse(raw: &'a [u8]) -> Result<RtpPacket<'a>, Error> {
        if raw.len() < 12 {
            return Err(Error::Truncated);
        }
        let v_p_x_cc = raw[0];
        let version = v_p_x_cc >> 6;
        if version != 2 {
            return Err(Error::InvalidVersion);
        }
        let padding = (v_p_x_cc & 0x20) != 0;
        let extension = (v_p_x_cc & 0x10) != 0;
        let csrc_count = v_p_x_cc & 0x0f;
        let m_pt = raw[1];
        let marker = (m_pt & 0x80) != 0;
        let payload_type = m_pt & 0x7f;
        let sequence_number = u16::from_be_bytes(raw[2..4].try_into().unwrap());
        let timestamp = u32::from_be_bytes(raw[4..8].try_into().unwrap());
        let ssrc = u32::from_be_bytes(raw[8..12].try_into().unwrap());
        let mut offset = 12usize;
        let mut csrcs = SmallVec::new();
        for _ in 0..csrc_count {
            if raw.len() < offset + 4 {
                return Err(Error::Truncated);
            }
            let c = u32::from_be_bytes(raw[offset..offset + 4].try_into().unwrap());
            csrcs.push(c);
            offset += 4;
        }
        let mut extensions = SmallVec::<[HeaderExtension; 4]>::new();
        if extension {
            if raw.len() < offset + 4 {
                return Err(Error::Truncated);
            }
            let profile = u16::from_be_bytes(raw[offset..offset + 2].try_into().unwrap());
            let ext_len_words = u16::from_be_bytes(raw[offset + 2..offset + 4].try_into().unwrap()) as usize;
            let ext_total = 4 + ext_len_words * 4;
            if raw.len() < offset + ext_total {
                return Err(Error::ExtensionOverflow);
            }
            let mut ext_off = offset + 4;
            let ext_end = offset + ext_total;
            if profile == 0xBEDE {
                while ext_off < ext_end {
                    let b = raw[ext_off];
                    ext_off += 1;
                    if b == 0 { continue }
                    let id = b >> 4;
                    let len = (b & 0x0f) as usize + 1;
                    if ext_off + len > ext_end { break }
                    extensions.push(HeaderExtension { id, offset: ext_off, len });
                    ext_off += len;
                }
            } else {
                while ext_off + 1 < ext_end {
                    let id = raw[ext_off];
                    let len = raw[ext_off + 1] as usize;
                    ext_off += 2;
                    if ext_off + len > ext_end { break }
                    extensions.push(HeaderExtension { id, offset: ext_off, len });
                    ext_off += len;
                }
            }
            offset += ext_total;
        }
        if raw.len() < offset {
            return Err(Error::Truncated);
        }
        let payload_len = raw.len() - offset;
        if padding {
            if payload_len == 0 {
                return Err(Error::Truncated);
            }
            let pad = raw[raw.len() - 1] as usize;
            if pad > payload_len {
                return Err(Error::Truncated);
            }
        }
        Ok(RtpPacket {
            raw,
            version,
            padding,
            extension,
            csrc_count,
            marker,
            payload_type,
            sequence_number,
            timestamp,
            ssrc,
            csrcs,
            payload_offset: offset,
            payload_len: raw.len() - offset,
            extensions,
        })
    }

    pub fn payload(&self) -> &'a [u8] {
        &self.raw[self.payload_offset..self.payload_offset + self.payload_len]
    }

    pub fn header_slice(&self) -> &'a [u8] {
        &self.raw[..self.payload_offset]
    }

    #[inline]
    pub fn twcc_seq(&self, ext_id: u8) -> Option<u16> {
        for ext in self.extensions.iter() {
            if ext.id == ext_id && ext.len >= 2 {
                let hi = self.raw[ext.offset] as u16;
                let lo = self.raw[ext.offset + 1] as u16;
                return Some((hi << 8) | lo);
            }
        }
        None
    }

    pub fn get_extension_by_id(&self, id: u8) -> Option<&'a [u8]> {
        for ext in self.extensions.iter() {
            if ext.id == id { return Some(&self.raw[ext.offset..ext.offset + ext.len]) }
        }
        None
    }

    pub fn get_extension<T: ExtensionType>(&self, map: &ExtensionMap) -> Option<T::Output> {
        let id = map.get_id(T::KIND)?;
        let slice = self.get_extension_by_id(id)?;
        T::parse(slice)
    }

    pub fn build_rewrite_header(&self, new_ssrc: u32, seq_delta: i32, ts_delta: i64) -> SmallVec<[u8; 64]> {
        let mut out = SmallVec::<[u8; 64]>::new();
        out.extend_from_slice(&self.raw[..self.payload_offset]);
        let seq_off = 2usize;
        let ts_off = 4usize;
        let ssrc_off = 8usize;
        let orig_seq = self.sequence_number as i32;
        let orig_ts = self.timestamp as i64;
        let new_seq = ((orig_seq.wrapping_add(seq_delta)) & 0xffff) as u16;
        let new_ts = ((orig_ts.wrapping_add(ts_delta)) & 0xffffffff) as u32;
        out[seq_off] = (new_seq >> 8) as u8;
        out[seq_off + 1] = (new_seq & 0xff) as u8;
        out[ts_off] = (new_ts >> 24) as u8;
        out[ts_off + 1] = ((new_ts >> 16) & 0xff) as u8;
        out[ts_off + 2] = ((new_ts >> 8) & 0xff) as u8;
        out[ts_off + 3] = (new_ts & 0xff) as u8;
        out[ssrc_off] = (new_ssrc >> 24) as u8;
        out[ssrc_off + 1] = ((new_ssrc >> 16) & 0xff) as u8;
        out[ssrc_off + 2] = ((new_ssrc >> 8) & 0xff) as u8;
        out[ssrc_off + 3] = (new_ssrc & 0xff) as u8;
        out
    }

    pub fn into_bytes(self) -> &'a [u8] {
        self.raw
    }
}

impl<'a> TryFrom<&'a Bytes> for RtpPacket<'a> {
    type Error = Error;
    fn try_from(b: &'a Bytes) -> Result<RtpPacket<'a>, Error> {
        RtpPacket::parse(b.as_ref())
    }
}
