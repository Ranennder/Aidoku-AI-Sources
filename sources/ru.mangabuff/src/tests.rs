use super::*;
use aidoku::{MangaStatus, PageContent, Viewer, imports::html::Html};
use aidoku_test::aidoku_test;

fn fixture(value: &str) -> Document {
	Html::parse_with_url(value, BASE_URL).expect("valid synthetic HTML")
}

#[aidoku_test]
fn catalog_handles_link_cards_and_nested_links_without_inventing_ids() {
	let document = fixture(include_str!("../tests/fixtures/catalog.html"));
	let result = parse_catalog(&document).unwrap();
	assert_eq!(result.entries.len(), 2);
	assert_eq!(result.entries[0].key, "/manga/fixture-one");
	assert_eq!(result.entries[0].title, "Первый & второй");
	assert_eq!(
		result.entries[0].cover.as_deref(),
		Some("https://mangabuff.ru/img/one.jpg?revision=2")
	);
	assert_eq!(result.entries[1].key, "/manga/fixture-two");
	assert_eq!(
		result.entries[1].cover.as_deref(),
		Some("https://mangabuff.ru/img/manga/posters/fixture-two.jpg")
	);
	assert!(result.has_next_page);
}

#[aidoku_test]
fn no_results_and_a_site_challenge_are_different_results() {
	let empty = parse_catalog(&fixture(
		"<div class='cards'></div><div class='pagination'><span>Вперёд</span></div>",
	))
	.unwrap();
	assert!(empty.entries.is_empty());
	assert!(!empty.has_next_page);
	assert!(parse_catalog(&fixture("<html><title>Site challenge</title></html>")).is_err());
}

#[aidoku_test]
fn search_uses_unicode_length_and_encodes_query_parameters() {
	assert!(parser::search_path(Some("ёж"), 1).is_err());
	assert_eq!(
		parser::search_path(Some("  a&+  "), 2).unwrap(),
		"/search?type=manga&q=a%26%2B&page=2"
	);
	assert_eq!(
		parser::search_path(Some("   "), 0).unwrap(),
		"/manga?sort=real_views&page=1"
	);
	assert_eq!(
		parser::catalog_path("updated_at", 3),
		"/manga?sort=updated_at&page=3"
	);
}

#[aidoku_test]
fn only_exact_site_paths_can_be_used_as_manga_or_chapter_keys() {
	assert_eq!(
		canonical_path(
			"https://mangabuff.ru/manga/fixture-one/?tracking=yes#top",
			false
		)
		.unwrap(),
		"/manga/fixture-one"
	);
	assert_eq!(
		canonical_path("/manga/fixture-one/2/12.5", true).unwrap(),
		"/manga/fixture-one/2/12.5"
	);
	for url in [
		"https://example.com/manga/fixture-one",
		"https://mangabuff.ru.example.com/manga/fixture-one",
		"/manga/../",
		"/manga/%2e%2e",
		"/manga/fixture-one/2/12.5/extra",
	] {
		assert!(canonical_path(url, false).is_err());
		assert!(canonical_path(url, true).is_err());
	}
}

#[aidoku_test]
fn details_preserve_site_cover_status_and_deduplicated_tags() {
	let document = fixture(include_str!("../tests/fixtures/details.html"));
	let manga = parse_details(
		&document,
		Manga {
			key: "/manga/fixture-one".into(),
			..Default::default()
		},
	)
	.unwrap();
	assert_eq!(manga.title, "Пример произведения");
	assert_eq!(
		manga.description.as_deref(),
		Some("Первая строка.\n\nВторая строка.")
	);
	assert_eq!(
		manga.cover.as_deref(),
		Some("https://mangabuff.ru/img/cover.jpg?revision=7")
	);
	assert_eq!(manga.status, MangaStatus::Completed);
	assert_eq!(manga.viewer, Viewer::Webtoon);
	let tags = manga.tags.unwrap();
	assert_eq!(
		tags.iter()
			.filter(|tag| tag.as_str() == "Приключения")
			.count(),
		1
	);
	assert!(!tags.iter().any(|tag| tag == "Завершён"));
	let mixed = parse_details(
		&fixture("<h1>Пример</h1><div class='manga__description'><p>Один<br>Два &amp; <em>три</em></p><p>Четыре</p></div>"),
		Manga { key: "/manga/fixture-one".into(), ..Default::default() },
	).unwrap();
	assert_eq!(
		mixed.description.as_deref(),
		Some("Один\nДва & три\n\nЧетыре")
	);
}

#[aidoku_test]
fn mobile_details_and_unknown_status_do_not_drop_existing_metadata() {
	let document = fixture(
		"<div class='manga-mobile__name'>Мобильное название</div><img class='manga-mobile__image' src='//mangabuff.ru/mobile.jpg'><div class='manga-mobile__info'><a>Манга</a><a>Неизвестно</a></div>",
	);
	let manga = parse_details(
		&document,
		Manga {
			key: "/manga/fixture-one".into(),
			status: MangaStatus::Hiatus,
			..Default::default()
		},
	)
	.unwrap();
	assert_eq!(manga.status, MangaStatus::Hiatus);
	assert_eq!(manga.viewer, Viewer::RightToLeft);
	assert_eq!(
		manga.cover.as_deref(),
		Some("https://mangabuff.ru/mobile.jpg")
	);
}

#[aidoku_test]
fn chapter_urls_decimals_dates_and_order_survive_the_100_entry_boundary() {
	let document = fixture(include_str!("../tests/fixtures/chapters.html"));
	let mut chapters = parse_chapters(&document).unwrap();
	assert_eq!(chapters.len(), 2);
	assert_eq!(chapters[0].chapter_number, Some(125.0));
	assert_eq!(chapters[0].volume_number, Some(1.0));
	assert_eq!(chapters[1].chapter_number, Some(124.5));
	assert_eq!(chapters[0].date_uploaded, Some(1_793_318_400));
	assert_eq!(chapters[1].date_uploaded, Some(1_792_713_600));
	assert_eq!(chapters[1].title.as_deref(), Some("Часть 2"));
	let remaining = parse_chapters(&fixture("<a class='chapters__item' href='/manga/fixture-one/1/124.5'></a><a class='chapters__item' href='/manga/fixture-one/1/124'><span class='chapters__value'>Глава 124</span></a><a class='chapters__item' href='/manga/fixture-one/2/124'><span class='chapters__value'>Глава 124</span></a>")).unwrap();
	parser::append_unique_chapters(&mut chapters, remaining);
	assert_eq!(chapters.len(), 4);
	assert_eq!(chapters[2].key, "/manga/fixture-one/1/124");
	assert_eq!(chapters[3].key, "/manga/fixture-one/2/124");
	assert_eq!(
		chapters[0].url.as_deref(),
		Some("https://mangabuff.ru/manga/fixture-one/1/125")
	);
}

#[aidoku_test]
fn upload_dates_are_utc_midnight_and_invalid_dates_stay_unknown() {
	assert_eq!(parser::parse_upload_date(" 29.02.2000 "), Some(951_782_400));
	assert_eq!(parser::parse_upload_date("01.01.1970"), Some(0));
	assert_eq!(parser::parse_upload_date("not-a-date"), None);
	assert_eq!(parser::parse_upload_date(""), None);
}

#[aidoku_test]
fn reader_keeps_lazy_image_order_query_strings_and_animated_urls() {
	let referer = "https://mangabuff.ru/manga/fixture-one/1/125";
	let pages = parse_pages(
		&fixture(include_str!("../tests/fixtures/reader.html")),
		referer,
	)
	.unwrap();
	assert_eq!(pages.len(), 3);
	for (index, expected) in [
		"https://c3.mangabuff.ru/chapters/first.jpeg?revision=9",
		"https://mangabuff.ru/chapters/second.gif",
		"https://c3.mangabuff.ru/chapters/third.webp",
	]
	.into_iter()
	.enumerate()
	{
		let PageContent::Url(url, Some(context)) = &pages[index].content else {
			panic!("URL page with context expected")
		};
		assert_eq!(url, expected);
		assert_eq!(context.get("referer").map(String::as_str), Some(referer));
	}
}

#[aidoku_test]
fn inaccessible_or_empty_chapters_report_errors_instead_of_zero_pages() {
	assert!(parse_pages(&fixture("<h1>Доступ ограничен 18+</h1>"), BASE_URL).is_err());
	assert!(parse_pages(&fixture("<div class='reader__pages'></div>"), BASE_URL).is_err());
	for status in [401, 403, 404, 419, 429, 500] {
		let aidoku::AidokuError::Message(message) = parser::check_status(status).unwrap_err()
		else {
			panic!("user-facing HTTP error expected")
		};
		assert!(message.starts_with(&format!("HTTP {status}: ")));
		assert!(message.contains("MangaBuff"));
	}
	let aidoku::AidokuError::Message(message) = parser::check_status(401).unwrap_err() else {
		panic!("user-facing error expected")
	};
	assert!(message.contains("Войти в MangaBuff"));
	for status in 200..=299 {
		assert!(parser::check_status(status).is_ok());
	}
}

#[aidoku_test]
fn deep_links_preserve_the_exact_volume_and_decimal_chapter_path() {
	let source = MangaBuff {
		csrf_token: RefCell::new(None),
		login_cookie: RefCell::new(None),
	};
	assert_eq!(
		source
			.handle_deep_link("https://mangabuff.ru/manga/fixture-one/2/12.5?ref=test".into())
			.unwrap(),
		Some(DeepLinkResult::Chapter {
			manga_key: "/manga/fixture-one".into(),
			key: "/manga/fixture-one/2/12.5".into()
		})
	);
	assert_eq!(
		source
			.handle_deep_link("https://example.com/manga/fixture-one".into())
			.unwrap(),
		None
	);
}

#[aidoku_test]
fn saved_web_login_credentials_only_target_the_exact_https_site() {
	let mut cookies = aidoku::HashMap::new();
	cookies.insert("mangabuff_session".into(), "fixture-session".into());
	cookies.insert("remember_web_fixture".into(), "fixture-remember".into());
	cookies.insert("third_party_tracker".into(), "unrelated".into());
	cookies.insert("__ddg10_".into(), "stale-webview-guard".into());
	let header = auth::cookie_header(cookies);
	assert_eq!(
		header.as_deref(),
		Some("mangabuff_session=fixture-session; remember_web_fixture=fixture-remember")
	);
	assert_eq!(
		auth::cookie_for_url("https://mangabuff.ru/manga/example", header.clone()),
		header
	);
	for url in [
		"http://mangabuff.ru/manga/example",
		"https://c3.mangabuff.ru/page.jpg",
		"https://mangabuff.ru.example.com/manga/example",
		"https://mangabuff.ru@example.com/page",
		"https://example.com/page",
	] {
		assert_eq!(auth::cookie_for_url(url, header.clone()), None);
	}
}

#[aidoku_test]
fn saved_login_changes_and_logout_invalidate_csrf_without_freezing_headers() {
	use aidoku::imports::defaults::{DefaultValue, defaults_set};
	let source = MangaBuff {
		csrf_token: RefCell::new(Some("fixture-anonymous-csrf".into())),
		login_cookie: RefCell::new(None),
	};
	defaults_set(
		"mangabuffLogin.keys",
		DefaultValue::StringArray(aidoku::alloc::vec![
			"mangabuff_session".into(),
			"remember_web_fixture".into()
		]),
	);
	defaults_set(
		"mangabuffLogin.values",
		DefaultValue::StringArray(aidoku::alloc::vec![
			"fixture-session-a".into(),
			"fixture-remember".into()
		]),
	);
	assert!(
		source
			.synchronize_login()
			.unwrap()
			.contains("fixture-session-a")
	);
	assert!(source.csrf_token.borrow().is_none());
	*source.csrf_token.borrow_mut() = Some("fixture-authenticated-csrf".into());
	// The WebView dictionary can publish the same cookies in a different order.
	defaults_set(
		"mangabuffLogin.keys",
		DefaultValue::StringArray(aidoku::alloc::vec![
			"remember_web_fixture".into(),
			"mangabuff_session".into()
		]),
	);
	defaults_set(
		"mangabuffLogin.values",
		DefaultValue::StringArray(aidoku::alloc::vec![
			"fixture-remember".into(),
			"fixture-session-a".into()
		]),
	);
	source.synchronize_login();
	assert_eq!(
		source.csrf_token.borrow().as_deref(),
		Some("fixture-authenticated-csrf")
	);
	defaults_set(
		"mangabuffLogin.values",
		DefaultValue::StringArray(aidoku::alloc::vec![
			"fixture-remember".into(),
			"fixture-session-b".into()
		]),
	);
	assert!(
		source
			.synchronize_login()
			.unwrap()
			.contains("fixture-session-b")
	);
	assert!(source.csrf_token.borrow().is_none());
	*source.csrf_token.borrow_mut() = Some("fixture-refreshed-csrf".into());
	// Missing/empty arrays are how the source observes native logout.
	defaults_set("mangabuffLogin.keys", DefaultValue::StringArray(Vec::new()));
	defaults_set(
		"mangabuffLogin.values",
		DefaultValue::StringArray(Vec::new()),
	);
	assert_eq!(source.synchronize_login(), None);
	assert!(source.csrf_token.borrow().is_none());
}

#[aidoku_test]
fn malformed_saved_cookie_values_cannot_inject_an_http_header() {
	let mut cookies = aidoku::HashMap::new();
	cookies.insert(
		"mangabuff_session".into(),
		"fixture; another=unexpected".into(),
	);
	cookies.insert(
		"remember_web_fixture".into(),
		"fixture\r\nX-Injected: value".into(),
	);
	cookies.insert("remember_web_invalid\n".into(), "fixture".into());
	assert_eq!(auth::cookie_header(cookies), None);
}

#[cfg(feature = "live-tests")]
mod live {
	use super::*;

	#[aidoku_test]
	fn live_catalog_pagination_search_and_details() {
		let source = MangaBuff::new();
		let popular = source
			.get_manga_list(
				Listing {
					id: "popular".into(),
					..Default::default()
				},
				1,
			)
			.unwrap();
		let next = source
			.get_manga_list(
				Listing {
					id: "popular".into(),
					..Default::default()
				},
				2,
			)
			.unwrap();
		assert!(!popular.entries.is_empty() && !next.entries.is_empty() && popular.has_next_page);
		assert_ne!(popular.entries[0].key, next.entries[0].key);
		let latest = source
			.get_manga_list(
				Listing {
					id: "latest".into(),
					..Default::default()
				},
				1,
			)
			.unwrap();
		assert!(!latest.entries.is_empty());
		let search = source
			.get_search_manga_list(Some("Герой".into()), 1, Vec::new())
			.unwrap();
		assert!(!search.entries.is_empty());
		let manga = source
			.get_manga_update(popular.entries[0].clone(), true, false)
			.unwrap();
		assert!(!manga.title.is_empty() && manga.cover.is_some());
	}

	#[aidoku_test]
	fn live_public_chapter_and_actual_first_image() {
		let source = MangaBuff::new();
		let pages = source
			.get_page_list(
				Manga::default(),
				Chapter {
					key: "/manga/vozvrashchenie-gonchei-zheleznoi-krovi/1/88".into(),
					..Default::default()
				},
			)
			.unwrap();
		assert!(!pages.is_empty());
		let PageContent::Url(url, context) = &pages[0].content else {
			panic!("URL page expected")
		};
		let response = source
			.get_image_request(url.clone(), context.clone())
			.unwrap()
			.send()
			.unwrap();
		parser::check_status(response.status_code()).unwrap();
		let bytes = response.get_data().unwrap();
		assert!(bytes.len() > 1000, "expected actual image bytes");
	}
}
