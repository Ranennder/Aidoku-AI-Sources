use super::*;
use aidoku_test::aidoku_test;

const TEST_BASE_URL: &str = "https://api.cdnlibs.org";

#[aidoku_test]
fn basic_urls() {
	assert_eq!(
		Url::manga_search(TEST_BASE_URL),
		"https://api.cdnlibs.org/api/manga"
	);

	assert_eq!(
		Url::manga_details(TEST_BASE_URL, "test-manga"),
		"https://api.cdnlibs.org/api/manga/test-manga"
	);

	assert_eq!(
		Url::manga_chapters(TEST_BASE_URL, "test-manga"),
		"https://api.cdnlibs.org/api/manga/test-manga/chapters"
	);

	assert_eq!(
		Url::manga_covers(TEST_BASE_URL, "test-manga"),
		"https://api.cdnlibs.org/api/manga/test-manga/covers"
	);

	assert_eq!(
		Url::constants(TEST_BASE_URL),
		"https://api.cdnlibs.org/api/constants"
	);

	assert_eq!(
		Url::auth_me(TEST_BASE_URL),
		"https://api.cdnlibs.org/api/auth/me"
	);
}

#[aidoku_test]
fn chapter_pages_url() {
	let url = Url::chapter_pages(TEST_BASE_URL, "test-manga");

	assert_eq!(url, "https://api.cdnlibs.org/api/manga/test-manga/chapter");
}

#[aidoku_test]
fn manga_search_with_params() {
	let url = Url::manga_search_with_params(
		TEST_BASE_URL,
		&[
			("q", "naruto"),
			("bookmarks_exclude[]", "2"),
			("buy", "1"),
			("caution[]", "3"),
			("fields[]", "rate"),
			("fields[]", "rate_avg"),
			("format[]", "6"),
			("genres[]", "41"),
			("licensed", "1"),
			("status[]", "1"),
		],
	);

	assert!(url.starts_with("https://api.cdnlibs.org/api/manga?"));
	assert!(url.contains("q=naruto"));
	assert!(url.contains("buy=1"));
	assert!(url.contains("licensed=1"));
	assert!(url.contains("bookmarks_exclude%5B%5D=2"));
}

#[aidoku_test]
fn manga_details_with_fields() {
	let url = Url::manga_details_with_fields(
		TEST_BASE_URL,
		"test-manga",
		&["summary", "tags", "authors", "artists"],
	);

	assert_eq!(
		url,
		"https://api.cdnlibs.org/api/manga/test-manga?fields%5B%5D=summary&fields%5B%5D=tags&fields%5B%5D=authors&fields%5B%5D=artists"
	);
}

#[aidoku_test]
fn manga_covers() {
	let url = Url::manga_covers(TEST_BASE_URL, "test-manga");

	assert_eq!(url, "https://api.cdnlibs.org/api/manga/test-manga/covers");
}

#[aidoku_test]
fn chapter_pages_with_params() {
	let url = Url::chapter_pages_with_params(TEST_BASE_URL, "test-manga", Some(123), "1", "1");

	assert_eq!(
		url,
		"https://api.cdnlibs.org/api/manga/test-manga/chapter?number=1&volume=1&branch_id=123"
	);

	let url_no_branch =
		Url::chapter_pages_with_params(TEST_BASE_URL, "test-manga", None, "2.5", "2");
	assert_eq!(
		url_no_branch,
		"https://api.cdnlibs.org/api/manga/test-manga/chapter?number=2.5&volume=2"
	);
}

#[aidoku_test]
fn constants_with_fields() {
	let url = Url::constants_with_fields(
		TEST_BASE_URL,
		&[
			"genres",
			"tags",
			"scanlateStatus",
			"status",
			"format",
			"ageRestriction",
			"imageServers",
		],
	);

	assert!(url.starts_with("https://api.cdnlibs.org/api/constants?"));
	assert!(url.contains("fields%5B%5D=genres"));
	assert!(url.contains("fields%5B%5D=tags"));
	assert!(url.contains("fields%5B%5D=scanlateStatus"));
	assert!(url.contains("fields%5B%5D=status"));
	assert!(url.contains("fields%5B%5D=format"));
	assert!(url.contains("fields%5B%5D=ageRestriction"));
	assert!(url.contains("fields%5B%5D=imageServers"));

	let image_servers_url = Url::constants_with_fields(TEST_BASE_URL, &["imageServers"]);
	assert_eq!(
		image_servers_url,
		"https://api.cdnlibs.org/api/constants?fields%5B%5D=imageServers"
	);

	let empty_url = Url::constants_with_fields(TEST_BASE_URL, &[]);
	assert_eq!(empty_url, "https://api.cdnlibs.org/api/constants");
}

#[aidoku_test]
fn trailing_slash_handling() {
	let base_with_slash = "https://api.example.com/";
	let base_without_slash = "https://api.example.com";

	let url1 = Url::manga_search(base_with_slash);
	let url2 = Url::manga_search(base_without_slash);

	assert_eq!(url1, url2);
	assert_eq!(url1, "https://api.example.com/api/manga");
}

#[aidoku_test]
fn search_special_characters_are_one_literal_query_value() {
	let url = Url::manga_search_with_params(
		TEST_BASE_URL,
		&[("q", "А & B+C#?=100%"), ("site_id[]", "1")],
	);
	assert_eq!(
		url,
		"https://api.cdnlibs.org/api/manga?q=%D0%90%20%26%20B%2BC%23%3F%3D100%25&site_id%5B%5D=1"
	);
	assert_eq!(url.matches('&').count(), 1);
}

#[aidoku_test]
fn repeated_array_filters_are_encoded_without_combining_values() {
	let url = Url::manga_search_with_params(TEST_BASE_URL, &[("genres[]", "1"), ("genres[]", "2")]);
	assert!(url.ends_with("genres%5B%5D=1&genres%5B%5D=2"));
}
