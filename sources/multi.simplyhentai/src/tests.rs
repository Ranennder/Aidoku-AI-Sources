use super::*;
use aidoku_test::aidoku_test;

const RECORD: &str = r#"{"preview":{"sizes":{"full":"https://images.example/cover.jpg","thumb":"https://images.example/thumb.jpg"}},"series":{"slug":"sample-series","title":"Sample Series"},"slug":"sample-gallery","title":"Neutral fixture"}"#;

#[aidoku_test]
fn all_ten_language_settings_map_to_the_actual_api_names() {
	for code in ["en", "ja", "zh", "ko", "es", "ru", "fr", "de", "it", "pl"] {
		let (slug, title) = language(code);
		assert!(!slug.is_empty());
		assert!(title.starts_with(|c: char| c.is_ascii_uppercase()));
	}
	assert_eq!(language("ru"), ("russian", "Russian"));
	assert_eq!(language("unknown"), ("english", "English"));
}

#[aidoku_test]
fn listings_use_language_tags_and_latest_order_with_one_based_pages() {
	assert_eq!(
		listing_url(0, true, "english"),
		"https://api.simply-hentai.com/v3/tag/english?type=language&page=1&sort=newest"
	);
	assert!(listing_url(2, false, "russian").ends_with("?type=language&page=2"));
}

#[aidoku_test]
fn complex_search_encodes_unicode_and_filters_without_query_injection() {
	let url = search_url(
		"пример & page=9",
		2,
		"Russian",
		aidoku::alloc::vec![FilterValue::Text {
			id: "artists".into(),
			value: "One, Two".into()
		}],
		"blocked,neutral",
	);
	assert!(url.contains("%26%20page%3D9"));
	assert!(url.contains("filter%5Blanguage%5D%5B0%5D=Russian"));
	assert!(url.contains("filter%5Bartists%5D%5B1%5D=Two"));
	assert!(url.contains("blacklist=blocked%2Cneutral"));
}

#[aidoku_test]
fn record_identity_retains_series_and_gallery_slug() {
	let record: Record = serde_json::from_str(RECORD).unwrap();
	let manga = record.into_manga().unwrap();
	assert_eq!(manga.key, "/sample-series/sample-gallery");
	assert_eq!(
		manga.cover.as_deref(),
		Some("https://images.example/thumb.jpg")
	);
}

#[aidoku_test]
fn foreign_urls_and_encoded_path_boundaries_are_rejected() {
	assert_eq!(
		key("https://www.simply-hentai.com/sample-series/sample-gallery/?q=1").unwrap(),
		"/sample-series/sample-gallery"
	);
	for value in [
		"https://example.com/a/b",
		"/a/../b",
		"/a/%2Fb",
		"https://www.simply-hentai.com.example/a/b",
	] {
		assert!(key(value).is_err());
	}
}

#[aidoku_test]
fn malformed_or_missing_record_data_is_not_a_successful_empty_gallery() {
	assert!(serde_json::from_str::<Envelope<Album>>(r#"{"message":"unavailable"}"#).is_err());
	assert!(serde_json::from_str::<Record>(r#"{"title":"missing identity"}"#).is_err());
	let empty: List<Albums> =
		serde_json::from_str(r#"{"pagination":{"next":null},"data":{"albums":[]}}"#).unwrap();
	assert!(empty.data.albums.is_empty());
	assert!(empty.pagination.next.is_none());
}

#[aidoku_test]
fn details_and_the_single_chapter_keep_scanlators_and_utc_upload_date() {
	let json = RECORD.replace("\"title\":\"Neutral fixture\"", "\"title\":\"Neutral fixture\",\"artists\":[{\"slug\":\"creator\",\"title\":\"Creator\"}],\"translators\":[{\"slug\":\"group\",\"title\":\"Group\"}],\"created_at\":\"2026-11-04T00:00:00Z\"");
	let album: Album = serde_json::from_str(&json).unwrap();
	let result = album.update(Manga::default(), true, true).unwrap();
	assert_eq!(
		result.authors.unwrap(),
		aidoku::alloc::vec![String::from("Creator")]
	);
	let chapter = &result.chapters.unwrap()[0];
	assert_eq!(chapter.key, "/sample-series/sample-gallery/all-pages");
	assert_eq!(chapter.scanlators.as_ref().unwrap()[0], "Group");
	assert!(chapter.date_uploaded.is_some());
}

#[aidoku_test]
fn chapters_only_refresh_preserves_existing_details() {
	let album: Album = serde_json::from_str(RECORD).unwrap();
	let result = album
		.update(
			Manga {
				key: "/sample-series/sample-gallery".into(),
				title: "Retained title".into(),
				..Default::default()
			},
			false,
			true,
		)
		.unwrap();
	assert_eq!(result.title, "Retained title");
	assert_eq!(result.chapters.unwrap().len(), 1);
}

#[aidoku_test]
fn pages_follow_numeric_page_numbers_and_preserve_full_image_query_strings() {
	let pages: Pages = serde_json::from_str(r#"{"pages":[{"page_num":10,"sizes":{"full":"https://images.example/10.gif?rev=2","thumb":"t"}},{"page_num":2,"sizes":{"full":"https://images.example/2.jpg","thumb":"t"}}]}"#).unwrap();
	let result = parse_pages(pages).unwrap();
	let PageContent::Url(first, _) = &result[0].content else {
		panic!("image expected")
	};
	let PageContent::Url(last, _) = &result[1].content else {
		panic!("image expected")
	};
	assert!(first.ends_with("/2.jpg"));
	assert!(last.ends_with("/10.gif?rev=2"));
}

#[aidoku_test]
fn empty_or_invalid_page_lists_report_an_error() {
	assert!(parse_pages(Pages { pages: Vec::new() }).is_err());
	let invalid: Pages =
		serde_json::from_str(r#"{"pages":[{"sizes":{"full":"javascript:invalid","thumb":"t"}}]}"#)
			.unwrap();
	assert!(parse_pages(invalid).is_err());
}
