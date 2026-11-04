use super::*;
use aidoku::{PageContent, alloc::vec, imports::html::Html};
use aidoku_test::aidoku_test;

fn fixture(value: &str) -> Document {
	Html::parse_with_url(value, DEFAULT_URL).unwrap()
}

#[aidoku_test]
fn catalog_keeps_site_keys_and_next_page_without_title_based_matching() {
	let document = fixture(
		r#"<table class='news_pic2'><tr><td><a href='/123--demo.html'><img src='/cover.jpg'><h2>Demo / Translation №2</h2></a></td></tr></table><a class='small' href='/all_manga?views&amp;rowstart=30'>&gt;</a>"#,
	);
	let result = parser::catalog(&document, DEFAULT_URL).unwrap();
	assert_eq!(result.entries.len(), 1);
	assert_eq!(result.entries[0].key, "/123--demo.html");
	assert_eq!(result.entries[0].title, "Demo");
	assert!(result.has_next_page);
}

#[aidoku_test]
fn search_encodes_unicode_and_reserved_characters_and_uses_thirty_entry_offsets() {
	let path = parser::search_path(Some("  текст &+  "), 2, vec![]);
	assert!(path.contains("%26%2B"));
	assert!(path.ends_with("rowstart=30"));
	assert_eq!(
		parser::catalog_path("date", 3),
		"/all_manga?date&rowstart=60"
	);
	assert_eq!(
		parser::catalog_path("views", -1),
		"/all_manga?views&rowstart=0"
	);
}

#[aidoku_test]
fn details_keep_existing_identity_and_extract_metadata() {
	let document = fixture(
		r#"<h1>Demo / Subtitle</h1><meta property='og:image' content='/cover.jpg'><div class='description'>Synthetic description</div><a href='/mangaka/example'>Author</a><div class='tag-links'><a>Fantasy</a></div>"#,
	);
	let manga = parser::details(
		&document,
		Manga {
			key: "/123--demo.html".into(),
			..Default::default()
		},
		DEFAULT_URL,
	)
	.unwrap();
	assert_eq!(manga.key, "/123--demo.html");
	assert_eq!(manga.description.as_deref(), Some("Synthetic description"));
	assert_eq!(
		manga.cover.as_deref(),
		Some("https://nude-moon.org/cover.jpg")
	);
}

#[aidoku_test]
fn all_chapter_pages_merge_without_duplicate_keys_or_decimal_loss() {
	let first = fixture(
		r#"<table class='news_pic2'><tr><td><a href='/ch-2.html'><h2>Demo №2-5 extra</h2></a><span class='small2'>3 ноября 2026</span></td></tr></table>"#,
	);
	let second = fixture(
		r#"<table class='news_pic2'><tr><td><a href='/ch-2.html'><h2>Duplicate №2-5</h2></a></td></tr></table><table class='news_pic2'><tr><td><a href='/ch-1.html'><h2>Demo №1</h2></a></td></tr></table>"#,
	);
	let mut chapters = parser::chapters(&first, DEFAULT_URL).unwrap();
	parser::append_unique(
		&mut chapters,
		parser::chapters(&second, DEFAULT_URL).unwrap(),
	);
	assert_eq!(chapters.len(), 2);
	assert_eq!(chapters[0].key, "/ch-2.html");
	assert_eq!(chapters[0].chapter_number, Some(2.5));
	assert_eq!(chapters[0].date_uploaded, Some(1_793_664_000));
	assert_eq!(chapters[1].key, "/ch-1.html");
}

#[aidoku_test]
fn chapter_list_button_and_single_chapter_are_distinct() {
	let document = fixture(
		"<table><tr><td class='button'><a href='/chapters/demo'>Все главы</a></td></tr></table>",
	);
	assert_eq!(
		parser::all_chapters_url(&document).as_deref(),
		Some("https://nude-moon.org/chapters/demo")
	);
	let single =
		parser::single_chapter(&fixture("<h1>Demo</h1>"), "/one.html", DEFAULT_URL).unwrap();
	assert_eq!(single.key, "/one.html");
	assert_eq!(single.chapter_number, Some(0.0));
}

#[aidoku_test]
fn reader_keeps_lazy_image_order_and_referer_without_fetching_images() {
	let document = fixture(
		r#"<img title='1' loading='lazy' data-src='https://cdn.example/second.jpg?v=2'><img title='2' loading='lazy' data-src='/first.gif'>"#,
	);
	let pages = parser::pages(&document, DEFAULT_URL, "/one.html").unwrap();
	assert_eq!(pages.len(), 2);
	let PageContent::Url(url, Some(context)) = &pages[0].content else {
		panic!("URL with referer expected")
	};
	assert_eq!(url, "https://cdn.example/second.jpg?v=2");
	assert_eq!(
		context.get("referer").map(String::as_str),
		Some("https://nude-moon.org/one.html")
	);
	assert!(
		parser::pages(
			&fixture("<div>Please sign in</div>"),
			DEFAULT_URL,
			"/one.html"
		)
		.is_err()
	);
}

#[aidoku_test]
fn required_site_preferences_do_not_override_or_expose_account_credentials() {
	let mut cookies = HashMap::new();
	cookies.insert("fusion_user".into(), "fixture-session".into());
	cookies.insert("NMfYa".into(), "0".into());
	cookies.insert("injected".into(), "bad\r\nHeader".into());
	assert_eq!(
		parser::cookie_header(cookies),
		"NMfYa=1; nm_mobile=1; fusion_user=fixture-session"
	);
}

#[aidoku_test]
fn pagination_and_keys_cannot_leave_the_configured_site() {
	assert_eq!(
		parser::canonical_path("https://nude-moon.org/[demo].html", DEFAULT_URL).unwrap(),
		"/[demo].html"
	);
	for path in [
		"https://other.example/one.html",
		"//other.example/one.html",
		"/../one.html",
		"https://nude-moon.org.example/one.html",
	] {
		assert!(parser::canonical_path(path, DEFAULT_URL).is_err());
	}
	assert_eq!(
		parser::base_url(" https://mirror.example/ "),
		"https://mirror.example"
	);
	assert_eq!(parser::base_url("https://user@example.com"), DEFAULT_URL);
}

#[aidoku_test]
fn http_failures_are_not_silent_empty_results() {
	for code in [401, 403, 404, 429, 503] {
		assert!(parser::check_status(code).is_err());
	}
	assert!(parser::check_status(200).is_ok());
	assert!(
		parser::catalog(
			&fixture("<h1>Browser verification required</h1>"),
			DEFAULT_URL
		)
		.is_err()
	);
}

#[aidoku_test]
fn site_tags_are_loaded_as_exact_distinct_filter_ids() {
	let document = fixture(
		"<input name='tag[]' value='Fantasy'><input name='tag[]' value='Fantasy'><input name='tag[]' value='Science fiction'><input name='unrelated' value='ignored'>",
	);
	assert_eq!(parser::tags(&document), vec!["Fantasy", "Science fiction"]);
	let path = parser::search_path(
		None,
		2,
		vec![FilterValue::MultiSelect {
			id: "tags".into(),
			included: vec!["Science fiction".into()],
			excluded: vec![],
		}],
	);
	assert_eq!(path, "/tags/Science_fiction&views&rowstart=30");
}

#[aidoku_test]
fn successful_service_html_cannot_replace_the_library_with_a_fabricated_single_chapter() {
	let source = NudeMoon;
	let document = fixture("<div>Browser verification or service message</div>");
	let manga = Manga {
		key: "/existing.html".into(),
		title: "Existing title".into(),
		..Default::default()
	};
	assert!(source.chapter_list(&document, &manga).is_err());
}
