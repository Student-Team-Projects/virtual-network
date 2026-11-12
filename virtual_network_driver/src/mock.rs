//! In-memory mock [`DeviceDriver`] that works without root privileges.
//!
//! [`MockDriver::new`] returns the driver together with a [`MockHandle`], which plays the role
//! of the operating system:
//!
//! - [`MockHandle::inject`] simulates a local application sending a packet into the virtual
//!   interface. The packet comes out of [`DeviceDriver::recv`].
//! - [`MockHandle::next_delivered`] returns packets that were written with [`DeviceDriver::send`],
//!   i.e. packets the router delivered to the local OS.

use std::net::Ipv4Addr;

use anyhow::{Context, bail};
use async_trait::async_trait;
use tokio::sync::{Mutex, mpsc};
use virtual_network_core::RECV_BUFFER_SIZE;
use virtual_network_core::driver::DeviceDriver;

pub struct MockDriver {
    virtual_ipv4: Ipv4Addr,
    /// Packets injected by the handle, returned from `recv`.
    outbound: Mutex<mpsc::UnboundedReceiver<Vec<u8>>>,
    /// Packets written with `send`, read by the handle.
    inbound: mpsc::UnboundedSender<Vec<u8>>,
}

/// The "OS side" of a [`MockDriver`].
pub struct MockHandle {
    outbound: mpsc::UnboundedSender<Vec<u8>>,
    inbound: mpsc::UnboundedReceiver<Vec<u8>>,
}

impl MockDriver {
    pub fn new(virtual_ipv4: Ipv4Addr) -> (Self, MockHandle) {
        let (outbound_tx, outbound_rx) = mpsc::unbounded_channel();
        let (inbound_tx, inbound_rx) = mpsc::unbounded_channel();

        let driver =
            MockDriver { virtual_ipv4, outbound: Mutex::new(outbound_rx), inbound: inbound_tx };
        let handle = MockHandle { outbound: outbound_tx, inbound: inbound_rx };
        (driver, handle)
    }
}

#[async_trait]
impl DeviceDriver for MockDriver {
    async fn recv(&self, buf: &mut [u8; RECV_BUFFER_SIZE]) -> anyhow::Result<usize> {
        let packet =
            self.outbound.lock().await.recv().await.context("mock driver handle was dropped")?;

        // `MockHandle::inject` should reject packets that don't fit.
        debug_assert!(packet.len() <= buf.len());
        buf[..packet.len()].copy_from_slice(&packet);
        Ok(packet.len())
    }

    async fn send(&self, packet: &[u8]) -> anyhow::Result<()> {
        self.inbound.send(packet.to_vec()).context("mock driver handle was dropped")
    }

    fn virtual_ipv4(&self) -> Ipv4Addr {
        self.virtual_ipv4
    }
}

impl MockHandle {
    /// Simulates a local application sending `packet` into the virtual interface.
    ///
    /// Fails if `packet` is larger than [`RECV_BUFFER_SIZE`] or the driver was dropped.
    pub fn inject(&self, packet: impl Into<Vec<u8>>) -> anyhow::Result<()> {
        let packet = packet.into();
        if packet.len() > RECV_BUFFER_SIZE {
            bail!(
                "injected packet is {} bytes, larger than the {} byte receive buffer",
                packet.len(),
                RECV_BUFFER_SIZE
            );
        }

        self.outbound.send(packet).ok().context("mock driver was dropped")
    }

    /// Waits for the next packet the driver delivered to the OS.
    ///
    /// Returns `None` once the driver is dropped and all delivered packets were read.
    pub async fn next_delivered(&mut self) -> Option<Vec<u8>> {
        self.inbound.recv().await
    }

    /// Returns the next delivered packet without waiting, if there is one.
    pub fn try_next_delivered(&mut self) -> Option<Vec<u8>> {
        self.inbound.try_recv().ok()
    }
}
