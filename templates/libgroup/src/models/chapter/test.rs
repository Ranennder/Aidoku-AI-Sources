use super::*;
use aidoku_test::aidoku_test;

fn chapters() -> Vec<LibGroupChapterListItem> {
	serde_json::from_str(
		r#"[
		{"volume":"1","number":"12.50","branches":[{"id":10,"branch_id":null},{"id":11,"branch_id":81}]},
		{"volume":"2","number":"12.50","branches":[{"id":12,"branch_id":82}]}
	]"#,
	)
	.unwrap()
}

#[aidoku_test]
fn chapter_selection_uses_exact_branch_identity_and_original_number_strings() {
	let items = chapters();
	let selection = LibGroupChapterListItem::resolve_chapter(&items, "12").unwrap();
	assert_eq!(selection, ("12.50", "2", Some(82)));
}

#[aidoku_test]
fn existing_null_branch_is_distinct_from_a_missing_branch() {
	let items = chapters();
	assert_eq!(
		LibGroupChapterListItem::resolve_chapter(&items, "10").unwrap(),
		("12.50", "1", None)
	);
	assert!(LibGroupChapterListItem::resolve_chapter(&items, "999").is_err());
	assert!(LibGroupChapterListItem::resolve_chapter(&items, "010").is_err());
}

#[aidoku_test]
fn duplicate_branch_identity_is_rejected_instead_of_selecting_another_translation() {
	let mut items = chapters();
	items[1].branches[0].id = 11;
	assert!(LibGroupChapterListItem::resolve_chapter(&items, "11").is_err());
}

#[aidoku_test]
fn image_pages_follow_numeric_slugs_even_when_the_api_array_is_shuffled() {
	let chapter: LibGroupImageChapter = serde_json::from_str(
		r#"{"pages":[
		{"slug":10,"url":"/last.webp"},{"slug":2,"url":"/middle.webp"},{"slug":1,"url":"/first.webp"}
	]}"#,
	)
	.unwrap();
	let urls: Vec<String> = chapter
		.ordered_pages()
		.into_iter()
		.map(|page| page.url)
		.collect();
	assert_eq!(urls, vec!["/first.webp", "/middle.webp", "/last.webp"]);
}

#[aidoku_test]
fn legacy_image_pages_without_slugs_keep_all_pages_in_array_order() {
	let chapter: LibGroupImageChapter = serde_json::from_str(
		r#"{"pages":[
		{"url":"/second.webp"},{"url":"/first.webp"}
	]}"#,
	)
	.unwrap();
	let urls: Vec<String> = chapter
		.ordered_pages()
		.into_iter()
		.map(|page| page.url)
		.collect();
	assert_eq!(urls, vec!["/second.webp", "/first.webp"]);
}

#[aidoku_test]
fn restricted_branches_remain_locked_without_attempting_to_unlock_them() {
	let item: LibGroupChapterListItem = serde_json::from_str(
		r#"{
		"volume":"1","number":"2","branches":[
			{"id":1,"restricted_view":{"is_open":false}},
			{"id":2,"restricted_view":{"is_open":true}}
		]
	}"#,
	)
	.unwrap();
	let chapters = item.into_chapters("https://mangalib.me", "1--test", &None);
	assert!(chapters[0].locked);
	assert!(!chapters[1].locked);
}

#[aidoku_test]
fn chapter_resolves_its_image_server_once_for_all_ordered_pages() {
	let chapter: LibGroupImageChapter =
		serde_json::from_str(r#"{"pages":[{"slug":2,"url":"/2.jpg"},{"slug":1,"url":"/1.jpg"}]}"#)
			.unwrap();
	let mut resolutions = 0;
	let pages = chapter
		.into_pages_with_server(|| {
			resolutions += 1;
			Ok("https://images.example".into())
		})
		.unwrap();
	assert_eq!(resolutions, 1);
	assert_eq!(pages.len(), 2);
	let PageContent::Url(first, _) = &pages[0].content else {
		panic!("image expected")
	};
	assert_eq!(first, "https://images.example/1.jpg");
}

#[aidoku_test]
fn missing_image_server_fails_the_chapter_instead_of_returning_relative_page_urls() {
	let chapter: LibGroupImageChapter =
		serde_json::from_str(r#"{"pages":[{"url":"/1.jpg"},{"url":"/2.jpg"}]}"#).unwrap();
	assert!(
		chapter
			.into_pages_with_server(|| {
				Err(aidoku::AidokuError::message(
					"CDN configuration unavailable",
				))
			})
			.is_err()
	);
}
