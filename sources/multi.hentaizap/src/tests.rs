use super::*;
use aidoku::imports::html::Html;
use aidoku_test::aidoku_test;

fn document(html: &str) -> Document {
	Html::parse(html).unwrap()
}
fn card(id: u32, lang: &str) -> String {
	format!(
		"<div class='hz-gallery-card'><a class='hz-gallery-card__cover' href='/gallery/{id}/'></a><h3 class='hz-gallery-card__title'><a>Neutral fixture {id}</a></h3><div class='hz-gallery-card__media thumb'><img data-src='https://images.example/{id}.jpg'></div><a class='hz-gallery-card__flag' href='/language/{lang}/'></a></div>"
	)
}

#[aidoku_test]
fn language_settings_cover_seven_languages_all_and_speechless() {
	assert_eq!(language("ru"), "russian");
	assert_eq!(language("ja"), "japanese");
	assert_eq!(language("all"), "");
	assert_eq!(language("speechless"), "speechless");
	assert_eq!(language("unknown"), "english");
	assert_eq!(
		search_url("ignored by site", 2, "speechless", Vec::new()),
		"https://hentaizap.com/language/speechless/popular/?page=2"
	);
	assert_eq!(
		search_url(
			"",
			3,
			"speechless",
			aidoku::alloc::vec![FilterValue::Select {
				id: "sort".into(),
				value: "lt".into()
			}]
		),
		"https://hentaizap.com/language/speechless/?page=3"
	);
}

#[aidoku_test]
fn catalog_pagination_and_languages_follow_galleryadults_routes() {
	assert_eq!(
		listing_url(2, false, "english"),
		"https://hentaizap.com/language/english/popular/?page=2"
	);
	assert_eq!(listing_url(0, true, ""), "https://hentaizap.com/?page=1");
	let html = format!(
		"{}{}<a rel='next' href='?page=2'>Next</a>",
		card(11, "english"),
		card(12, "russian")
	);
	let result = parse_catalog(&document(&html), "", false).unwrap();
	assert_eq!(result.entries.len(), 2);
	assert!(result.has_next_page);
	assert_eq!(result.entries[0].key, "/gallery/11/");
	assert_eq!(
		result.entries[0].cover.as_deref(),
		Some("https://images.example/11.jpg")
	);
}

#[aidoku_test]
fn search_keeps_selected_language_and_speechless_results() {
	let html = format!(
		"{}{}{}",
		card(1, "english"),
		card(2, "russian"),
		card(3, "speechless")
	);
	let result = parse_catalog(&document(&html), "english", true).unwrap();
	assert_eq!(result.entries.len(), 2);
	assert_eq!(result.entries[1].key, "/gallery/3/");
}

#[aidoku_test]
fn intermediate_search_encodes_terms_and_sets_exact_language_flags() {
	let url = search_url("neutral title, sample & page=8", 2, "russian", Vec::new());
	assert!(url.contains("ru=1"));
	assert!(url.contains("en=0"));
	assert!(url.contains("pp=1"));
	assert!(url.contains("key=neutral%2Btitle%2Csample%2B%26%2Bpage%3D8"));
	assert!(url.ends_with("page=2"));
}

#[aidoku_test]
fn only_exact_gallery_and_reader_paths_are_used() {
	assert_eq!(
		gallery_key("https://hentaizap.com/gallery/123?ref=1").unwrap(),
		"/gallery/123/"
	);
	for bad in [
		"https://example.com/gallery/123",
		"/gallery/../",
		"/gallery/%31",
		"https://hentaizap.com.example/gallery/1",
	] {
		assert!(gallery_key(bad).is_err());
	}
	assert!(is_reader_url("https://hentaizap.com/g/123/2/"));
	for bad in [
		"https://hentaizap.com.example/g/123/2/",
		"https://images.example/g/123/2/",
		"https://hentaizap.com/g/123/../",
	] {
		assert!(!is_reader_url(bad));
	}
}

#[aidoku_test]
fn challenges_are_not_misreported_as_an_empty_catalog() {
	assert!(
		parse_catalog(
			&document("<div id='challenge-running'>Check</div>"),
			"",
			false
		)
		.is_err()
	);
	assert!(
		parse_catalog(&document("<div class='hz-gallery-grid'></div>"), "", false)
			.unwrap()
			.entries
			.is_empty()
	);
}

#[aidoku_test]
fn metadata_and_single_chapter_preserve_credits_and_identity() {
	let html = "<div class='hz-gallery-details'><h1>Neutral gallery</h1><div class='hz-gallery-entity-group'>Artists:<a class='hz-gallery-tag'><span class='hz-gallery-tag__name'>Creator</span></a></div><div class='hz-gallery-entity-group'>Groups:<a class='hz-gallery-tag'><span class='hz-gallery-tag__name'>Group</span></a></div></div><div class='hz-gallery-cover'><img src='/cover.jpg'></div>";
	let result = details(
		&document(html),
		Manga {
			key: "/gallery/123/".into(),
			..Default::default()
		},
		true,
		true,
	)
	.unwrap();
	assert_eq!(result.title, "Neutral gallery");
	assert_eq!(result.authors.unwrap()[0], "Creator");
	assert_eq!(result.chapters.unwrap()[0].key, "/gallery/123/");
	assert_eq!(
		result.cover.as_deref(),
		Some("https://hentaizap.com/cover.jpg")
	);
}

#[aidoku_test]
fn chapters_only_refresh_retains_existing_metadata() {
	let result = details(
		&document("<div class='hz-gallery-details'><h1>New</h1></div>"),
		Manga {
			key: "/gallery/1/".into(),
			title: "Retained".into(),
			..Default::default()
		},
		false,
		true,
	)
	.unwrap();
	assert_eq!(result.title, "Retained");
	assert_eq!(result.chapters.unwrap().len(), 1);
}

#[aidoku_test]
fn lazy_reader_urls_cover_every_page_without_guessing_thumbnail_extensions() {
	let result = pages(
		&document("<div data-gallery-id='123' data-total-pages='3'></div>"),
		"https://hentaizap.com/gallery/123/",
	)
	.unwrap();
	assert_eq!(result.len(), 3);
	let PageContent::Url(url, context) = &result[2].content else {
		panic!("url expected")
	};
	assert_eq!(url, "https://hentaizap.com/g/123/3/");
	assert_eq!(
		context.as_ref().unwrap()["referer"],
		"https://hentaizap.com/gallery/123/"
	);
}

#[aidoku_test]
fn legacy_json_metadata_sorts_numeric_indices_and_preserves_animated_extension() {
	let entries = (1..=10)
		.map(|i| format!("\"{i}\":\"{},800,1200\"", if i == 10 { "g" } else { "p" }))
		.collect::<Vec<_>>()
		.join(",");
	let html = format!(
		"<div data-gallery-id='123' data-total-pages='10'></div><input id='load_dir' value='galleries'><input id='load_id' value='456'><input id='load_server' value='3'><script>var pages=$.parseJSON('{{{entries}}}');</script>"
	);
	let result = pages(&document(&html), BASE).unwrap();
	let PageContent::Url(second, _) = &result[1].content else {
		panic!("image expected")
	};
	let PageContent::Url(last, _) = &result[9].content else {
		panic!("image expected")
	};
	assert_eq!(second, "https://m3.hentaizap.com/galleries/456/2.png");
	assert!(last.ends_with("/10.gif"));
}

#[aidoku_test]
fn reader_html_provides_the_exact_full_size_url_and_query_string() {
	let result = reader_image(&document(
		"<img id='gimg' src='placeholder.jpg' data-src='https://images.example/full.webp?rev=2'>",
	))
	.unwrap();
	assert_eq!(result, "https://images.example/full.webp?rev=2");
	assert!(
		reader_image(&document(
			"<img class='advertisement' src='https://images.example/ad.jpg'>"
		))
		.is_err()
	);
}

#[aidoku_test]
fn cloudflare_lazy_images_and_srcset_follow_upstream_attribute_priority() {
	let protected = document(
		"<img id='gimg' data-cfsrc='https://images.example/protected.webp' src='https://images.example/placeholder.jpg'>",
	);
	assert_eq!(
		reader_image(&protected).unwrap(),
		"https://images.example/protected.webp"
	);
	let srcset = document(
		"<img id='gimg' srcset='https://images.example/first.jpg 1x, https://images.example/second.jpg 2x'>",
	);
	assert_eq!(
		reader_image(&srcset).unwrap(),
		"https://images.example/first.jpg"
	);
}

#[aidoku_test]
fn missing_or_unbounded_gallery_counts_fail_explicitly() {
	for html in [
		"<div data-gallery-id='123'></div>",
		"<div data-gallery-id='123' data-total-pages='0'></div>",
		"<div data-gallery-id='123' data-total-pages='999999999'></div>",
	] {
		assert!(pages(&document(html), BASE).is_err());
	}
}
