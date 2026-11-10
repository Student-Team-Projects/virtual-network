use std::net::Ipv4Addr;
use async_trait::async_trait;

#[async_trait]
pub trait DeviceDriver: Send + Sync {
    /// Waits for the next outbound IPv4 packet from the OS virtual interface and copies it into
    /// `buf`, returning the packet length.
    async fn recv(&self, buf: &mut [u8; crate::RECV_BUFFER_SIZE]) -> anyhow::Result<usize>;

    /// Writes an inbound IPv4 packet received from a peer into the OS virtual interface.
    async fn send(&self, packet: &[u8]) -> anyhow::Result<()>;

    /// Returns the local node's virtual IPv4 address.
    fn virtual_ipv4(&self) -> Ipv4Addr;
}