use super::*;
use aidoku::{PageContent, alloc::vec, imports::html::Html};
use aidoku_test::aidoku_test;

fn fixture(value: &str) -> Document {
	Html::parse_with_url(value, DEFAULT_URL).unwrap()
}

#[aidoku_test]
fn catalog_preserves_path_identity_and_upgrades_thumbnail_without_reader_downloads() {
	let doc = fixture(
		r#"<div class='content_row' title='Synthetic title'><h2><a href='/manga/123-demo.html'>Demo</a></h2><img src='/manganew_thumbs/cover.jpg'></div><a href='/mostfavorites?offset=20'>Вперед</a>"#,
	);
	let result = parser::catalog(&doc, DEFAULT_URL, false).unwrap();
	assert_eq!(result.entries[0].key, "/manga/123-demo.html");
	assert_eq!(
		result.entries[0].cover.as_deref(),
		Some("https://xxl.hentaichan.live/showfull_retina/manga/cover.jpg")
	);
	assert!(result.has_next_page);
}

#[aidoku_test]
fn search_encodes_query_and_matches_twenty_entry_catalog_pagination() {
	let path = parser::search_path(Some("текст &+"), 2, vec![]);
	assert!(path.contains("%26%2B"));
	assert!(path.ends_with("search_start=2"));
	assert_eq!(
		parser::search_path(None, 3, vec![]),
		"/mostfavorites&sort=manga?offset=40"
	);
}

#[aidoku_test]
fn related_pagination_accumulates_all_pages_and_deduplicates_only_exact_paths() {
	let first = fixture(
		r#"<div class='related'><h2><a title='Demo Глава 2.5' href='/manga/2-demo.html'>Demo</a></h2></div><div id='pagination_related'><a href='/related/demo.html?offset=20'>Вперед</a></div>"#,
	);
	let second = fixture(
		r#"<div class='related'><h2><a title='Demo Глава 2.5' href='/manga/2-demo.html'>Duplicate</a></h2></div><div class='related'><h2><a title='Demo Глава 1' href='/manga/1-demo.html'>Demo</a></h2></div>"#,
	);
	assert_eq!(
		parser::next_related_page(&first).as_deref(),
		Some("https://xxl.hentaichan.live/related/demo.html?offset=20")
	);
	assert!(parser::next_related_page(&second).is_none());
	let mut chapters = parser::chapters(&first, DEFAULT_URL).unwrap();
	parser::append_unique(
		&mut chapters,
		parser::chapters(&second, DEFAULT_URL).unwrap(),
	);
	assert_eq!(chapters.len(), 2);
	assert_eq!(chapters[0].chapter_number, Some(2.5));
	assert_eq!(chapters[1].key, "/manga/1-demo.html");
}

#[aidoku_test]
fn external_gallery_and_single_related_variants_are_kept_as_one_chapter() {
	let doc = fixture(
		"<img id='cover' src='/manganew_thumbs_blur/cover.jpg'><a class='title_top_a'>Synthetic gallery</a>",
	);
	assert!(parser::external_gallery(&doc));
	assert_eq!(
		parser::single_chapter(&doc, "/manga/one.html", DEFAULT_URL).key,
		"/manga/one.html"
	);
	assert!(parser::has_single_related(&fixture(
		"<div id='right'>Материал похожий на Demo</div>"
	)));
}

#[aidoku_test]
fn fullimg_json_keeps_commas_escaped_slashes_animation_and_array_order() {
	let html = r#"<script>var images = {"fullimg": ["https:\/\/cdn.example\/second.jpg?q=a,b]c", "https://cdn.example/first.gif",]};</script>"#;
	let pages = parser::pages(html, DEFAULT_URL, "/manga/one.html").unwrap();
	assert_eq!(pages.len(), 2);
	let PageContent::Url(url, Some(context)) = &pages[0].content else {
		panic!("URL expected")
	};
	assert_eq!(url, "https://cdn.example/second.jpg?q=a,b]c");
	assert_eq!(
		context.get("referer").map(String::as_str),
		Some("https://xxl.hentaichan.live/manga/one.html")
	);
	let PageContent::Url(url, _) = &pages[1].content else {
		panic!("URL expected")
	};
	assert_eq!(url, "https://cdn.example/first.gif");
}

#[aidoku_test]
fn missing_empty_or_partial_images_never_become_a_successful_truncated_chapter() {
	for html in [
		"<h1>Login required</h1>",
		r#"{"fullimg":[]}"#,
		r#"{"fullimg":["https://cdn.example/1.jpg",null]}"#,
		r#"{"fullimg":["https://cdn.example/unfinished.jpg""#,
	] {
		assert!(parser::pages(html, DEFAULT_URL, "/manga/one.html").is_err());
	}
}

#[aidoku_test]
fn details_preserve_title_description_and_finished_status() {
	let doc = fixture(
		"<title>Demo » HenChan</title><img id='cover' src='/manganew_thumbs/cover.jpg'><div id='description'>Synthetic description</div><div id='info_wrap'><div>Загружено: перевод завершен</div></div>",
	);
	let manga = parser::details(
		&doc,
		Manga {
			key: "/manga/one.html".into(),
			..Default::default()
		},
		DEFAULT_URL,
	)
	.unwrap();
	assert_eq!(manga.title, "Demo");
	assert_eq!(manga.description.as_deref(), Some("Synthetic description"));
	assert_eq!(manga.status, aidoku::MangaStatus::Completed);
}

#[aidoku_test]
fn cookies_are_sanitized_and_canonical_keys_stay_on_the_selected_origin() {
	let mut cookies = HashMap::new();
	cookies.insert("dle_user_id".into(), "fixture-account".into());
	cookies.insert("dle_password".into(), "fixture-token".into());
	cookies.insert("bad".into(), "fixture; other=value".into());
	assert_eq!(
		parser::cookie_header(cookies).as_deref(),
		Some("dle_password=fixture-token; dle_user_id=fixture-account")
	);
	for url in [
		"https://other.example/manga/one.html",
		"//other.example/one.html",
		"/../one.html",
	] {
		assert!(parser::canonical_path(url, DEFAULT_URL).is_err());
	}
}

#[aidoku_test]
fn http_failures_remain_visible() {
	for code in [401, 403, 404, 429, 503] {
		assert!(parser::check_status(code).is_err());
	}
	assert!(parser::check_status(200).is_ok());
	assert!(
		parser::catalog(
			&fixture("<h1>Browser verification required</h1>"),
			DEFAULT_URL,
			false
		)
		.is_err()
	);
}

#[aidoku_test]
fn native_guest_cookie_storage_does_not_run_a_login_success_callback_and_logout_clears_origin() {
	let source = HenChan;
	defaults_set(
		"login.keys",
		DefaultValue::StringArray(vec!["PHPSESSID".into()]),
	);
	defaults_set(
		"login.values",
		DefaultValue::StringArray(vec!["fixture-guest".into()]),
	);
	source.handle_notification("login.changed".into());
	assert_eq!(
		defaults_get::<String>("loginOrigin").as_deref(),
		Some(DEFAULT_URL)
	);
	// The source exposes NotificationHandler, never WebLoginHandler. Keeping a
	// guest session snapshot cannot tell the host to close the login window.
	defaults_set("login.keys", DefaultValue::StringArray(Vec::new()));
	defaults_set("login.values", DefaultValue::StringArray(Vec::new()));
	source.handle_notification("login.changed".into());
	assert!(defaults_get::<String>("loginOrigin").is_none());
	defaults_set("login.keys", DefaultValue::Null);
	defaults_set("login.values", DefaultValue::Null);
}
