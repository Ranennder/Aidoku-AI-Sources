use super::*;
use aidoku::imports::html::Html;
use aidoku_test::aidoku_test;
fn html(value: &str) -> Document {
	Html::parse_with_url(value, BASE).unwrap()
}
#[aidoku_test]
fn catalogue_preserves_title_paths_and_real_pagination() {
	let result=parse_catalog(&html("<div class='item-grid'><a class='fw-medium' href='/title/fixture'>Fixture</a><img class='item-grid-image' src='/cover.jpg'></div><ul class='pagination'><li class='page-item'><a href='?page=2'>Next</a></li></ul>")).unwrap();
	assert_eq!(result.entries[0].key, "/title/fixture");
	assert!(result.has_next_page);
	assert!(
		parse_catalog(&html("<div id='filter_genres'></div>"))
			.unwrap()
			.entries
			.is_empty()
	);
	assert!(parse_catalog(&html("<title>Challenge</title>")).is_err());
}
#[aidoku_test]
fn filter_segments_follow_the_sites_required_order() {
	let filters = aidoku::alloc::vec![
		FilterValue::MultiSelect {
			id: "tags".into(),
			included: aidoku::alloc::vec!["z".into(), "a".into()],
			excluded: Vec::new()
		},
		FilterValue::MultiSelect {
			id: "genres".into(),
			included: aidoku::alloc::vec!["2".into()],
			excluded: aidoku::alloc::vec!["5".into()]
		},
		FilterValue::Range {
			id: "year".into(),
			from: Some(2020.0),
			to: None
		}
	];
	assert_eq!(
		catalog_path(None, 2, "rating", filters),
		"/explore/genres-is-2-nor-5/tags-is-a-or-z/year-from-2020/sort-is-rating?page=2"
	);
	assert!(catalog_path(Some("A & B"), 1, "rating", Vec::new()).contains("query=A%20%26%20B"));
}
#[aidoku_test]
fn details_read_named_attributes_without_host_specific_text_selectors() {
	let manga=parse_details(&html("<div class='detail-panel'><h1>Fixture</h1></div><div><span class='attr-name'>Автор</span><span class='attr-value'><a>Fixture Author</a></span></div><div><span class='attr-name'>Томов</span><span class='attr-value'>2, завершен</span></div><img class='cover-detail' src='/cover.jpg'>"),Manga{key:"/title/fixture".into(),..Default::default()}).unwrap();
	assert_eq!(manga.title, "Fixture");
	assert_eq!(manga.authors.as_ref().unwrap()[0], "Fixture Author");
	assert_eq!(manga.status, MangaStatus::Completed);
}
#[aidoku_test]
fn chapters_keep_decimal_numbers_dates_and_reader_keys() {
	let chapters = parse_chapters(&html(
		"<div class='detail-items'><div class='detail-item'><div class='align-items-center'><a href='/read/fixture-12'>Глава 12.5</a></div><div class='text-muted'>30.10.2026</div></div></div>",
	));
	assert_eq!(chapters[0].key, "/read/fixture-12");
	assert_eq!(chapters[0].chapter_number, Some(12.5));
	assert_eq!(chapters[0].date_uploaded, Some(1793318400));
}
#[aidoku_test]
fn page_urls_keep_https_and_reader_order_and_reject_empty_access_pages() {
	let pages=parse_pages(&html("<img class='reader-viewer-img' data-src='//cdn.example/first.webp'><img class='reader-viewer-img' data-src='/second.jpg'>"),"https://mangahub.ru/read/fixture").unwrap();
	assert_eq!(pages.len(), 2);
	let PageContent::Url(url, _) = &pages[0].content else {
		panic!("URL expected")
	};
	assert_eq!(url, "https://cdn.example/first.webp");
	assert!(parse_pages(&html("<h1>Login required</h1>"), BASE).is_err());
	assert!(site_path("https://mangahub.ru.example.com/title/fixture").is_err());
}
