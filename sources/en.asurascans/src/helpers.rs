use crate::BASE_URL;
use aidoku::{
	Page, PageContent, Result,
	alloc::{Vec, string::String},
	prelude::*,
};

pub fn reader_pages(values: &[serde_json::Value], astro: bool) -> Result<Vec<Page>> {
	if values.is_empty() {
		bail!("Asura Scans: chapter has no available pages");
	}
	values
		.iter()
		.map(|value| {
			let url = if astro {
				value[1]["url"][1].as_str()
			} else {
				value
					.as_str()
					.or_else(|| value["url"].as_str())
					.or_else(|| value["url"][1].as_str())
			}
			.filter(|url| url.starts_with("https://") || url.starts_with("http://"))
			.ok_or_else(|| {
				error!("Asura Scans: an image is unavailable; retry or check the source login")
			})?;
			Ok(Page {
				content: PageContent::url(url),
				..Default::default()
			})
		})
		.collect()
}

/// Returns the ID of a manga from a URL.
pub fn get_manga_key(url: &str) -> Option<String> {
	// Asura Scans appends a random string at the end of each series slug
	// The random string is not necessary, along with the trailing '-'

	// remove query parameters
	let path = url.split('?').next().unwrap_or("");

	// find the segment after "series"
	let manga_segment = path
		.split('/')
		.skip_while(|segment| *segment != "comics")
		.nth(1)?;

	// find the last '-' and keep it in the id
	let pos = manga_segment.rfind('-')?;
	Some(manga_segment[..pos].into())
}

/// Returns the ID of a chapter from a URL.
pub fn get_chapter_key(url: &str) -> Option<String> {
	// remove query parameters
	let path = url.split('?').next().unwrap_or("");

	// find the segment after "chapter"
	let chapter_segment = path
		.split('/')
		.skip_while(|segment| *segment != "chapter")
		.nth(1)?;

	// extract only the numeric (and '.') prefix
	let end_pos = chapter_segment
		.find(|c: char| !c.is_numeric() && c != '.')
		.unwrap_or(chapter_segment.len());

	Some(chapter_segment[..end_pos].into())
}

/// Returns full URL of a manga from a manga ID.
pub fn get_manga_url(manga_id: &str) -> String {
	format!("{BASE_URL}/comics/{manga_id}")
}

/// Returns full URL of a chapter from a chapter ID and manga ID.
pub fn get_chapter_url(chapter_id: &str, manga_id: &str) -> String {
	format!("{BASE_URL}/comics/{manga_id}/chapter/{chapter_id}")
}

#[cfg(test)]
mod tests {
	use super::*;
	use aidoku_test::aidoku_test;

	#[aidoku_test]
	fn test_manga_keys() {
		assert_eq!(
			get_manga_key("https://asurascans.com/comics/swordmasters-youngest-son-cb22671f")
				.as_deref(),
			Some("swordmasters-youngest-son")
		);
		assert_eq!(
			get_manga_key(
				"https://asurascans.com/comics/swordmasters-youngest-son-cb22671f?blahblah"
			)
			.as_deref(),
			Some("swordmasters-youngest-son")
		);
		assert_eq!(
			get_manga_key(
				"https://asurascans.com/comics/swordmasters-youngest-son-cb22671f/chapter/1"
			)
			.as_deref(),
			Some("swordmasters-youngest-son")
		);
	}

	#[aidoku_test]
	fn test_chapter_keys() {
		assert_eq!(
			get_chapter_key("https://asurascans.com/comics/swordmasters-youngest-son-cb22671f"),
			None
		);
		assert_eq!(
			get_chapter_key(
				"https://asurascans.com/comics/swordmasters-youngest-son-cb22671f?blahblah"
			),
			None
		);
		assert_eq!(
			get_chapter_key(
				"https://asurascans.com/comics/swordmasters-youngest-son-cb22671f/chapter/1"
			)
			.as_deref(),
			Some("1")
		);
		assert_eq!(
			get_chapter_key("https://asurascans.com/comics/swordmasters-youngest-son/chapter/1")
				.as_deref(),
			Some("1")
		);
	}

	#[aidoku_test]
	fn api_reader_keeps_all_supported_url_shapes_and_original_order() {
		let values: serde_json::Value = serde_json::from_str(r#"["https://images.example/2.webp?v=1",{"url":"https://images.example/1.gif"},{"url":[0,"https://images.example/3.jpg"]}]"#).unwrap();
		let pages = reader_pages(values.as_array().unwrap(), false).unwrap();
		assert_eq!(pages.len(), 3);
		assert_eq!(
			pages[0].content,
			PageContent::url("https://images.example/2.webp?v=1")
		);
		assert_eq!(
			pages[1].content,
			PageContent::url("https://images.example/1.gif")
		);
	}

	#[aidoku_test]
	fn astro_reader_keeps_its_encoded_array_and_rejects_partial_or_empty_chapters() {
		let values: serde_json::Value = serde_json::from_str(r#"[[0,{"url":[0,"https://images.example/1.jpg"]}],[0,{"url":[0,"https://images.example/2.jpg"]}]]"#).unwrap();
		let pages = reader_pages(values.as_array().unwrap(), true).unwrap();
		assert_eq!(pages.len(), 2);
		assert_eq!(
			pages[1].content,
			PageContent::url("https://images.example/2.jpg")
		);
		assert!(reader_pages(&[], false).is_err());
		let partial: serde_json::Value =
			serde_json::from_str(r#"["https://images.example/1.jpg",{}]"#).unwrap();
		assert!(reader_pages(partial.as_array().unwrap(), false).is_err());
	}
}
