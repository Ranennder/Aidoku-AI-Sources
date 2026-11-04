use super::*;
use aidoku_test::aidoku_test;

fn record() -> Record {
	serde_json::from_str(r#"{"slug":"neutral-title","title":"Тест","type":"manga","coverUrl":"/cover.webp","authors":["N/A","Автор"],"statusTitle":"completed","chapters":[{"position":"12.5","createdAt":"2026-11-04T00:00:00Z"}]}"#).unwrap()
}

#[aidoku_test]
fn devalue_resolves_refs_without_treating_primitive_numbers_as_indices() {
	let decoded = decode(
		r#"{"nodes":[{"type":"data","data":[{"title":1,"count":2,"items":3},"Neutral",24,[1,2,-1]]}]}"#,
	)
	.unwrap();
	assert_eq!(decoded["title"], "Neutral");
	assert_eq!(decoded["count"], 24);
	assert!(decoded["items"][2].is_null());
	assert_eq!(decoded["items"][1], 24);
}

#[aidoku_test]
fn streamed_response_uses_last_data_node_and_ignores_related_lazy_chunk_lines() {
	let raw = "{\"nodes\":[{\"type\":\"data\",\"data\":[{\"layout\":1},\"old\"]},{\"type\":\"skip\"},{\"type\":\"data\",\"data\":[{\"title\":1},\"Actual\"]}]}\n{\"type\":\"chunk\",\"id\":1,\"data\":[\"ignored\"]}";
	let result = decode(raw).unwrap();
	assert_eq!(result["title"], "Actual");
	assert!(result.get("layout").is_none());
}

#[aidoku_test]
fn empty_cyclic_and_invalid_ref_payloads_are_errors() {
	for raw in [
		"",
		"{\"nodes\":[]}",
		"{\"nodes\":[{\"type\":\"data\",\"data\":[]}]}",
		"{\"nodes\":[{\"type\":\"data\",\"data\":[{\"cycle\":0}]}]}",
		"{\"nodes\":[{\"type\":\"data\",\"data\":[{\"invalid\":9}]}]}",
	] {
		assert!(decode(raw).is_err());
	}
}

#[aidoku_test]
fn offset_sort_and_query_are_encoded_without_duplicate_sort_keys() {
	let url = catalog_url(
		"Тест &limit=1",
		2,
		"likes",
		aidoku::alloc::vec![FilterValue::Sort {
			id: "sort".into(),
			index: 4,
			ascending: true
		}],
	);
	assert!(url.contains("offset=24&limit=24"));
	assert!(url.contains("sortKey=alphabetical"));
	assert!(url.contains("sortDescending=false"));
	assert_eq!(url.matches("sortKey=").count(), 1);
	assert!(url.contains("%26limit%3D1"));
}

#[aidoku_test]
fn repeated_type_filters_and_bounded_year_values_are_preserved() {
	let filters = aidoku::alloc::vec![
		FilterValue::MultiSelect {
			id: "types".into(),
			included: aidoku::alloc::vec!["manga".into(), "manhwa".into()],
			excluded: Vec::new()
		},
		FilterValue::Text {
			id: "releaseYearFrom".into(),
			value: "1000".into()
		}
	];
	let url = catalog_url("", 0, "updatedAt", filters);
	assert!(url.contains("types=manga&types=manhwa"));
	assert!(url.contains("releaseYearFrom=1990"));
	assert!(url.contains("offset=0"));
}

#[aidoku_test]
fn exact_keys_preserve_type_and_slug_but_reject_foreign_or_encoded_paths() {
	assert_eq!(
		key("https://wamanga.ru/manga/neutral-title?ref=1").unwrap(),
		"manga/neutral-title"
	);
	for value in [
		"https://example.com/manga/title",
		"https://wamanga.ru.example/manga/title",
		"manga/../title",
		"manga/%2Ftitle",
	] {
		assert!(key(value).is_err());
	}
}

#[aidoku_test]
fn details_preserve_non_placeholder_credits_and_decimal_chapter_identity() {
	let manga = record().into_manga(true, true, Manga::default()).unwrap();
	assert_eq!(manga.key, "manga/neutral-title");
	assert_eq!(
		manga.authors.unwrap(),
		aidoku::alloc::vec![String::from("Автор")]
	);
	let chapter = &manga.chapters.unwrap()[0];
	assert_eq!(chapter.key, "manga/neutral-title/12.5");
	assert_eq!(chapter.chapter_number, Some(12.5));
	assert!(chapter.url.as_deref().unwrap().ends_with("/glava-12.5"));
	assert!(chapter.date_uploaded.is_some());
}

#[aidoku_test]
fn a_chapters_only_update_does_not_replace_the_existing_title() {
	let manga = record()
		.into_manga(
			false,
			true,
			Manga {
				title: "Сохранено".into(),
				..Default::default()
			},
		)
		.unwrap();
	assert_eq!(manga.title, "Сохранено");
	assert_eq!(manga.chapters.unwrap().len(), 1);
}

#[aidoku_test]
fn files_use_numeric_fractional_order_not_lexicographic_order() {
	let data: Pages = serde_json::from_str(r#"{"chapter":{"files":[{"diskFile":"/10.webp","position":"10.000000000"},{"diskFile":"/2.gif?rev=1","position":"2.000000000"}]}}"#).unwrap();
	let pages = parse_pages(data, "https://wamanga.ru/manga/neutral-title/glava-1").unwrap();
	let PageContent::Url(url, context) = &pages[0].content else {
		panic!("image expected")
	};
	assert_eq!(url, "https://wamanga.ru/2.gif?rev=1");
	assert!(context.as_ref().unwrap()["referer"].ends_with("glava-1"));
}

#[aidoku_test]
fn bad_positions_and_empty_files_are_errors_not_empty_successes() {
	for value in [
		Value::String("../1".into()),
		Value::String("NaN".into()),
		Value::Null,
	] {
		assert!(position(&value).is_err());
	}
	assert!(
		parse_pages(
			Pages {
				chapter: Files { files: Vec::new() }
			},
			BASE
		)
		.is_err()
	);
	assert!(absolute("javascript:invalid").is_err());
}

#[aidoku_test]
fn only_full_twenty_four_entry_catalog_pages_advertise_a_next_page() {
	let empty = parse_catalog(Catalog {
		initial_mangas: Vec::new(),
	})
	.unwrap();
	assert!(!empty.has_next_page);
	let full = parse_catalog(Catalog {
		initial_mangas: (0..24).map(|_| record()).collect(),
	})
	.unwrap();
	assert!(full.has_next_page);
}
