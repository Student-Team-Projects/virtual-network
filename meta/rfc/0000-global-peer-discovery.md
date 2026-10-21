# RFC #0000: Global Peer discovery

Status: **Exploring**

## Abstract

This document describes the different approaches for globally distributed peer
discovery.

## Introduction

We want to avoid the requirement of having an always online node with a static
public IP in each virtual network. This is a result of these goals specified in [RFC #0001]:

- *Avoid single points of failure*
- *Peers should not require a public IP address*

We therefore need a way for peers to find the *connection addresses*[^1] of other
peers already in the network.

Finding the *connection address* of a peer inside on the same LAN can usually be
implemented relatively easily (for example using mDNS).
Similarly, finding addresses of "peers of peers" can be implemented easily,
for example by broadcasting and forwarding *connection addresses* inside
an partially established virtual network.
The details of these methods will not be covered in this document.

This document will focus on methods of discovering peer *connection addresses*
over the Internet.

## Distributed Hash Tables

A *Distributed Hash Table*, for example [Kademlia DHT],
is a distributed key -> value "database", which avoids single points of failure
and is resilient to nodes dropping from the network. Read the linked article for
more details.

## Finding peers using a Distributed Hash Table

A *Distributed Hash Table* can be used for virtual network peer discovery.
Conceptually it works like this:

Let's say that node *A* wants to announce its *connection address* for discovery.
For this let's say that it has a unique and hard to forge Peer ID (*PID A*).
To start, Node *A* has to be able to reach any node participating in the DHT
(this is usually archived by having multiple stable bootstrap nodes with known
addresses and by caching the addresses of nodes met before).
By recursively examining the neighbors of known nodes, it is able to locate
the specific node responsible for storing the value for the key *PID A*.
It asks that node to store *A*'s *connection address* as the value for the key
*PID A* (with additional cryptographic verification).

Now let's say that node *C* wants to find the *connection address* of the node
with Peer ID *PID A*. It connects to the DHT (potentially to a different
starting node) and recursively finds the DHT node for key *PID A*.
It then queries that node for entry with key *PID A*, performs a cryptographic
check to confirm the record has not been forged, a hopefully received the value
stored for that key - the *connection address* announced by node *A*.

## Existing public DHTs

We are considering three options for the public DHT used for global peer
discovery:

### Amino (IPFS Kademlia DHT)

Spec: <https://specs.ipfs.tech/routing/kad-dht/>

A large public DHT, based on [libp2p Kademlia DHT], which supports the following
record types:
1. Peer Records: [Peer ID] -> [multiaddress]
2. [Provider Records]: CID (Content ID, a somewhat arbitrary key) -> append only
list of [Peer ID]
3. [IPNS Routing Records]: Public key *x* -> CID, signed with *x*

The Amino DHT can also be used to discover [Circuit Relay v2] servers,
useful for sending short messages and hole punching to nodes behind NAT.

### BitTorrent Mainline DHT

Spec: [BEP-5] (DHT specification) + [BEP-44] (arbitrary record storage)

Another public Kademlia (but not libp2p) based DHT. The [BEP-44] extension
supports these record types, with the values `v` no longer than 1000 bytes:
1. Immutable items: `sha1(v)` -> value `v`
2. Mutable items: Public key `x` -> value `v` signed with `x` + a sequence
number

### Custom [libp2p Kademlia DHT]-based DHT for this project

This DHT could be made to store the record types supported by the Amino DHT and
any other custom record types which we can imagine.

## Possible uses of these DHTs in this project

### Amino DHT

A single VPN would have an associated CID and all peers would register as
providers for this CID.

- **pro**: Single DHT using the libp2p framework
- **pro**: Access to public [Circuit Relay v2] servers
- **con**: Unencrypted provider records
  - possible to discover all peers in a single VPN network just by knowing its CID
  - possible to "spam" the provider records with uncooperative peers

### Custom DHT

- **pro**: Elegant, single DHT and libp2p can be used
- **pro**: Arbitrary record types can support signed and encrypted VPN peers
  lists and VPN metadata records
- **pro**: Good extensibility in the future
- **con**: Hard to establish "critical mass"
- **con**: Requires own bootstrap servers
- **con**: Requires own relay and hole punching servers
- **con**: A small network is more susceptible to attacks

### Amino DHT + Custom DHT
- Amino DHT used for:
  - Finding bootstrap peers on the Custom DHT
  - Finding public [Circuit Relay v2] servers
- Custom DHT used for:
  - Storing signed and encrypted VPN peers lists and VPN metadata records
- **pro** (Custom DHT): Arbitrary record types can support signed and encrypted VPN peers
  lists and VPN metadata records
- **pro** (Custom DHT): Good extensibility in the future
- **pro**: libp2p can be used for everything
- **pro** (Amino DHT): Access to public [Circuit Relay v2] servers
- **con**: Using two DHTs complicates the system
- **con** (Custom DHT): Hard to establish "critical mass"
- **con** (Custom DHT): A small network is more susceptible to attacks

### BitTorrent Mainline DHT

- **pro**: Arbitrary record types can support signed and encrypted VPN peers
  lists and VPN metadata records
- **pro**: Single DHT
- **con**: No public relay and hole punching networks
- **con**: Does not use libp2p

### Amino DHT + BitTorrent Mainline DHT
- Amino DHT used for:
  - Finding public [Circuit Relay v2] servers
- BitTorrent Mainline DHT used for:
  - Storing signed and encrypted VPN peers lists and VPN metadata records
- **pro** (BitTorrent): Arbitrary record types can support signed and encrypted VPN peers
  lists and VPN metadata records
- **pro** (Amino DHT): Access to public [Circuit Relay v2] servers
- **con**: Using two DHTs complicates the system, especially with two different
  standards. Amino DHT used only for relays

### BitTorrent Mainline DHT + another public relay network?
Perhaps there is another public relay network, less restrictive than [Circuit Relay v2]?
We would still get the custom record type benefits of Amino DHT.

[^1]: By *connection address* we mean all information necessary to reach a node from the Internet, perhaps through intermediaries (relays).

[RFC #0001]: ./0001-design-goals.md
[Kademlia DHT]: https://docs.ipfs.tech/concepts/dht/#kademlia
[libp2p Kademlia DHT]: https://libp2p.io/docs/dht/
[Peer ID]: https://github.com/libp2p/specs/blob/master/peer-ids/peer-ids.md
[multiaddress]: https://github.com/libp2p/specs/blob/master/addressing/README.md
[Provider Records]: https://specs.ipfs.tech/routing/kad-dht/#provider-record-routing
[IPNS Routing Records]: https://specs.ipfs.tech/ipns/ipns-record/#routing-record
[Circuit Relay v2]: https://github.com/libp2p/specs/blob/master/relay/circuit-v2.md
[BEP-5]: https://www.bittorrent.org/beps/bep_0005.html
[BEP-44]: https://bittorrent.org/beps/bep_0044.html
