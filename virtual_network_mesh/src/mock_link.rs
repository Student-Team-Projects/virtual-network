//! In-memory mock [`PeerLink`] that needs no network.
//!
//! [`MockPeerLink::new`] returns the link together with a [`MockPeerHandle`], which plays the
//! role of the remote peer:
//!
//! - [`MockPeerHandle::inject`] simulates the peer sending a packet. The packet comes out of
//!   [`PeerLink::recv_packet`].
//! - [`MockPeerHandle::next_sent`] returns packets that were written with
//!   [`PeerLink::send_packet`], i.e. packets sent to the peer.

use std::net::Ipv4Addr;

use anyhow::{Context, bail};
use async_trait::async_trait;
use tokio::sync::{Mutex, mpsc};
use virtual_network_core::RECV_BUFFER_SIZE;
use virtual_network_core::mesh::PeerLink;

pub struct MockPeerLink {
    virtual_ipv4: Ipv4Addr,
    /// Packets injected by the handle, returned from `recv_packet`.
    incoming: Mutex<mpsc::UnboundedReceiver<Vec<u8>>>,
    /// Packets written with `send_packet`, read by the handle.
    outgoing: mpsc::UnboundedSender<Vec<u8>>,
}

/// The "remote peer side" of a [`MockPeerLink`].
pub struct MockPeerHandle {
    /// Packets injected here are returned from `recv_packet`.
    to_link: mpsc::UnboundedSender<Vec<u8>>,
    /// Packets written with `send_packet`.
    from_link: mpsc::UnboundedReceiver<Vec<u8>>,
}

impl MockPeerLink {
    /// Creates a link to a peer with the virtual address `virtual_ipv4`.
    pub fn new(virtual_ipv4: Ipv4Addr) -> (Self, MockPeerHandle) {
        let (to_link, incoming) = mpsc::unbounded_channel();
        let (outgoing, from_link) = mpsc::unbounded_channel();

        let link = MockPeerLink { virtual_ipv4, incoming: Mutex::new(incoming), outgoing };
        let handle = MockPeerHandle { to_link, from_link };
        (link, handle)
    }
}

#[async_trait]
impl PeerLink for MockPeerLink {
    fn virtual_ipv4(&self) -> Ipv4Addr {
        self.virtual_ipv4
    }

    fn name(&self) -> String {
        self.virtual_ipv4.to_string()
    }

    async fn send_packet(&self, packet: &[u8]) -> anyhow::Result<()> {
        self.outgoing.send(packet.to_vec()).context("mock peer handle was dropped")
    }

    async fn recv_packet(&self, buf: &mut [u8; RECV_BUFFER_SIZE]) -> anyhow::Result<usize> {
        let packet =
            self.incoming.lock().await.recv().await.context("mock peer handle was dropped")?;

        // `MockPeerHandle::inject` rejects packets that don't fit.
        debug_assert!(packet.len() <= buf.len());
        buf[..packet.len()].copy_from_slice(&packet);
        Ok(packet.len())
    }
}

impl MockPeerHandle {
    /// Simulates the peer sending `packet` to us.
    ///
    /// Fails if `packet` is larger than [`RECV_BUFFER_SIZE`] or the link was dropped.
    pub fn inject(&self, packet: impl Into<Vec<u8>>) -> anyhow::Result<()> {
        let packet = packet.into();
        if packet.len() > RECV_BUFFER_SIZE {
            bail!(
                "injected packet is {} bytes, larger than the {} byte receive buffer",
                packet.len(),
                RECV_BUFFER_SIZE
            );
        }

        self.to_link.send(packet).ok().context("mock peer link was dropped")
    }

    /// Waits for the next packet that was sent to the peer.
    ///
    /// Returns `None` once the link is dropped and all sent packets were read.
    pub async fn next_sent(&mut self) -> Option<Vec<u8>> {
        self.from_link.recv().await
    }

    /// Returns the next packet that was sent to the peer without waiting, if there is one.
    pub fn try_next_sent(&mut self) -> Option<Vec<u8>> {
        self.from_link.try_recv().ok()
    }
}
