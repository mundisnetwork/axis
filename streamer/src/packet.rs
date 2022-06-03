//! The `packet` module defines data structures and methods to pull data from the network.
use {
    crate::{
        recvmmsg::{recv_mmsg, NUM_RCVMMSGS},
        socket::SocketAddrSpace,
    },
    mundis_metrics::inc_new_counter_debug,
    std::{io::Result, net::UdpSocket, time::Instant},
};
pub use {
    mundis_perf::packet::{
        to_packet_batches, PacketBatch, PacketBatchRecycler, NUM_PACKETS, PACKETS_PER_BATCH,
    },
    mundis_sdk::packet::{Meta, Packet, PACKET_DATA_SIZE},
};

pub fn recv_from(batch: &mut PacketBatch, socket: &UdpSocket, max_wait_ms: u64) -> Result<usize> {
    let mut i = 0;
    //DOCUMENTED SIDE-EFFECT
    //Performance out of the IO without poll
    //  * block on the socket until it's readable
    //  * set the socket to non blocking
    //  * read until it fails
    //  * set it back to blocking before returning
    socket.set_nonblocking(false)?;
    trace!("receiving on {}", socket.local_addr().unwrap());
    let start = Instant::now();
    loop {
        batch.packets.resize(
            std::cmp::min(i + NUM_RCVMMSGS, PACKETS_PER_BATCH),
            Packet::default(),
        );
        match recv_mmsg(socket, &mut batch.packets[i..]) {
            Err(_) if i > 0 => {
                if start.elapsed().as_millis() as u64 > max_wait_ms {
                    break;
                }
            }
            Err(e) => {
                trace!("recv_from err {:?}", e);
                return Err(e);
            }
            Ok(npkts) => {
                if i == 0 {
                    socket.set_nonblocking(true)?;
                }
                trace!("got {} packets", npkts);
                i += npkts;
                // Try to batch into big enough buffers
                // will cause less re-shuffling later on.
                if start.elapsed().as_millis() as u64 > max_wait_ms || i >= PACKETS_PER_BATCH {
                    break;
                }
            }
        }
    }
    batch.packets.truncate(i);
    inc_new_counter_debug!("packets-recv_count", i);
    Ok(i)
}

pub fn send_to(
    batch: &PacketBatch,
    socket: &UdpSocket,
    socket_addr_space: &SocketAddrSpace,
) -> Result<()> {
    for p in &batch.packets {
        let addr = p.meta.addr();
        if socket_addr_space.check(&addr) {
            socket.send_to(&p.data[..p.meta.size], &addr)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        std::{
            io,
            io::Write,
            net::{SocketAddr, UdpSocket},
        },
    };

    #[test]
    fn test_packets_set_addr() {
        // test that the address is actually being updated
        let send_addr: SocketAddr = "127.0.0.1:123".parse().unwrap();
        let packets = vec![Packet::default()];
        let mut packet_batch = PacketBatch::new(packets);
        packet_batch.set_addr(&send_addr);
        assert_eq!(packet_batch.packets[0].meta.addr(), send_addr);
    }

    #[test]
    pub fn packet_send_recv() {
        mundis_logger::setup();
        let recv_socket = UdpSocket::bind("127.0.0.1:0").expect("bind");
        let addr = recv_socket.local_addr().unwrap();
        let send_socket = UdpSocket::bind("127.0.0.1:0").expect("bind");
        let saddr = send_socket.local_addr().unwrap();
        let mut batch = PacketBatch::default();

        batch.packets.resize(10, Packet::default());

        for m in batch.packets.iter_mut() {
            m.meta.set_addr(&addr);
            m.meta.size = PACKET_DATA_SIZE;
        }
        send_to(&batch, &send_socket, &SocketAddrSpace::Unspecified).unwrap();

        batch
            .packets
            .iter_mut()
            .for_each(|pkt| pkt.meta = Meta::default());
        let recvd = recv_from(&mut batch, &recv_socket, 1).unwrap();

        assert_eq!(recvd, batch.packets.len());

        for m in &batch.packets {
            assert_eq!(m.meta.size, PACKET_DATA_SIZE);
            assert_eq!(m.meta.addr(), saddr);
        }
    }

    #[test]
    pub fn debug_trait() {
        write!(io::sink(), "{:?}", Packet::default()).unwrap();
        write!(io::sink(), "{:?}", PacketBatch::default()).unwrap();
    }

    #[test]
    fn test_packet_partial_eq() {
        let mut p1 = Packet::default();
        let mut p2 = Packet::default();

        p1.meta.size = 1;
        p1.data[0] = 0;

        p2.meta.size = 1;
        p2.data[0] = 0;

        assert!(p1 == p2);

        p2.data[0] = 4;
        assert!(p1 != p2);
    }

    #[test]
    fn test_packet_resize() {
        mundis_logger::setup();
        let recv_socket = UdpSocket::bind("127.0.0.1:0").expect("bind");
        let addr = recv_socket.local_addr().unwrap();
        let send_socket = UdpSocket::bind("127.0.0.1:0").expect("bind");
        let mut batch = PacketBatch::default();
        batch.packets.resize(PACKETS_PER_BATCH, Packet::default());

        // Should only get PACKETS_PER_BATCH packets per iteration even
        // if a lot more were sent, and regardless of packet size
        for _ in 0..2 * PACKETS_PER_BATCH {
            let mut batch = PacketBatch::default();
            batch.packets.resize(1, Packet::default());
            for m in batch.packets.iter_mut() {
                m.meta.set_addr(&addr);
                m.meta.size = 1;
            }
            send_to(&batch, &send_socket, &SocketAddrSpace::Unspecified).unwrap();
        }

        let recvd = recv_from(&mut batch, &recv_socket, 100).unwrap();

        // Check we only got PACKETS_PER_BATCH packets
        assert_eq!(recvd, PACKETS_PER_BATCH);
        assert_eq!(batch.packets.capacity(), PACKETS_PER_BATCH);
    }
}
