use crate::mesh::PeerLink;
use std::sync::Arc;

pub trait Router {
    fn add_peer_link(&self, link: Arc<dyn PeerLink>);

    /// Removes the link that is the same `Arc` as `link` (compared with `Arc::ptr_eq`).
    fn remove_peer_link(&self, link: Arc<dyn PeerLink>);
}
