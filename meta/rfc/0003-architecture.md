# [RFC #0003]: Architecture

Status: **Accepted**

## Abstract

This document describes the internal architecture of the VPN client, the choice
of implementation language, and how the codebase is divided into independent
modules so that development work can be split across the team.

## Introduction

Based on [RFC #0001] and [RFC #0002], the client program has to handle
several distinct responsibilities:

- discovering peers (via the Amino DHT, LAN broadcast, and peer exchange),
  maintaining relay reservations, authenticating peers, and punching holes
  through NAT
- managing an OS-level virtual network interface (TUN), which requires elevated
  privileges
- forwarding raw IPv4 packets between the virtual interface and connected peers
  with low latency
- providing an easy-to-use interface (CLI or GUI) for creating and joining
  networks

Putting all of this into a single tightly coupled loop would make the code hard
to test (since creating a TUN interface requires root/Administrator rights) and
hard to work on in parallel.

Therefore, I propose separating the code into six components:

- Core: the main app logic and shared traits, doesn't depend on any other
  module.
- Driver: creates and configures the OS virtual network interface,
  depends on `core`.
- Mesh: manages DHT discovery, relays, hole punching, and peer
  authentication, depends on `core`.
- Router: moves the IP packets between the virtual interface and
  connected peers, depends on `core`.
- Daemon: the main background logic that initializes the modules, stores
  state on disk, and coordinates everything.
- UI: let the user create or join networks and see the connection
  status.

## Language and technology choices

I propose writing the client in Rust.

The main reason is `rust-libp2p`. Since [RFC #0002] relies on the Amino DHT,
[Circuit Relay v2], and hole punching, the client needs a mature `libp2p`
implementation. Official `libp2p` implementations with full support for these
protocols mainly exist in Go and Rust.

Rust is a good fit for the rest of the client as well:

- It compiles to a single native binary on both Linux and Windows without
  needing a separate runtime installed.
- Key building blocks already exist as crates: `libp2p` for P2P networking,
  `tokio` for async I/O and channels between modules, and `tun-rs` for managing
  OS virtual network interfaces across platforms.

## Components

The modules are organized so that `driver`, `mesh`, and `router` depend only on
`core`, not on each other. Once `core` is agreed on, those three modules can be
built and tested in parallel by different people.

### 1. Core

Pure domain logic and shared interfaces with no I/O, no async runtime, and no
OS-specific dependencies:

- **Network credentials and key derivation**: generating and validating the
  shared network secret, and deriving separate keys from it:
  - the `cid` used for Amino DHT provider records (see [RFC #0002]),
  - the key used to authenticate peers.
- **Packet inspection**: parsing raw IPv4 headers to read source and
  destination addresses, and classifying packets as unicast, broadcast,
  multicast, or invalid.
- **Shared traits (`DriverDevice` and `PeerLink`)**: the interfaces that connect
  the data plane to the OS network adapter and to peer connections.

### 2. Driver

Implementations of the `DriverDevice` trait:

- **OS TUN adapter**: uses `tun-rs` to create and configure the virtual
  interface and provide async packet read/write methods.
- **Mock adapter**: a simple in-memory implementation.
  This allows running automated tests of the router and daemon on any machine
  without root or Administrator privileges.

Keeping `tun-rs` behind the `DriverDevice` trait keeps OS-specific setup and
privilege requirements out of the rest of the codebase.

### 3. Mesh

Manages peer discovery, NAT traversal, and peer authentication using libp2p.
It does not inspect or route user IP packets.

Its responsibilities follow [RFC #0002]:

- **Relay reservations**: discovers public [Circuit Relay v2] servers (and any
  user-configured relays), keeps an active reservation, and switches to another
  relay if a reservation is refused or lost.
- **Peer discovery**:
  - publishes provider records for the network's `cid` on the Amino DHT (using
    the peer's relayed multiaddress so its public IP is not exposed to
    unauthenticated nodes)
  - queries the Amino DHT for providers of `cid`
  - discovers peers on the local network
  - exchanges lists of known connected peers with already-authenticated peers
  - attempts to reconnect to previously known peers using cached addresses
- **Authentication before upgrade**: when a connection is established through a
  relay, `mesh` runs the authentication handshake first. Only after both peers
  prove membership does it exchange direct addresses and attempt hole punching
  to upgrade to a direct connection. This avoids leaking public IP addresses to
  unauthorized parties.
- **Producing `PeerLink`s**: once a peer is authenticated, `mesh` opens a
  packet stream to that peer, wraps it in a `PeerLink`, and emits a
  `PeerVerified` event to the daemon.

### 4. Router

A switch that moves raw IP packets between a `DriverDevice` and active
`PeerLink`s:

- **Routing table**: maps each authenticated peer's virtual IP to its
  `PeerLink`. Updated by the daemon as peers connect and disconnect.
- **Outbound (egress) path**: reads packets from `DriverDevice`. Unicast packets
  are queued for the matching peer; broadcast and multicast packets are cloned
  to all active peers.
- **Inbound (ingress) path**: reads packets from each `PeerLink` and checks
  that the packet's source IP matches the sender's assigned virtual IP
  (preventing IP spoofing) before writing the packet to `DriverDevice`.
- **Bounded queues**: each peer has a bounded outbound queue. If a peer's
  connection stalls, excess packets to that peer are dropped rather than
  buffered without limit, so one slow peer cannot affect traffic to other
  peers.

Because `router` depends only on `core` (for `DriverDevice`, `PeerLink`, and
packet parsing), it knows nothing about libp2p or OS drivers.

### 5. Daemon

The central component that wires everything together:

- loads and saves persistent files in a local data directory (the node's
  keypair, joined network configuration, and cached peer addresses for offline
  restarts as required by [RFC #0001])
- initializes `driver`, `mesh`, and `router`
- runs the central event loop: when `mesh` reports a newly authenticated peer,
  the daemon registers its route in `router`; when a peer disconnects, it
  removes the route
- exposes channels so the CLI or GUI can check the current status and run
  commands (`create`, `join`, `leave`)

### 6. UI

Frontends that let the user control the client:

- **CLI**: terminal commands (`create`, `join`, `leave`, `status`, `peers`).
- **GUI**: a simple window showing the network code, local virtual IP,
  connected peers, whether each peer is relayed or direct, and ping latency.

## Typical flows

### Joining a network and connecting to a peer

1. The user runs `join <secret>` in the CLI or GUI, which sends the command to
   the `daemon`.
2. The `daemon` validates and saves the secret, derives the network keys using
   `core`, and sends `MeshCommand::Join` to `mesh`.
3. `mesh` reserves a slot on a public [Circuit Relay v2] server, publishes a
   provider record for `cid` on the Amino DHT, and queries existing providers
   (while also trying any cached peer addresses).
4. When a provider is found, `mesh` dials it through its relay and runs the
   authentication handshake.
5. Once authentication succeeds on both sides:
   - `mesh` emits `MeshEvent::PeerVerified { peer_id, link }` to the `daemon`,
   - `daemon` registers `(virtual_ip, link)` in `router`,
   - `mesh` exchanges known peer lists with the new peer and initiates hole
     punching to upgrade the relayed connection to a direct connection.

### Forwarding a packet

1. An application (such as a game or `ping`) sends an IPv4 packet to
   `100.64.x.y`.
2. The OS routes the packet to the virtual interface, where `router` reads it
   via `DriverDevice::recv`.
3. `router` uses `core` to inspect the destination IP, looks up the recipient's
   `PeerLink` in its routing table (or selects all links for broadcast), and
   sends the packet.
4. On the receiving machine, `router` reads the packet from `PeerLink`, verifies
   that the source IP matches the sender's virtual IP, and injects it into the
   OS via `DriverDevice::send`.


[RFC #0001]: ./0001-design-goals.md
[RFC #0002]: ./0002-global-peer-discovery.md
[RFC #0003]: https://github.com/Student-Team-Projects/virtual-network/pull/3
[Circuit Relay v2]: https://github.com/libp2p/specs/blob/master/relay/circuit-v2.md
