//! The `packet` module defines data structures and methods to pull data from the network.
pub use mundis_sdk::packet::{Meta, Packet, PacketFlags, PACKET_DATA_SIZE};
use {
    crate::{cuda_runtime::PinnedVec, recycler::Recycler},
    serde::Serialize,
    std::net::SocketAddr,
};

pub const NUM_PACKETS: usize = 1024 * 8;

pub const PACKETS_PER_BATCH: usize = 64;
pub const NUM_RCVMMSGS: usize = 64;

#[derive(Debug, Default, Clone)]
pub struct PacketBatch {
    pub packets: PinnedVec<Packet>,
}

pub type PacketBatchRecycler = Recycler<PinnedVec<Packet>>;

impl PacketBatch {
    pub fn new(packets: Vec<Packet>) -> Self {
        let packets = PinnedVec::from_vec(packets);
        Self { packets }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        let packets = PinnedVec::with_capacity(capacity);
        PacketBatch { packets }
    }

    pub fn new_unpinned_with_recycler(
        recycler: PacketBatchRecycler,
        size: usize,
        name: &'static str,
    ) -> Self {
        let mut packets = recycler.allocate(name);
        packets.reserve(size);
        PacketBatch { packets }
    }

    pub fn new_with_recycler(
        recycler: PacketBatchRecycler,
        size: usize,
        name: &'static str,
    ) -> Self {
        let mut packets = recycler.allocate(name);
        packets.reserve_and_pin(size);
        PacketBatch { packets }
    }

    pub fn new_with_recycler_data(
        recycler: &PacketBatchRecycler,
        name: &'static str,
        mut packets: Vec<Packet>,
    ) -> Self {
        let mut batch = Self::new_with_recycler(recycler.clone(), packets.len(), name);
        batch.packets.append(&mut packets);
        batch
    }

    pub fn new_unpinned_with_recycler_data(
        recycler: &PacketBatchRecycler,
        name: &'static str,
        mut packets: Vec<Packet>,
    ) -> Self {
        let mut batch = Self::new_unpinned_with_recycler(recycler.clone(), packets.len(), name);
        batch.packets.append(&mut packets);
        batch
    }

    pub fn set_addr(&mut self, addr: &SocketAddr) {
        for p in self.packets.iter_mut() {
            p.meta.set_addr(addr);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.packets.is_empty()
    }
}

pub fn to_packet_batches<T: Serialize>(xs: &[T], chunks: usize) -> Vec<PacketBatch> {
    xs.chunks(chunks)
        .map(|x| {
            let mut batch = PacketBatch::with_capacity(x.len());
            batch.packets.resize(x.len(), Packet::default());
            for (i, packet) in x.iter().zip(batch.packets.iter_mut()) {
                Packet::populate_packet(packet, None, i).expect("serialize request");
            }
            batch
        })
        .collect()
}

#[cfg(test)]
pub fn to_packet_batches_for_tests<T: Serialize>(xs: &[T]) -> Vec<PacketBatch> {
    to_packet_batches(xs, NUM_PACKETS)
}

pub fn to_packet_batch_with_destination<T: Serialize>(
    recycler: PacketBatchRecycler,
    dests_and_data: &[(SocketAddr, T)],
) -> PacketBatch {
    let mut out = PacketBatch::new_unpinned_with_recycler(
        recycler,
        dests_and_data.len(),
        "to_packet_batch_with_destination",
    );
    out.packets.resize(dests_and_data.len(), Packet::default());
    for (dest_and_data, o) in dests_and_data.iter().zip(out.packets.iter_mut()) {
        if !dest_and_data.0.ip().is_unspecified() && dest_and_data.0.port() != 0 {
            if let Err(e) = Packet::populate_packet(o, Some(&dest_and_data.0), &dest_and_data.1) {
                // TODO: This should never happen. Instead the caller should
                // break the payload into smaller messages, and here any errors
                // should be propagated.
                error!("Couldn't write to packet {:?}. Data skipped.", e);
            }
        } else {
            trace!("Dropping packet, as destination is unknown");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        mundis_sdk::{
            hash::Hash,
            signature::{Keypair, Signer},
            system_transaction,
        },
    };

    #[test]
    fn test_to_packet_batches() {
        let keypair = Keypair::new();
        let hash = Hash::new(&[1; 32]);
        let tx = system_transaction::transfer(&keypair, &keypair.pubkey(), 1, hash);
        let rv = to_packet_batches_for_tests(&[tx.clone(); 1]);
        assert_eq!(rv.len(), 1);
        assert_eq!(rv[0].packets.len(), 1);

        #[allow(clippy::useless_vec)]
        let rv = to_packet_batches_for_tests(&vec![tx.clone(); NUM_PACKETS]);
        assert_eq!(rv.len(), 1);
        assert_eq!(rv[0].packets.len(), NUM_PACKETS);

        #[allow(clippy::useless_vec)]
        let rv = to_packet_batches_for_tests(&vec![tx; NUM_PACKETS + 1]);
        assert_eq!(rv.len(), 2);
        assert_eq!(rv[0].packets.len(), NUM_PACKETS);
        assert_eq!(rv[1].packets.len(), 1);
    }

    #[test]
    fn test_to_packets_pinning() {
        let recycler = PacketBatchRecycler::default();
        for i in 0..2 {
            let _first_packets =
                PacketBatch::new_with_recycler(recycler.clone(), i + 1, "first one");
        }
    }
}
