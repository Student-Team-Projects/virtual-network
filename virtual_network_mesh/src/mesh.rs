use crate::link;
use tokio::sync::{mpsc, oneshot};
use virtual_network_core::mesh::MeshConfig;
use virtual_network_core::mesh::MeshEvent;

/// A joined network. Dropping it leaves the network.
pub struct Mesh {
    /// Tells the background task to leave the network and close all peer links.
    shutdown: Option<oneshot::Sender<()>>,
}

impl virtual_network_core::mesh::Mesh for Mesh {
    type MeshPeerLink = link::PeerLink;

    fn join(
        config: MeshConfig,
    ) -> anyhow::Result<(Self, mpsc::Receiver<MeshEvent<link::PeerLink>>)> {
        todo!()
    }
}

impl Drop for Mesh {
    fn drop(&mut self) {
        // `Drop` can't be async, so this only signals the background task, which then leaves
        // the network on its own.
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
    }
}
