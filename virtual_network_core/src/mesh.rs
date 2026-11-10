use std::net::Ipv4Addr;
use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::mpsc;

#[async_trait]
pub trait PeerLink: Send + Sync {
    /// Returns the peer's virtual IPv4 address.
    fn virtual_ipv4(&self) -> Ipv4Addr;

    /// Returns the peer's name.
    fn name(&self) -> String;

    /// Sends a raw IPv4 packet over the encrypted stream to this peer.
    async fn send_packet(&self, packet: &[u8]) -> anyhow::Result<()>;

    /// Waits for the next raw IPv4 packet from this peer and writes it into `buf`.
    async fn recv_packet(&self, buf: &mut [u8; crate::RECV_BUFFER_SIZE]) -> anyhow::Result<usize>;
}

/// Everything a mesh needs to join a network.
pub struct MeshConfig {
    /// The shared network secret.
    pub secret: String,

    /// This node's name, shown to other peers.
    pub name: String,

    /// This node's virtual IPv4 address.
    pub virtual_ipv4: Ipv4Addr,
}

pub enum MeshEvent<MeshPeerLink : PeerLink> {
    /// A peer was authenticated and can be routed to.
    PeerFound(Arc<MeshPeerLink>),

    /// The connection to a previously found peer was closed.
    PeerLost(Arc<MeshPeerLink>),
}

pub trait Mesh {
    type MeshPeerLink: PeerLink + 'static;

    /// Joins the network and starts discovering peers in the background.
    ///
    /// Returns the mesh and the receiver for its events. Dropping the mesh leaves the network.
    /// Must be called from within a tokio runtime.
    fn join(config: MeshConfig) -> anyhow::Result<(Self, mpsc::Receiver<MeshEvent<Self::MeshPeerLink>>)>
    where
        Self: Sized;
}