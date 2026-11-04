use super::*;
use aidoku_test::aidoku_test;

#[aidoku_test]
fn search_uses_the_live_mirror_and_zero_based_pages() {
	assert_eq!(base_url(), "https://kagane.moe");
	assert!(
		search_url(1, 35, "updated_at", false).ends_with("page=0&size=35&sort=updated_at,desc")
	);
	assert!(search_url(2, 35, "", true).ends_with("page=1&size=35"));
	assert!(search_url(0, 35, "", false).ends_with("page=0&size=35"));
}

#[aidoku_test]
fn search_body_encodes_text_and_keeps_default_content_and_language_filters() {
	let body: serde_json::Value =
		serde_json::from_str(&build_search_body(Some("A & B"), SearchFilters::default()).unwrap())
			.unwrap();
	assert_eq!(body["title"], "A & B");
	assert_eq!(body["content_lang"], serde_json::json!(["en"]));
	assert_eq!(body["is_disturbing"], false);
	assert_eq!(body["is_pornographic"], false);
	assert_eq!(
		body["age_rating"],
		serde_json::json!(["FifteenPlus", "Safe"])
	);
}

#[aidoku_test]
fn reordered_chapter_columns_preserve_ids_and_site_reading_order() {
	let index: BookIndex = serde_json::from_str(r#"{"fields":["title","book_id","volume_no","chapter_no","sort_no","released_at","group_ix"],"groups":[{"title":"Fixture Group"}],"books":[["First","book-a","1","12.5",2,1700000000,[0]],["Second","book-b","2","1",3,1700000100,[0]]]}"#).unwrap();
	let chapters = index.into_chapters("series-fixture", None);
	assert_eq!(chapters.len(), 2);
	assert_eq!(chapters[0].key, "book-b");
	assert_eq!(chapters[1].chapter_number, Some(12.5));
	assert_eq!(chapters[0].volume_number, Some(2.0));
	assert_eq!(chapters[0].date_uploaded, Some(1700000100));
	assert_eq!(chapters[0].scanlators.as_ref().unwrap()[0], "Fixture Group");
	assert_eq!(
		chapters[0].url.as_deref(),
		Some("https://kagane.moe/series/series-fixture/reader/book-b")
	);
}

#[aidoku_test]
fn manifest_pages_follow_page_numbers_and_keep_the_server_format() {
	let challenge: ChallengeResponse = serde_json::from_str(r#"{"cache_url":"https://images.example/","access_token":"fixture-token","manifest":{"pages":[{"page_no":2,"page_id":"second","ext":"webp"},{"page_no":1,"page_id":"first","ext":null}]}}"#).unwrap();
	let pages = pages_from_manifest(challenge, "book-fixture").unwrap();
	let PageContent::Url(first, _) = &pages[0].content else {
		panic!("URL page expected")
	};
	let PageContent::Url(second, _) = &pages[1].content else {
		panic!("URL page expected")
	};
	assert_eq!(
		first,
		"https://images.example/api/v2/books/page/book-fixture/first.jxl?token=fixture-token"
	);
	assert!(second.contains("/second.webp?token=fixture-token"));
}

#[aidoku_test]
fn inaccessible_manifest_is_an_error_instead_of_a_zero_page_chapter() {
	for manifest in ["null", "{\"pages\":[]}"] {
		let challenge: ChallengeResponse = serde_json::from_str(&format!(r#"{{"cache_url":"https://images.example","access_token":"fixture-token","manifest":{manifest}}}"#)).unwrap();
		assert!(pages_from_manifest(challenge, "book-fixture").is_err());
	}
}

#[aidoku_test]
fn catalogue_pagination_and_series_keys_come_from_the_api() {
	let page: SearchResponse = serde_json::from_str(r#"{"content":[{"series_id":"series-fixture","title":" Fixture Title ","cover_image_id":"cover-fixture"}],"last":false}"#).unwrap();
	assert!(!page.last);
	let manga = page.content.into_iter().next().unwrap().into_manga(&[]);
	assert_eq!(manga.key, "series-fixture");
	assert_eq!(manga.title, "Fixture Title");
	assert_eq!(
		manga.cover.as_deref(),
		Some("https://kagane.moe/api/v2/image/cover-fixture")
	);
}
