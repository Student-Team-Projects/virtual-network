use std::sync::Arc;
use virtual_network_core::mesh::PeerLink;

struct Router {

}

impl Router {
    fn new() -> Self {
        Router {}
    }
}

impl virtual_network_core::router::Router for Router {
    fn add_peer_link(&self, link: Arc<dyn PeerLink>) {
        todo!()
    }

    fn remove_peer_link(&self, link: Arc<dyn PeerLink>) {
        todo!()
    }
}