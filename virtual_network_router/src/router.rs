use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;
use tokio::task::AbortHandle;
use virtual_network_core::RECV_BUFFER_SIZE;
use virtual_network_core::driver::DeviceDriver;
use virtual_network_core::mesh::PeerLink;
use virtual_network_core::network::VirtualNetwork;
use virtual_network_core::packet::parse_ipv4;

/// Packets waiting to be sent to a single peer. When full, new packets to that peer are dropped.
const PEER_SEND_QUEUE_CAPACITY: usize = 128;

type Packet = Arc<[u8]>;
type Peers = Arc<Mutex<HashMap<Ipv4Addr, PeerEntry>>>;

struct PeerEntry {
    link: Arc<dyn PeerLink>,
    /// Queues packets that should be sent to this peer.
    send_queue: mpsc::Sender<Packet>,
    /// The task reading packets from this peer.
    receive_task: AbortHandle,
}

impl Drop for PeerEntry {
    fn drop(&mut self) {
        // `send_to_peer` stops once `send_queue` is dropped, but `receive_from_peer` may be
        // waiting on `recv_packet` indefinitely.
        self.receive_task.abort();
    }
}

/// Forwards packets between the driver and peer links. Dropping it stops all forwarding.
pub struct Router {
    driver: Arc<dyn DeviceDriver>,
    network: VirtualNetwork,
    peers: Peers,
    /// The task reading packets from the driver.
    driver_task: AbortHandle,
}

impl Router {
    /// Starts forwarding packets read from `driver` for destinations in `network`.
    ///
    /// Must be called from within a tokio runtime.
    pub fn new(driver: Arc<dyn DeviceDriver>, network: VirtualNetwork) -> Self {
        let peers = Peers::default();
        let driver_task = tokio::spawn(forward_from_driver(driver.clone(), network, peers.clone()))
            .abort_handle();
        Router { driver, network, peers, driver_task }
    }
}

impl Drop for Router {
    fn drop(&mut self) {
        self.driver_task.abort();
    }
}

impl virtual_network_core::router::Router for Router {
    /// Replaces any existing link with the same virtual IP.
    ///
    /// Links whose address is not a host address in the router's virtual network are ignored.
    ///
    /// Must be called from within a tokio runtime.
    fn add_peer_link(&self, link: Arc<dyn PeerLink>) {
        let virtual_ipv4 = link.virtual_ipv4();
        if !self.network.is_host(virtual_ipv4) {
            return;
        }

        let (send_queue, queued_packets) = mpsc::channel(PEER_SEND_QUEUE_CAPACITY);
        tokio::spawn(send_to_peer(link.clone(), queued_packets));
        let receive_task =
            tokio::spawn(receive_from_peer(link.clone(), self.driver.clone(), self.network))
                .abort_handle();

        let entry = PeerEntry { link, send_queue, receive_task };
        self.peers.lock().unwrap().insert(virtual_ipv4, entry);
    }

    /// Removes `link` if it is still the active link for its virtual IP.
    fn remove_peer_link(&self, link: Arc<dyn PeerLink>) {
        let virtual_ipv4 = link.virtual_ipv4();
        let mut peers = self.peers.lock().unwrap();
        if peers.get(&virtual_ipv4).is_some_and(|entry| same_link(&entry.link, &link)) {
            peers.remove(&virtual_ipv4);
        }
    }
}

/// Compares only the data pointers, since vtable pointers for the same type aren't guaranteed to
/// be equal.
fn same_link(a: &Arc<dyn PeerLink>, b: &Arc<dyn PeerLink>) -> bool {
    std::ptr::addr_eq(Arc::as_ptr(a), Arc::as_ptr(b))
}

/// Whether packets to `destination` go to every peer instead of a single one.
fn is_flood(destination: Ipv4Addr, network: VirtualNetwork) -> bool {
    destination.is_broadcast() || destination.is_multicast() || destination == network.broadcast()
}

/// Reads packets from the driver and queues them for the peers they're addressed to.
///
/// Stops when the driver fails, since there is nothing left to forward.
async fn forward_from_driver(driver: Arc<dyn DeviceDriver>, network: VirtualNetwork, peers: Peers) {
    let mut buf = Box::new([0u8; RECV_BUFFER_SIZE]);
    loop {
        let Ok(len) = driver.recv(&mut buf).await else {
            return;
        };
        let packet = &buf[..len];
        let Some(header) = parse_ipv4(packet) else {
            continue;
        };

        // A full queue means that peer is too slow, so its packet is dropped (`try_send`) rather
        // than blocking everyone else.
        let peers = peers.lock().unwrap();
        if is_flood(header.destination, network) {
            let packet = Packet::from(packet);
            for entry in peers.values() {
                let _ = entry.send_queue.try_send(packet.clone());
            }
        } else if let Some(entry) = peers.get(&header.destination) {
            let _ = entry.send_queue.try_send(Packet::from(packet));
        }
    }
}

/// Sends queued packets to a single peer until its queue is closed or the link fails.
async fn send_to_peer(link: Arc<dyn PeerLink>, mut queued_packets: mpsc::Receiver<Packet>) {
    while let Some(packet) = queued_packets.recv().await {
        if link.send_packet(&packet).await.is_err() {
            return;
        }
    }
}

/// Reads packets from a peer and writes the accepted ones to the driver.
///
/// Accepts packets from that peer's own virtual address that are addressed to this node or are
/// broadcast/multicast. Unicast meant for someone else is dropped so we don't act as a relay.
async fn receive_from_peer(
    link: Arc<dyn PeerLink>,
    driver: Arc<dyn DeviceDriver>,
    network: VirtualNetwork,
) {
    let expected_source = link.virtual_ipv4();
    let local_ipv4 = driver.virtual_ipv4();
    let mut buf = Box::new([0u8; RECV_BUFFER_SIZE]);
    loop {
        let Ok(len) = link.recv_packet(&mut buf).await else {
            return;
        };
        let packet = &buf[..len];
        let Some(header) = parse_ipv4(packet) else {
            continue;
        };
        let for_us = header.destination == local_ipv4 || is_flood(header.destination, network);
        if header.source != expected_source || !for_us {
            continue;
        }
        let _ = driver.send(packet).await;
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::time::Duration;

    use async_trait::async_trait;
    use etherparse::PacketBuilder;
    use virtual_network_core::router::Router as _;
    use virtual_network_driver::mock::{MockDriver, MockHandle};
    use virtual_network_mesh::mock_link::{MockPeerHandle, MockPeerLink};

    use super::*;

    const LOCAL: Ipv4Addr = Ipv4Addr::new(100, 64, 0, 1);
    const PEER_A: Ipv4Addr = Ipv4Addr::new(100, 64, 0, 2);
    const PEER_B: Ipv4Addr = Ipv4Addr::new(100, 64, 0, 3);
    const OUTSIDE: Ipv4Addr = Ipv4Addr::new(8, 8, 8, 8);
    const MULTICAST: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 1);

    fn network() -> VirtualNetwork {
        VirtualNetwork::new(Ipv4Addr::new(100, 64, 0, 0), 24).unwrap()
    }

    /// Destinations that are sent to every peer.
    fn flood_destinations() -> [Ipv4Addr; 3] {
        [Ipv4Addr::BROADCAST, network().broadcast(), MULTICAST]
    }

    fn packet(source: Ipv4Addr, destination: Ipv4Addr) -> Vec<u8> {
        let builder = PacketBuilder::ipv4(source.octets(), destination.octets(), 64).udp(1, 2);
        let mut packet = Vec::with_capacity(builder.size(5));
        builder.write(&mut packet, b"hello").unwrap();
        packet
    }

    /// Waits briefly for a packet; `None` means that nothing arrived.
    async fn within<T>(next: impl Future<Output = Option<T>>) -> Option<T> {
        tokio::time::timeout(Duration::from_millis(100), next).await.ok().flatten()
    }

    fn start() -> (Router, MockHandle) {
        let (driver, os) = MockDriver::new(LOCAL);
        (Router::new(Arc::new(driver), network()), os)
    }

    fn add_peer(router: &Router, ip: Ipv4Addr) -> (Arc<dyn PeerLink>, MockPeerHandle) {
        let (link, peer) = MockPeerLink::new(ip);
        let link: Arc<dyn PeerLink> = Arc::new(link);
        router.add_peer_link(link.clone());
        (link, peer)
    }

    /// A peer that never finishes sending or receiving.
    struct StuckLink;

    #[async_trait]
    impl PeerLink for StuckLink {
        fn virtual_ipv4(&self) -> Ipv4Addr {
            PEER_A
        }

        fn name(&self) -> String {
            "stuck".into()
        }

        async fn send_packet(&self, _packet: &[u8]) -> anyhow::Result<()> {
            std::future::pending().await
        }

        async fn recv_packet(&self, _buf: &mut [u8; RECV_BUFFER_SIZE]) -> anyhow::Result<usize> {
            std::future::pending().await
        }
    }

    #[tokio::test]
    async fn unicast_goes_only_to_the_destination_peer() {
        let (router, os) = start();
        let (_, mut a) = add_peer(&router, PEER_A);
        let (_, mut b) = add_peer(&router, PEER_B);

        let packet = packet(LOCAL, PEER_B);
        os.inject(packet.clone()).unwrap();

        assert_eq!(within(b.next_sent()).await, Some(packet));
        assert_eq!(within(a.next_sent()).await, None);
    }

    #[tokio::test]
    async fn broadcast_and_multicast_go_to_all_peers() {
        let (router, os) = start();
        let (_, mut a) = add_peer(&router, PEER_A);
        let (_, mut b) = add_peer(&router, PEER_B);

        for destination in flood_destinations() {
            let packet = packet(LOCAL, destination);
            os.inject(packet.clone()).unwrap();
            assert_eq!(within(a.next_sent()).await, Some(packet.clone()));
            assert_eq!(within(b.next_sent()).await, Some(packet));
        }
    }

    #[tokio::test]
    async fn drops_unroutable_and_invalid_packets_but_keeps_forwarding() {
        let (router, os) = start();
        let (_, mut a) = add_peer(&router, PEER_A);

        os.inject(packet(LOCAL, Ipv4Addr::new(100, 64, 0, 99))).unwrap();
        os.inject(packet(LOCAL, LOCAL)).unwrap();
        os.inject(vec![1, 2, 3]).unwrap();
        let valid = packet(LOCAL, PEER_A);
        os.inject(valid.clone()).unwrap();

        assert_eq!(within(a.next_sent()).await, Some(valid));
        assert_eq!(within(a.next_sent()).await, None);
    }

    #[tokio::test]
    async fn delivers_inbound_packets_for_us_and_drops_everything_else() {
        let (router, mut os) = start();
        let (_, a) = add_peer(&router, PEER_A);

        a.inject(packet(PEER_B, LOCAL)).unwrap(); // spoofed source
        a.inject(packet(PEER_A, PEER_B)).unwrap(); // meant for another peer
        a.inject(packet(PEER_A, OUTSIDE)).unwrap(); // would be relayed to another network
        a.inject(vec![1, 2, 3]).unwrap(); // invalid
        let valid = packet(PEER_A, LOCAL);
        a.inject(valid.clone()).unwrap();

        assert_eq!(within(os.next_delivered()).await, Some(valid));
        assert_eq!(within(os.next_delivered()).await, None);
    }

    #[tokio::test]
    async fn delivers_inbound_broadcast_and_multicast() {
        let (router, mut os) = start();
        let (_, a) = add_peer(&router, PEER_A);

        for destination in flood_destinations() {
            let packet = packet(PEER_A, destination);
            a.inject(packet.clone()).unwrap();
            assert_eq!(within(os.next_delivered()).await, Some(packet));
        }
    }

    #[tokio::test]
    async fn ignores_peers_that_are_not_hosts_of_the_network() {
        let (router, os) = start();
        let outside_subnet = Ipv4Addr::new(10, 0, 0, 2);
        let addresses = [outside_subnet, network().network(), network().broadcast()];
        let mut peers: Vec<_> = addresses.map(|ip| add_peer(&router, ip).1).into();

        os.inject(packet(LOCAL, Ipv4Addr::BROADCAST)).unwrap();

        for peer in &mut peers {
            assert_eq!(within(peer.next_sent()).await, None);
        }
    }

    #[tokio::test]
    async fn removed_peer_neither_receives_nor_sends() {
        let (router, mut os) = start();
        let (link, mut a) = add_peer(&router, PEER_A);
        router.remove_peer_link(link);

        os.inject(packet(LOCAL, PEER_A)).unwrap();
        os.inject(packet(LOCAL, Ipv4Addr::BROADCAST)).unwrap();
        a.inject(packet(PEER_A, LOCAL)).ok();

        assert_eq!(within(a.next_sent()).await, None);
        assert_eq!(within(os.next_delivered()).await, None);
    }

    #[tokio::test]
    async fn removing_a_stale_link_keeps_the_active_one() {
        let (router, os) = start();
        let (_, mut a) = add_peer(&router, PEER_A);
        let (stale, _stale_handle) = MockPeerLink::new(PEER_A);
        router.remove_peer_link(Arc::new(stale));

        let packet = packet(LOCAL, PEER_A);
        os.inject(packet.clone()).unwrap();

        assert_eq!(within(a.next_sent()).await, Some(packet));
    }

    #[tokio::test]
    async fn new_link_replaces_the_old_one_with_the_same_ip() {
        let (router, os) = start();
        let (_, mut old) = add_peer(&router, PEER_A);
        let (_, mut new) = add_peer(&router, PEER_A);

        let packet = packet(LOCAL, PEER_A);
        os.inject(packet.clone()).unwrap();

        assert_eq!(within(new.next_sent()).await, Some(packet));
        assert_eq!(within(old.next_sent()).await, None);
    }

    #[tokio::test]
    async fn slow_peer_does_not_block_other_peers() {
        let (router, os) = start();
        router.add_peer_link(Arc::new(StuckLink));
        let (_, mut b) = add_peer(&router, PEER_B);

        for _ in 0..PEER_SEND_QUEUE_CAPACITY * 2 {
            os.inject(packet(LOCAL, PEER_A)).unwrap();
        }
        let packet = packet(LOCAL, PEER_B);
        os.inject(packet.clone()).unwrap();

        assert_eq!(within(b.next_sent()).await, Some(packet));
    }

    #[tokio::test]
    async fn dropping_the_router_stops_forwarding() {
        let (router, os) = start();
        let (_, mut a) = add_peer(&router, PEER_A);
        drop(router);

        os.inject(packet(LOCAL, PEER_A)).ok();

        assert_eq!(within(a.next_sent()).await, None);
    }
}
