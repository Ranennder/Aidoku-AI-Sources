use super::*;
use aidoku_test::aidoku_test;

fn chapter(id: i32, volume: i32, language: &str) -> MangaChapter {
	serde_json::from_value(serde_json::json!({"id":id,"language":language,"chapter_number":"1.5","volume_number":volume,"date_added":"2026-01-01 12:00:00+0000"})).unwrap()
}

#[aidoku_test]
fn pointer_table_decodes_route_payload_arrays_and_null_references() {
	let table = serde_json::json!([{"_1":2},"data",{"_3":4,"_5":-1},"items",[6],"missing",{"_7":8},"title","Fixture title"]);
	let decoded = helpers::resolve_ptr_table_json(table.as_array().unwrap(), 0).unwrap();
	assert_eq!(
		decoded,
		serde_json::json!({"data":{"items":[{"title":"Fixture title"}],"missing":null}})
	);
	assert!(helpers::resolve_ptr_table_json(&[], 1).is_err());
}

#[aidoku_test]
fn page_and_cover_links_preserve_absolute_cdn_urls() {
	assert_eq!(
		base_url_join("/uploads/fixture.jpg"),
		"https://mangadot.net/uploads/fixture.jpg"
	);
	assert_eq!(
		base_url_join("uploads/fixture.jpg"),
		"https://mangadot.net/uploads/fixture.jpg"
	);
	assert_eq!(
		base_url_join("https://cdn.example/fixture.jpg?revision=2"),
		"https://cdn.example/fixture.jpg?revision=2"
	);
	assert_eq!(
		base_url_join("//cdn.example/fixture.webp"),
		"https://cdn.example/fixture.webp"
	);
}

#[aidoku_test]
fn optional_chapter_deduplication_keeps_different_volumes_and_languages() {
	let mut map = HashMap::new();
	dedup_insert(&mut map, chapter(1, 1, "en"));
	dedup_insert(&mut map, chapter(2, 2, "en"));
	dedup_insert(&mut map, chapter(3, 1, "ru"));
	dedup_insert(&mut map, chapter(4, 1, "en"));
	assert_eq!(map.len(), 3);
	let mut untitled = chapter(5, 1, "en");
	untitled.chapter_number = None;
	dedup_insert(&mut map, untitled);
	let mut other = chapter(6, 1, "en");
	other.chapter_number = None;
	dedup_insert(&mut map, other);
	assert_eq!(map.len(), 5);
}

#[aidoku_test]
fn chapter_keys_remain_numeric_and_user_upload_route_is_retained() {
	let mut value = chapter(42, 3, "en");
	value.uploader_username = Some("fixture-reader".into());
	value.chapter_title = Some("A synthetic chapter".into());
	let value: Chapter = value.into();
	assert_eq!(value.key, "42");
	assert_eq!(value.chapter_number, Some(1.5));
	assert_eq!(value.volume_number, Some(3.0));
	assert_eq!(
		value.url.as_deref(),
		Some("https://mangadot.net/chapter/42?source=user")
	);
}

#[aidoku_test]
fn unconfigured_language_selection_can_load_english_chapters() {
	assert_eq!(get_languages().unwrap(), vec![String::from("en")]);
}

#[aidoku_test]
fn details_parse_string_booleans_authors_and_longstrip_metadata() {
	let value: MangaItem = serde_json::from_str(r#"{"id":123,"title":"Fixture title","photo":"/fixture.jpg","status":"Completed","is_adult":"false","is_longstrip":1,"country_of_origin":"JP","authors":"[\"Fixture Author\"]","artists":["Fixture Artist"],"genres":["Adventure"]}"#).unwrap();
	let manga: Manga = value.into();
	assert_eq!(manga.key, "123");
	assert_eq!(manga.authors.as_ref().unwrap()[0], "Fixture Author");
	assert_eq!(manga.artists.as_ref().unwrap()[0], "Fixture Artist");
	assert_eq!(manga.viewer, aidoku::Viewer::Webtoon);
	assert_eq!(manga.status, aidoku::MangaStatus::Completed);
}
