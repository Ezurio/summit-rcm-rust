use crate::service::NetworkService;

#[tokio::test(flavor = "current_thread")]
async fn get_interface_statistics_fails_for_missing_interface() {
    let result = NetworkService::get_interface_statistics("__missing_interface__").await;
    assert!(result.is_err(), "missing interface statistics should fail");
}
