use {
    bincode::{Options, Result},
    bitflags::bitflags,
    serde::Serialize,
    std::{
        fmt, io,
        net::{IpAddr, Ipv4Addr, SocketAddr},
    },
};

/// Maximum over-the-wire size of a Transaction
///   1280 is IPv6 minimum MTU
///   40 bytes is the size of the IPv6 header
///   8 bytes is the size of the fragment header
pub const PACKET_DATA_SIZE: usize = 1280 - 40 - 8;

bitflags! {
    #[repr(C)]
    pub struct PacketFlags: u8 {
        const DISCARD        = 0b00000001;
        const FORWARDED      = 0b00000010;
        const REPAIR         = 0b00000100;
        const SIMPLE_VOTE_TX = 0b00001000;
        const TRACER_TX      = 0b00010000;
    }
}

#[derive(Clone, Debug, PartialEq)]
#[repr(C)]
pub struct Meta {
    pub size: usize,
    pub addr: IpAddr,
    pub port: u16,
    pub flags: PacketFlags,
}

#[derive(Clone)]
#[repr(C)]
pub struct Packet {
    pub data: [u8; PACKET_DATA_SIZE],
    pub meta: Meta,
}

impl Packet {
    pub fn new(data: [u8; PACKET_DATA_SIZE], meta: Meta) -> Self {
        Self { data, meta }
    }

    pub fn from_data<T: Serialize>(dest: Option<&SocketAddr>, data: T) -> Result<Self> {
        let mut packet = Packet::default();
        Self::populate_packet(&mut packet, dest, &data)?;
        Ok(packet)
    }

    pub fn populate_packet<T: Serialize>(
        packet: &mut Packet,
        dest: Option<&SocketAddr>,
        data: &T,
    ) -> Result<()> {
        let mut wr = io::Cursor::new(&mut packet.data[..]);
        bincode::serialize_into(&mut wr, data)?;
        let len = wr.position() as usize;
        packet.meta.size = len;
        if let Some(dest) = dest {
            packet.meta.set_addr(dest);
        }
        Ok(())
    }

    pub fn deserialize_slice<T, I>(&self, index: I) -> Result<T>
        where
            T: serde::de::DeserializeOwned,
            I: std::slice::SliceIndex<[u8], Output = [u8]>,
    {
        let data = &self.data[0..self.meta.size];
        let bytes = data.get(index).ok_or(bincode::ErrorKind::SizeLimit)?;
        bincode::options()
            .with_limit(PACKET_DATA_SIZE as u64)
            .with_fixint_encoding()
            .reject_trailing_bytes()
            .deserialize(bytes)
    }
}

impl fmt::Debug for Packet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Packet {{ size: {:?}, addr: {:?} }}",
            self.meta.size,
            self.meta.addr()
        )
    }
}

#[allow(clippy::uninit_assumed_init)]
impl Default for Packet {
    fn default() -> Packet {
        Packet {
            data: unsafe { std::mem::MaybeUninit::uninit().assume_init() },
            meta: Meta::default(),
        }
    }
}

impl PartialEq for Packet {
    fn eq(&self, other: &Packet) -> bool {
        let self_data: &[u8] = self.data.as_ref();
        let other_data: &[u8] = other.data.as_ref();
        self.meta == other.meta && self_data[..self.meta.size] == other_data[..self.meta.size]
    }
}

impl Meta {
    pub fn addr(&self) -> SocketAddr {
        SocketAddr::new(self.addr, self.port)
    }

    pub fn set_addr(&mut self, socket_addr: &SocketAddr) {
        self.addr = socket_addr.ip();
        self.port = socket_addr.port();
    }

    #[inline]
    pub fn discard(&self) -> bool {
        self.flags.contains(PacketFlags::DISCARD)
    }

    #[inline]
    pub fn set_discard(&mut self, discard: bool) {
        self.flags.set(PacketFlags::DISCARD, discard);
    }

    #[inline]
    pub fn forwarded(&self) -> bool {
        self.flags.contains(PacketFlags::FORWARDED)
    }

    #[inline]
    pub fn repair(&self) -> bool {
        self.flags.contains(PacketFlags::REPAIR)
    }

    #[inline]
    pub fn is_simple_vote_tx(&self) -> bool {
        self.flags.contains(PacketFlags::SIMPLE_VOTE_TX)
    }

    #[inline]
    pub fn is_tracer_tx(&self) -> bool {
        self.flags.contains(PacketFlags::TRACER_TX)
    }
}

impl Default for Meta {
    fn default() -> Self {
        Self {
            size: 0,
            addr: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            port: 0,
            flags: PacketFlags::empty(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_slice() {
        let p = Packet::from_data(None, u32::MAX).unwrap();
        assert_eq!(p.deserialize_slice(..).ok(), Some(u32::MAX));
        assert_eq!(p.deserialize_slice(0..4).ok(), Some(u32::MAX));
        assert_eq!(
            p.deserialize_slice::<u16, _>(0..4)
                .map_err(|e| e.to_string()),
            Err("Slice had bytes remaining after deserialization".to_string()),
        );
        assert_eq!(
            p.deserialize_slice::<u32, _>(0..0)
                .map_err(|e| e.to_string()),
            Err("io error: unexpected end of file".to_string()),
        );
        assert_eq!(
            p.deserialize_slice::<u32, _>(0..1)
                .map_err(|e| e.to_string()),
            Err("io error: unexpected end of file".to_string()),
        );
        assert_eq!(
            p.deserialize_slice::<u32, _>(0..5)
                .map_err(|e| e.to_string()),
            Err("the size limit has been reached".to_string()),
        );
        #[allow(clippy::reversed_empty_ranges)]
            let reversed_empty_range = 4..0;
        assert_eq!(
            p.deserialize_slice::<u32, _>(reversed_empty_range)
                .map_err(|e| e.to_string()),
            Err("the size limit has been reached".to_string()),
        );
        assert_eq!(
            p.deserialize_slice::<u32, _>(4..5)
                .map_err(|e| e.to_string()),
            Err("the size limit has been reached".to_string()),
        );
    }
}