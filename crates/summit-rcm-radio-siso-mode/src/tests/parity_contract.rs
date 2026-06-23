#![cfg(all(feature = "radio-siso-mode", feature = "api-v2"))]

#[tokio::test]
async fn v2_radio_siso_invalid_payload_does_not_return_sdcerr() {
	use summit_rcm_web::axum::{body::to_bytes, response::IntoResponse, Json};

	let response = crate::routes::v2::put_radio_siso_mode(
		Json(crate::routes::v2::SisoModePut {
			siso_mode: 999,
		}),
	)
	.await
	.into_response();

	let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
	let body_text = String::from_utf8_lossy(&body);
	assert!(!body_text.contains("SDCERR"));
	assert!(!body_text.contains("InfoMsg"));
}
