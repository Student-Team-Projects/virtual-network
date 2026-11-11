use std::io;
use std::net::Ipv4Addr;

use anyhow::{Context, ensure};
use async_trait::async_trait;
use tun_rs::{AsyncDevice, DeviceBuilder};
use virtual_network_core::RECV_BUFFER_SIZE;
use virtual_network_core::driver::DeviceDriver;
use virtual_network_core::network::VirtualNetwork;
use virtual_network_core::packet::parse_ipv4;

pub const TUN_MTU: u16 = 1280;

/// Linux limits interface names to IFNAMSIZ - 1 bytes.
const MAX_IFNAME_LEN: usize = 15;

/// A Linux TUN interface carrying the virtual network's IPv4 traffic.
///
/// The interface is created and brought up in [`TunDriver::new`] and removed
/// when the driver is dropped. Creating it requires root or `CAP_NET_ADMIN`.
pub struct TunDriver {
    device: AsyncDevice,
    ipv4_address: Ipv4Addr,
}

impl TunDriver {
    /// Creates the TUN interface `name` with address `ipv4_address` inside `network`.
    ///
    /// The kernel routes every destination inside `network` into this
    /// interface.
    pub fn new(name: &str, ipv4_address: Ipv4Addr, network: &VirtualNetwork) -> anyhow::Result<Self> {
        ensure!(
            !name.is_empty() && name.len() <= MAX_IFNAME_LEN,
            "interface name {name:?} must be 1..={MAX_IFNAME_LEN} bytes"
        );
        ensure!(
            network.is_host(ipv4_address),
            "{ipv4_address} is not a host address in the virtual network {network}"
        );

        let device = DeviceBuilder::new()
            .name(name)
            .ipv4(ipv4_address, network.prefix_len(), None)
            .mtu(TUN_MTU)
            .build_async()
            .map_err(|err| match err.kind() {
                io::ErrorKind::PermissionDenied => anyhow::Error::new(err).context(format!(
                    "creating TUN interface {name} requires root or CAP_NET_ADMIN"
                )),
                _ => anyhow::Error::new(err)
                    .context(format!("failed to create TUN interface {name}")),
            })?;

        Ok(TunDriver { device, ipv4_address })
    }

    /// Returns the interface name as reported by the kernel.
    pub fn name(&self) -> anyhow::Result<String> {
        self.device.name().context("failed to read TUN interface name")
    }
}

#[async_trait]
impl DeviceDriver for TunDriver {
    async fn recv(&self, buf: &mut [u8; RECV_BUFFER_SIZE]) -> anyhow::Result<usize> {
        // The kernel also emits IPv6 traffic (router solicitations, MLD, ...)
        // on the interface; the trait only promises IPv4, so skip anything
        // that isn't a valid IPv4 packet.
        loop {
            let len = self
                .device
                .recv(buf)
                .await
                .context("failed to read from TUN interface")?;

            if parse_ipv4(&buf[..len]).is_some() {
                return Ok(len);
            }
        }
    }

    async fn send(&self, packet: &[u8]) -> anyhow::Result<()> {
        // Without packet info, Linux picks the protocol from the version
        // nibble, so this is what stops a peer from injecting IPv6 or
        // malformed packets.
        ensure!(
            parse_ipv4(packet).is_some(),
            "refusing to write an invalid IPv4 packet to TUN"
        );

        let written = self
            .device
            .send(packet)
            .await
            .context("failed to write to TUN interface")?;
        ensure!(
            written == packet.len(),
            "short write to TUN: {written}/{} bytes",
            packet.len()
        );
        Ok(())
    }

    fn virtual_ipv4(&self) -> Ipv4Addr {
        self.ipv4_address
    }
}
