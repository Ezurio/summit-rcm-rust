#![cfg(feature = "api-v2")]

#[path = "support/live_hid_support.rs"]
mod support;

use std::{fs::File, io::Read};
use summit_rcm_bluetooth_hid::WEB_PUBLICATION;

use support::{
    MockBluezHarness, TEST_DEVICE_ADDRESS, compile_uhid_simulator, reserve_tcp_port,
    run_live_hid_connect_test, spawn_uhid_simulator, wait_for_hidraw_by_uniq,
};

#[test]
#[ignore = "requires root access to /dev/uhid and /dev/hidraw* plus a local gcc toolchain"]
fn live_uhid_scanner_emits_expected_hid_reports() {
    let uniq = TEST_DEVICE_ADDRESS;
    let simulator = compile_uhid_simulator();
    let mut child = spawn_uhid_simulator(&simulator, uniq, "ABC123");

    let hidraw = wait_for_hidraw_by_uniq(uniq);
    let mut file = File::open(&hidraw).expect("hidraw device should be readable");
    let mut reports = Vec::new();
    let mut buffer = [0u8; 8];
    for _ in 0..14 {
        file.read_exact(&mut buffer)
            .expect("expected UHID simulator to emit a full 8-byte report");
        reports.push(buffer);
    }

    let status = child.wait().expect("simulator process should exit cleanly");
    assert!(status.success(), "simulator should exit successfully");

    assert_eq!(reports[0], [0x02, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00]);
    assert_eq!(reports[1], [0x00; 8]);
    assert_eq!(reports[2], [0x02, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00]);
    assert_eq!(reports[3], [0x00; 8]);
    assert_eq!(reports[4], [0x02, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00]);
    assert_eq!(reports[5], [0x00; 8]);
    assert_eq!(reports[6], [0x00, 0x00, 0x1E, 0x00, 0x00, 0x00, 0x00, 0x00]);
    assert_eq!(reports[7], [0x00; 8]);
    assert_eq!(reports[8], [0x00, 0x00, 0x1F, 0x00, 0x00, 0x00, 0x00, 0x00]);
    assert_eq!(reports[9], [0x00; 8]);
    assert_eq!(
        reports[10],
        [0x00, 0x00, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00]
    );
    assert_eq!(reports[11], [0x00; 8]);
    assert_eq!(
        reports[12],
        [0x00, 0x00, 0x28, 0x00, 0x00, 0x00, 0x00, 0x00]
    );
    assert_eq!(reports[13], [0x00; 8]);
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "requires root access to /dev/uhid and /dev/hidraw* plus a local gcc toolchain"]
async fn live_uhid_hid_connect_streams_barcode_with_mock_bluez() {
    let _ = WEB_PUBLICATION.name;
    let harness = MockBluezHarness::start()
        .await
        .expect("mock bluez harness should start");
    let simulator = compile_uhid_simulator();
    let tcp_port = reserve_tcp_port();

    run_live_hid_connect_test(harness, simulator, tcp_port).await;
}
