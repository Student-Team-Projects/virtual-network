use std::net::Ipv4Addr;

use async_trait::async_trait;
use virtual_network_core::RECV_BUFFER_SIZE;

struct Driver {}

impl Driver {
    fn new() -> Self {
        Driver {}
    }
}

#[async_trait]
impl virtual_network_core::driver::DeviceDriver for Driver {
    async fn recv(&self, buf: &mut [u8; RECV_BUFFER_SIZE]) -> anyhow::Result<usize> {
        todo!()
    }

    async fn send(&self, packet: &[u8]) -> anyhow::Result<()> {
        todo!()
    }

    fn virtual_ipv4(&self) -> Ipv4Addr {
        todo!()
    }
}
