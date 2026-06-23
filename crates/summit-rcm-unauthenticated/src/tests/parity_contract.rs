#![cfg(all(feature = "unauthenticated", feature = "api-v2"))]

#[tokio::test]
async fn v2_unauthenticated_get_does_not_return_sdcerr() {
	use summit_rcm_web::axum::{body::to_bytes, response::IntoResponse};
	use summit_rcm_web::serde_json::Value;

	let response = crate::routes::v2::get_unauthenticated()
		.await
		.into_response();

	let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
	let payload: Value = summit_rcm_web::serde_json::from_slice(&body).unwrap();

	assert!(payload.get("SDCERR").is_none());
	assert!(payload.get("InfoMsg").is_none());
	assert!(payload.get("allowUnauthenticatedRebootReset").is_some());
}

#[tokio::test]
async fn v2_unauthenticated_put_delete_do_not_return_sdcerr() {
	use summit_rcm_web::axum::{body::to_bytes, response::IntoResponse};

	let put_response = crate::routes::v2::put_unauthenticated()
		.await
		.into_response();
	let put_body = to_bytes(put_response.into_body(), usize::MAX).await.unwrap();
	assert!(!String::from_utf8_lossy(&put_body).contains("SDCERR"));

	let delete_response = crate::routes::v2::delete_unauthenticated()
		.await
		.into_response();
	let delete_body = to_bytes(delete_response.into_body(), usize::MAX).await.unwrap();
	assert!(!String::from_utf8_lossy(&delete_body).contains("SDCERR"));
}
