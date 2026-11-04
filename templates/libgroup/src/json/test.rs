use super::*;
use crate::models::responses::{ChaptersResponse, MangaDetailResponse, MangaListResponse};
use aidoku_test::aidoku_test;

#[aidoku_test]
fn missing_manga_http_error_cannot_become_an_empty_metadata_update() {
	// The public API returns HTTP 404 with an object under `data`. Its unknown
	// error fields used to deserialize as an all-default manga with an empty key.
	let response = decode_response::<MangaDetailResponse>(
		404,
		br#"{"data":{"toast":{"type":"error","message":"Not found"}}}"#,
	);
	let AidokuError::Message(message) = response.err().unwrap() else {
		panic!("HTTP error expected")
	};
	assert!(message.contains("404"));
}

#[aidoku_test]
fn authentication_and_server_failures_cannot_clear_a_chapter_list() {
	for status in [401, 403, 429, 500, 503] {
		assert!(
			decode_response::<ChaptersResponse>(status, br#"{"message":"unavailable"}"#).is_err()
		);
	}
}

#[aidoku_test]
fn a_successful_empty_catalog_is_still_a_valid_result() {
	let result =
		decode_response::<MangaListResponse>(200, br#"{"data":[],"meta":{"has_next_page":false}}"#)
			.unwrap();
	assert!(result.data.is_empty());
	assert_eq!(result.meta.has_next_page, Some(false));
}

#[aidoku_test]
fn success_status_with_a_missing_data_envelope_is_rejected() {
	assert!(decode_response::<MangaDetailResponse>(200, br#"{"message":"unavailable"}"#).is_err());
	assert!(decode_response::<ChaptersResponse>(200, br#"{"message":"unavailable"}"#).is_err());
}

#[aidoku_test]
fn diagnostics_do_not_echo_response_bodies_or_credentials() {
	for status in [200, 401, 503] {
		let response =
			decode_response::<MangaListResponse>(status, b"not-json secret-token-cookie-value");
		let AidokuError::Message(message) = response.err().unwrap() else {
			panic!("safe error expected")
		};
		assert!(!message.contains("secret-token-cookie-value"));
	}
}
