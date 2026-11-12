use std::net::Ipv4Addr;

use async_trait::async_trait;
use virtual_network_core::RECV_BUFFER_SIZE;

pub struct PeerLink {}

#[async_trait]
impl virtual_network_core::mesh::PeerLink for PeerLink {
    fn virtual_ipv4(&self) -> Ipv4Addr {
        todo!()
    }

    fn name(&self) -> String {
        todo!()
    }

    async fn send_packet(&self, packet: &[u8]) -> anyhow::Result<()> {
        todo!()
    }

    async fn recv_packet(&self, buf: &mut [u8; RECV_BUFFER_SIZE]) -> anyhow::Result<usize> {
        todo!()
    }
}
