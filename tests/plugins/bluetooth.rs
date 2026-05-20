#[cfg(feature = "bluetooth")]
#[path = "bluetooth/general.rs"]
mod general;
#[cfg(all(feature = "bluetooth", feature = "bluetooth-hid"))]
#[path = "bluetooth/hid.rs"]
mod hid;
#[cfg(feature = "bluetooth-websocket")]
#[path = "bluetooth/websocket.rs"]
mod websocket;
