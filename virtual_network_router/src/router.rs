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
