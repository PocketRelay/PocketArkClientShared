//! # Virtual Peering
//!
//! Unfortunately the [Dedicated Topology](https://jacobtread.com/blog/pocket-relay-tunnel/) tunneled networking
//! method does not work for Mass Effect Andromeda attempting to use that topology causes the game client to crash
//!
//! So instead of trying to force everyone to connect to a dedicated virtual peer we can use a virtual addressing
//! system thats intercepted by the local client to re-route the traffic to the tunneling socket system using the slot
//! encoded into the IP address

/// Port used for virtual peer addresses
pub const VIRTUAL_PEER_PORT: u16 = 7337;

/// IP address offset for virtual peer addresses
pub const VIRTUAL_PEER_ADDRESS_OFFSET: u8 = 24;

/// Checks if the address is a virtual peer address
pub fn is_virtual_peer_address(a: u8, b: u8, c: u8, d: u8, port: u16) -> bool {
    port == VIRTUAL_PEER_PORT && a == 127 && b == 0 && c == 0 && d >= VIRTUAL_PEER_ADDRESS_OFFSET
}

/// Get the slot from the virtual peer address
pub fn get_virtual_peer_address(d: u8) -> u8 {
    d.saturating_sub(VIRTUAL_PEER_ADDRESS_OFFSET)
}
