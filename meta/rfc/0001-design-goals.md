# [RFC #0001]: Design goals
Status: **Proposal**

## Abstract
Introduction of the initial project design goals

## Introduction

This project is intended as an alternative to commercial, cloud hosted VPNs
(NetBird Cloud, Tailscale). It will allow users to setup a layer 3 Virtual
Private Network (VPN), relying only on public or self-hosted infrastructure.

## Design goals

### Don't reinvent the wheel

Prefer established protocols with existing implementations instead of creating
everything from scratch, where possible.

### Prioritize ease of use

The initial setup and peer enrollment processes should not require understating
advanced networking concepts.
Ideally, it should not be more involved than using a cloud hosted VPN.

### Avoid vendor lock-in

Do not rely on any single service on the Internet.
Any required infrastructure must be possible to self-host.

### Avoid single points of failure

Prefer decentralized, peer to peer solutions instead of a single primary server.

### Peers should not require a public IP address

Hosts which do not have a public IP address (for example because they are behind
one or more layers of NAT) or which cannot open incoming ports, should be able
to join the network.

Some connections will by necessity require a server with a public IP address
(for example if both peers are both behind NAT and hole punching doesn't work,
a relay server might be impossible to avoid).

### Infrastructure can be shared by many networks

Servers used for hole punching, relaying connections, peer discovery, etc.
should not be specific to a single network, so that they can be shared.

Use of public infrastructure, such as distributed hash tables, STUN/TURN servers
is preferable to avoid the need for self-hosting servers when deploying
a network.

Peers should not blindly trust any shared servers.

### Prepare for host roaming

The public IP address of a given host might change, peers should be able to
reestablish connections when this happens.

### Peers might be offline for extended periods of time

Some or all peers in the network might connect to it only occasionally.

To allow the use of this VPN as an backup remote host access, staying offline
for a long period of time should not prevent from reconnecting to the network.

There might be periods where no peers are online and the IP addresses of all
peers might change during such periods. The network should still restart after
that.

### Security

All connections must be authenticated.
All transfers over public networks should be encrypted.

### Privacy

The amount of knowledge about a given network that unauthenticated parties can
learn should be as little as possible.

### Portability

The protocols used by this projects should be as portable as possible.
The initial implementation in this repo does not.


[RFC #0001]: https://github.com/Student-Team-Projects/virtual-network/pull/1
