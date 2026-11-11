use std::fmt;
use std::net::Ipv4Addr;

use anyhow::ensure;

/// The virtual IPv4 subnet shared by all peers of a network.
///
/// Every node's virtual address must be a host address inside it, and the OS
/// routes every destination inside it into the virtual interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VirtualNetwork {
    network: Ipv4Addr,
    prefix_len: u8,
}

impl VirtualNetwork {
    /// Creates the network `network/prefix_len`.
    ///
    /// `prefix_len` must be between 1 and 30 (/31 and /32 leave no room for
    /// peers), and `network` must have no host bits set.
    pub fn new(network: Ipv4Addr, prefix_len: u8) -> anyhow::Result<Self> {
        ensure!(
            (1..=30).contains(&prefix_len),
            "prefix length {prefix_len} must be between 1 and 30"
        );

        let result = Self { network, prefix_len };
        ensure!(
            u32::from(network) & !result.mask() == 0,
            "{network}/{prefix_len} has host bits set"
        );
        Ok(result)
    }

    /// The network address, e.g. `100.64.0.0`.
    pub fn network(&self) -> Ipv4Addr {
        self.network
    }

    /// The prefix length, e.g. `10`.
    pub fn prefix_len(&self) -> u8 {
        self.prefix_len
    }

    /// The subnet mask, e.g. `255.192.0.0`.
    pub fn netmask(&self) -> Ipv4Addr {
        Ipv4Addr::from(self.mask())
    }

    /// The subnet broadcast address, e.g. `100.127.255.255`.
    pub fn broadcast(&self) -> Ipv4Addr {
        Ipv4Addr::from(u32::from(self.network) | !self.mask())
    }

    /// Whether `ip` is inside the network, including the network and broadcast
    /// addresses.
    pub fn contains(&self, ip: Ipv4Addr) -> bool {
        u32::from(ip) & self.mask() == u32::from(self.network)
    }

    /// Whether `ip` can be assigned to a node: inside the network, and neither
    /// the network nor the broadcast address.
    pub fn is_host(&self, ip: Ipv4Addr) -> bool {
        self.contains(ip) && ip != self.network && ip != self.broadcast()
    }

    fn mask(&self) -> u32 {
        u32::MAX << (32 - self.prefix_len)
    }
}

impl fmt::Display for VirtualNetwork {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.network, self.prefix_len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn net() -> VirtualNetwork {
        VirtualNetwork::new(Ipv4Addr::new(100, 64, 0, 0), 10).unwrap()
    }

    #[test]
    fn derived_addresses() {
        let net = net();
        assert_eq!(net.network(), Ipv4Addr::new(100, 64, 0, 0));
        assert_eq!(net.prefix_len(), 10);
        assert_eq!(net.to_string(), "100.64.0.0/10");
        assert_eq!(net.netmask(), Ipv4Addr::new(255, 192, 0, 0));
        assert_eq!(net.broadcast(), Ipv4Addr::new(100, 127, 255, 255));
    }

    #[test]
    fn rejects_invalid_prefix_len() {
        for prefix_len in [0, 31, 32, 33] {
            assert!(VirtualNetwork::new(Ipv4Addr::new(10, 0, 0, 0), prefix_len).is_err());
        }
    }

    #[test]
    fn rejects_host_bits() {
        assert!(VirtualNetwork::new(Ipv4Addr::new(100, 64, 0, 1), 10).is_err());
        assert!(VirtualNetwork::new(Ipv4Addr::new(100, 65, 0, 0), 10).is_err());
    }

    #[test]
    fn membership() {
        let net = net();

        assert!(net.is_host(Ipv4Addr::new(100, 64, 0, 1)));
        assert!(net.is_host(Ipv4Addr::new(100, 127, 255, 254)));

        assert!(net.contains(net.network()) && !net.is_host(net.network()));
        assert!(net.contains(net.broadcast()) && !net.is_host(net.broadcast()));

        assert!(!net.contains(Ipv4Addr::new(100, 128, 0, 1)));
        assert!(!net.contains(Ipv4Addr::new(100, 63, 255, 255)));
        assert!(!net.is_host(Ipv4Addr::new(10, 0, 0, 1)));
    }
}
