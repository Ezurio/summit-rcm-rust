#[cfg(any(feature = "bluetooth", feature = "bluetooth-websocket"))]
#[path = "plugins/bluetooth.rs"]
mod bluetooth;

#[path = "plugins/parity_contract.rs"]
mod parity_contract;
