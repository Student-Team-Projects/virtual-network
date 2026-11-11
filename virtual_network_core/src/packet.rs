use std::net::Ipv4Addr;

use etherparse::Ipv4Slice;

/// The addresses of a validated IPv4 packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ipv4Header {
    pub source: Ipv4Addr,
    pub destination: Ipv4Addr,
}

/// Validates that `packet` is exactly one well-formed IPv4 packet and returns
/// its addresses.
///
/// Checks the version, header length (IHL) and total length against the
/// buffer. Only the IP header is inspected, never the payload.
pub fn parse_ipv4(packet: &[u8]) -> Option<Ipv4Header> {
    let slice = Ipv4Slice::from_slice(packet).ok()?;
    let header = slice.header();

    // `from_slice` tolerates trailing bytes after the packet; a packet
    // buffer must not have any.
    if usize::from(header.total_len()) != packet.len() {
        return None;
    }

    Some(Ipv4Header {
        source: header.source_addr(),
        destination: header.destination_addr(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use etherparse::PacketBuilder;

    fn udp_packet(source: [u8; 4], destination: [u8; 4]) -> Vec<u8> {
        let builder = PacketBuilder::ipv4(source, destination, 64).udp(1000, 2000);
        let payload = b"hello";
        let mut packet = Vec::with_capacity(builder.size(payload.len()));
        builder.write(&mut packet, payload).unwrap();
        packet
    }

    #[test]
    fn parses_valid_packet() {
        let packet = udp_packet([100, 64, 0, 1], [100, 64, 0, 2]);
        assert_eq!(
            parse_ipv4(&packet),
            Some(Ipv4Header {
                source: Ipv4Addr::new(100, 64, 0, 1),
                destination: Ipv4Addr::new(100, 64, 0, 2),
            })
        );
    }

    #[test]
    fn rejects_ipv6() {
        let builder = PacketBuilder::ipv6([0; 16], [0; 16], 64).udp(1000, 2000);
        let mut packet = Vec::new();
        builder.write(&mut packet, b"hello").unwrap();
        assert_eq!(parse_ipv4(&packet), None);
    }

    #[test]
    fn rejects_truncated_and_padded() {
        let packet = udp_packet([100, 64, 0, 1], [100, 64, 0, 2]);
        assert_eq!(parse_ipv4(&packet[..packet.len() - 1]), None);
        assert_eq!(parse_ipv4(&packet[..10]), None);
        assert_eq!(parse_ipv4(&[]), None);

        let mut padded = packet.clone();
        padded.push(0);
        assert_eq!(parse_ipv4(&padded), None);
    }

    #[test]
    fn rejects_bad_ihl() {
        let mut packet = udp_packet([100, 64, 0, 1], [100, 64, 0, 2]);
        packet[0] = 0x44; // IHL 4 (16 bytes) is below the minimum of 5
        assert_eq!(parse_ipv4(&packet), None);
    }
}
