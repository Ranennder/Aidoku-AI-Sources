use super::*;
use aidoku::{PageContent, imports::html::Html};
use aidoku_test::aidoku_test;
fn html(value: &str) -> Document {
	Html::parse_with_url(value, BASE).unwrap()
}

#[aidoku_test]
fn catalogue_keeps_paths_covers_and_empty_grid() {
	let result=parse_catalog(&html("<div id='manga-grid'><a href='/manga/fixture/'><h3>Fixture</h3><img data-src='/cover.webp'></a><a href='/manga/fixture/'><h3>duplicate</h3></a></div>")).unwrap();
	assert_eq!(result.entries.len(), 1);
	assert_eq!(result.entries[0].key, "/manga/fixture/");
	assert_eq!(
		result.entries[0].cover.as_deref(),
		Some("https://manga-shi.org/cover.webp")
	);
	assert!(
		parse_catalog(&html("<div id='manga-grid'></div>"))
			.unwrap()
			.entries
			.is_empty()
	);
	assert!(parse_catalog(&html("<title>Challenge</title>")).is_err());
}
#[aidoku_test]
fn search_encodes_query_and_paginates_without_losing_filters() {
	let path = catalog_path(
		Some("A & B"),
		2,
		"popular",
		aidoku::alloc::vec![FilterValue::Select {
			id: "status".into(),
			value: "COMPLETED".into()
		}],
	);
	assert!(path.contains("q=A%20%26%20B"));
	assert!(path.contains("page=2"));
	assert!(path.contains("status=COMPLETED"));
}
#[aidoku_test]
fn chapter_identifiers_keep_decimal_numbers_and_dates() {
	let chapters = parse_chapters(
		&html(
			"<div id='chapters-list'><a href='/manga/fixture/glava-12,5/'><span class='chapter-title'><span>Глава 12.5</span></span><span><span>30.10.2026</span></span></a></div>",
		),
		2000000000,
	);
	assert_eq!(chapters.len(), 1);
	assert_eq!(chapters[0].chapter_number, Some(12.5));
	assert_eq!(chapters[0].date_uploaded, Some(1793318400));
	assert_eq!(chapter_date("2 дня назад", 2000000000), Some(1999827200));
}
#[aidoku_test]
fn reader_prefers_real_lazy_pages_and_keeps_order() {
	let pages=parse_pages(&html("<img class='reader-image' src='/first.webp'><img class='reader-image' src='data:image/gif;base64,placeholder' data-src='/second.webp'>"),"https://manga-shi.org/manga/fixture/glava-1/").unwrap();
	assert_eq!(pages.len(), 2);
	let PageContent::Url(url, _) = &pages[1].content else {
		panic!("URL expected")
	};
	assert_eq!(url, "https://manga-shi.org/second.webp");
	assert!(parse_pages(&html("<h1>Login</h1>"), BASE).is_err());
}
#[aidoku_test]
fn details_keep_completion_status_and_foreign_links_are_rejected() {
	let manga=parse_details(&html("<h1>Fixture</h1><span class='tracking-widest'>Завершён</span><span class='tracking-widest'>Манхва</span><meta property='og:image' content='/cover.jpg'><div class='leading-relaxed' x-ref='descDesktop'>Description</div>"),Manga{key:"/manga/fixture/".into(),..Default::default()}).unwrap();
	assert_eq!(manga.status, MangaStatus::Completed);
	assert_eq!(manga.viewer, Viewer::Webtoon);
	assert_eq!(manga.description.as_deref(), Some("Description"));
	assert!(site_path("https://example.com/manga/fixture/").is_err());
}
