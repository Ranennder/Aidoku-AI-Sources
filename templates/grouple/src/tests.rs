use super::*;
use aidoku::imports::html::Html;
use aidoku_test::aidoku_test;

#[aidoku_test]
fn catalog_uses_pagination_and_stable_slug() {
	let data: Catalog = serde_json::from_str(r#"{"total":51,"offset":0,"limit":50,"list":[{"name":"Sample","picUrl":"//cdn.example.org/cover.jpg","elementId":{"linkName":"sample__Ab1"}}]}"#).unwrap();
	let result = data.into_page("https://example.org", false);
	assert_eq!(result.entries[0].key, "/sample__Ab1");
	assert_eq!(
		result.entries[0].cover.as_deref(),
		Some("https://cdn.example.org/cover.jpg")
	);
	assert!(result.has_next_page);
}

#[aidoku_test]
fn chapters_preserve_fractional_numbers_hash_and_date() {
	let document = Html::parse(r#"<html><script>var user_hash = 'sample_hash';</script><table>
	<tr class="item-row"><td class="item-title" data-num="125"><a class="chapter-link" href="/sample/vol1/12.5" title="Example (Переводчик)">Part</a></td><td class="date"></td><td class="d-none">04.11.26</td></tr>
	<tr class="item-row"><td><a class="chapter-link" href="/sample/vol1/13">Locked</a></td><td class="date text-info"></td></tr>
	</table></html>"#).unwrap();
	let chapters = parse_chapters(&document, "https://example.org").unwrap();
	assert_eq!(chapters.len(), 1);
	assert_eq!(chapters[0].chapter_number, Some(12.5));
	assert_eq!(chapters[0].key, "/sample/vol1/12.5?mtr=true&d=sample_hash");
	assert_eq!(chapters[0].date_uploaded, Some(1793750400));
	assert_eq!(chapters[0].scanlators, Some(vec![String::from("Example")]));
}

#[aidoku_test]
fn reader_preserves_site_tuple_order_and_signed_query() {
	let script = r#"rm_h.readerInit('sample', [['/images','//a.example.org','/1.jpg?token=abc'],['//b.example.org','/manga/','/2.webp'],['','','/static/3.png']]);"#;
	assert_eq!(
		image_triples(script, "https://example.org"),
		vec![
			"https://a.example.org/images/1.jpg?token=abc",
			"https://b.example.org/2.webp",
			"https://example.org/static/3.png"
		]
	);
}

#[aidoku_test]
fn reader_refuses_missing_pages_and_purchase_form() {
	let empty = Html::parse("<html><body>Login required</body></html>").unwrap();
	assert!(parse_pages(&empty, "https://example.org", "https://example.org/sample").is_err());
	let purchase = Html::parse(r#"<form class="purchase-form"></form><script>rm_h.readerInit([['//cdn.example.org','','/1.jpg']]);</script>"#).unwrap();
	assert!(
		parse_pages(
			&purchase,
			"https://example.org",
			"https://example.org/sample"
		)
		.is_err()
	);
}

#[aidoku_test]
fn details_reject_challenge_page_and_parse_metadata() {
	let challenge = Html::parse("<html><title>Just a moment</title></html>").unwrap();
	assert!(parse_details(&challenge, Manga::default(), "https://example.org", false).is_err());
	let document=Html::parse(r#"<div class="cr-hero-names__main">Sample</div><img class="cr-hero-poster__img" data-src="/cover.jpg"><div class="cr-description__content">Description</div><div class="cr-info-details-item"><span class="cr-info-details-item__title">Выпуск</span><span class="cr-info-details-item__status">Завершён</span></div>"#).unwrap();
	let manga = parse_details(
		&document,
		Manga {
			key: "/sample".into(),
			..Default::default()
		},
		"https://example.org",
		true,
	)
	.unwrap();
	assert_eq!(manga.status, MangaStatus::Completed);
	assert_eq!(manga.content_rating, ContentRating::NSFW);
	assert_eq!(
		manga.cover.as_deref(),
		Some("https://example.org/cover.jpg")
	);
}

#[aidoku_test]
fn keys_and_javascript_data_cannot_inject_other_hosts() {
	assert!(site_path("https://example.org.evil/sample", "https://example.org").is_err());
	assert!(site_path("//evil.org/sample", "https://example.org").is_err());
	assert!(site_path("/sample\r\nInjected: value", "https://example.org").is_err());
	let quoted = r#"'\/sample\u0020name'"#;
	assert_eq!(
		js_string(quoted),
		Some((String::from("/sample name"), quoted.len()))
	);
	assert!(js_string("'unterminated").is_none());
}
