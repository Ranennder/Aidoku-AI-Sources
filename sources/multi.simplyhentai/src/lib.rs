// Protocol adapted from Keiyoushi SimplyHentai; pinned attribution in UPSTREAM.md.
#![no_std]

use aidoku::{
	Chapter, DeepLinkHandler, DeepLinkResult, FilterValue, ImageRequestProvider, Listing,
	ListingProvider, Manga, MangaPageResult, MangaStatus, Page, PageContent, PageContext, Result,
	Source,
	alloc::{String, Vec, string::ToString},
	helpers::uri::QueryParameters,
	imports::{defaults::defaults_get, net::Request},
	prelude::*,
};
use serde::Deserialize;

const BASE: &str = "https://www.simply-hentai.com";
const API: &str = "https://api.simply-hentai.com/v3";

fn language(value: &str) -> (&'static str, &'static str) {
	match value {
		"ja" => ("japanese", "Japanese"),
		"zh" => ("chinese", "Chinese"),
		"ko" => ("korean", "Korean"),
		"es" => ("spanish", "Spanish"),
		"ru" => ("russian", "Russian"),
		"fr" => ("french", "French"),
		"de" => ("german", "German"),
		"it" => ("italian", "Italian"),
		"pl" => ("polish", "Polish"),
		_ => ("english", "English"),
	}
}

fn selected_language() -> (&'static str, &'static str) {
	language(&defaults_get::<String>("language").unwrap_or_else(|| "en".into()))
}

fn key(value: &str) -> Result<String> {
	let path = value
		.strip_prefix(BASE)
		.or_else(|| value.strip_prefix("https://simply-hentai.com"))
		.unwrap_or(value);
	let path = path
		.split(['?', '#'])
		.next()
		.unwrap_or_default()
		.trim_end_matches('/');
	let parts: Vec<_> = path.split('/').collect();
	if parts.len() != 3
		|| !parts[0].is_empty()
		|| parts.iter().skip(1).any(|part| {
			part.is_empty() || *part == "." || *part == ".." || part.contains([':', '\\', '%'])
		}) {
		bail!("Simply Hentai: invalid gallery URL.");
	}
	Ok(path.into())
}

fn http_json<T: serde::de::DeserializeOwned>(url: &str) -> Result<T> {
	let response = Request::get(url)?
		.header("Referer", &format!("{BASE}/"))
		.header("Accept", "application/json")
		.header("X-Requested-With", "XMLHttpRequest")
		.timeout(30.0)
		.send()?;
	if !(200..300).contains(&response.status_code()) {
		bail!(
			"Simply Hentai returned HTTP {}. Open the source WebView and check access.",
			response.status_code()
		);
	}
	response.get_json_owned()
}

#[derive(Deserialize)]
struct Envelope<T> {
	data: T,
}
#[derive(Deserialize)]
struct List<T> {
	data: T,
	pagination: Pagination,
}
#[derive(Deserialize)]
struct Pagination {
	next: Option<i32>,
}
#[derive(Deserialize)]
struct Albums {
	albums: Vec<Record>,
}
#[derive(Deserialize)]
struct Wrapped {
	object: Record,
}
#[derive(Deserialize)]
struct Tag {
	slug: String,
	title: String,
}
#[derive(Deserialize)]
struct Sizes {
	full: String,
	thumb: String,
}
#[derive(Deserialize)]
struct Image {
	#[serde(default)]
	page_num: i32,
	sizes: Sizes,
}
#[derive(Deserialize)]
struct Record {
	preview: Image,
	series: Tag,
	slug: String,
	title: String,
}
impl Record {
	fn into_manga(self) -> Result<Manga> {
		let key = key(&format!("/{}/{}", self.series.slug, self.slug))?;
		Ok(Manga {
			url: Some(format!("{BASE}{key}")),
			key,
			title: self.title,
			cover: Some(self.preview.sizes.thumb),
			..Default::default()
		})
	}
}

fn listing_url(page: i32, latest: bool, lang: &str) -> String {
	let mut query = QueryParameters::new();
	query.push("type", Some("language"));
	query.push("page", Some(&page.max(1).to_string()));
	if latest {
		query.push("sort", Some("newest"));
	}
	format!("{API}/tag/{lang}?{query}")
}

fn search_url(
	query: &str,
	page: i32,
	lang: &str,
	filters: Vec<FilterValue>,
	blacklist: &str,
) -> String {
	let mut params = QueryParameters::new();
	params.push("query", Some(query));
	params.push("page", Some(&page.max(1).to_string()));
	params.push("blacklist", Some(blacklist));
	params.push("filter[language][0]", Some(lang));
	for filter in filters {
		match filter {
			FilterValue::Select { id, value } if id == "sort" => {
				params.push("sort", Some(&value));
			}
			FilterValue::Text { id, value }
				if [
					"series_title",
					"tags",
					"artists",
					"translators",
					"characters",
				]
				.contains(&id.as_str()) =>
			{
				for (index, term) in value
					.split(',')
					.map(str::trim)
					.filter(|s| !s.is_empty())
					.enumerate()
				{
					params.push(&format!("filter[{id}][{index}]"), Some(term));
				}
			}
			_ => {}
		}
	}
	format!("{API}/search/complex?{params}")
}

#[derive(Deserialize)]
struct Album {
	slug: String,
	title: String,
	series: Tag,
	preview: Image,
	#[serde(default)]
	description: Option<String>,
	#[serde(default)]
	artists: Vec<Tag>,
	#[serde(default)]
	characters: Vec<Tag>,
	#[serde(default)]
	tags: Vec<Tag>,
	#[serde(default)]
	translators: Vec<Tag>,
	#[serde(default)]
	created_at: Option<String>,
}
impl Album {
	fn update(self, mut manga: Manga, details: bool, chapters: bool) -> Result<Manga> {
		let canonical = key(&format!("/{}/{}", self.series.slug, self.slug))?;
		if details {
			manga.key = canonical.clone();
			manga.url = Some(format!("{BASE}{canonical}"));
			manga.title = self.title;
			manga.cover = Some(self.preview.sizes.thumb);
			let mut description = self.description.unwrap_or_default();
			description.push_str(&format!("\n\nSeries: {}", self.series.title));
			if !self.characters.is_empty() {
				description.push_str(&format!(
					"\nCharacters: {}",
					self.characters
						.iter()
						.map(|t| t.title.as_str())
						.collect::<Vec<_>>()
						.join(", ")
				));
			}
			manga.description = Some(description.trim().into());
			let artists: Vec<_> = self.artists.into_iter().map(|tag| tag.title).collect();
			manga.authors = Some(artists.clone());
			manga.artists = Some(artists);
			manga.tags = Some(self.tags.into_iter().map(|tag| tag.title).collect());
			manga.status = MangaStatus::Completed;
		}
		if chapters {
			let chapter_key = format!("{canonical}/all-pages");
			manga.chapters = Some(aidoku::alloc::vec![Chapter {
				url: Some(format!("{BASE}{chapter_key}")),
				key: chapter_key,
				title: Some("Chapter".into()),
				chapter_number: Some(1.0),
				scanlators: Some(self.translators.into_iter().map(|tag| tag.title).collect()),
				date_uploaded: self
					.created_at
					.and_then(|value| chrono::DateTime::parse_from_rfc3339(&value).ok())
					.map(|date| date.timestamp()),
				..Default::default()
			}]);
		}
		Ok(manga)
	}
}

#[derive(Deserialize)]
struct Pages {
	pages: Vec<Image>,
}
fn parse_pages(mut data: Pages) -> Result<Vec<Page>> {
	data.pages.sort_by_key(|page| page.page_num);
	if data.pages.is_empty() {
		bail!("Simply Hentai: this gallery has no available pages.");
	}
	data.pages
		.into_iter()
		.map(|page| {
			if !page.sizes.full.starts_with("https://") && !page.sizes.full.starts_with("http://") {
				bail!("Simply Hentai: invalid page URL.");
			}
			Ok(Page {
				content: PageContent::url(page.sizes.full),
				..Default::default()
			})
		})
		.collect()
}

struct SimplyHentai;
impl Source for SimplyHentai {
	fn new() -> Self {
		Self
	}
	fn get_search_manga_list(
		&self,
		query: Option<String>,
		page: i32,
		filters: Vec<FilterValue>,
	) -> Result<MangaPageResult> {
		let (_, lang) = selected_language();
		let url = search_url(
			query.as_deref().unwrap_or_default(),
			page,
			lang,
			filters,
			&defaults_get::<String>("blacklist").unwrap_or_default(),
		);
		let response: List<Vec<Wrapped>> = http_json(&url)?;
		let has_next_page = response.pagination.next.is_some();
		Ok(MangaPageResult {
			entries: response
				.data
				.into_iter()
				.map(|item| item.object.into_manga())
				.collect::<Result<_>>()?,
			has_next_page,
		})
	}
	fn get_manga_update(&self, manga: Manga, details: bool, chapters: bool) -> Result<Manga> {
		if !details && !chapters {
			return Ok(manga);
		}
		let canonical = key(&manga.key)?;
		let slug = canonical.rsplit('/').next().unwrap_or_default();
		let response: Envelope<Album> = http_json(&format!("{API}/manga/{slug}"))?;
		response.data.update(manga, details, chapters)
	}
	fn get_page_list(&self, _manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let raw = chapter
			.key
			.strip_suffix("/all-pages")
			.ok_or_else(|| error!("Simply Hentai: invalid chapter key."))?;
		let canonical = key(raw)?;
		let slug = canonical.rsplit('/').next().unwrap_or_default();
		let response: Envelope<Pages> = http_json(&format!("{API}/manga/{slug}/pages"))?;
		parse_pages(response.data)
	}
}
impl ListingProvider for SimplyHentai {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		if listing.id != "popular" && listing.id != "latest" {
			bail!("Simply Hentai: unknown listing.");
		}
		let response: List<Albums> = http_json(&listing_url(
			page,
			listing.id == "latest",
			selected_language().0,
		))?;
		let has_next_page = response.pagination.next.is_some();
		Ok(MangaPageResult {
			entries: response
				.data
				.albums
				.into_iter()
				.map(Record::into_manga)
				.collect::<Result<_>>()?,
			has_next_page,
		})
	}
}
impl ImageRequestProvider for SimplyHentai {
	fn get_image_request(&self, url: String, _context: Option<PageContext>) -> Result<Request> {
		Ok(Request::get(url)?.header("Referer", &format!("{BASE}/")))
	}
}
impl DeepLinkHandler for SimplyHentai {
	fn handle_deep_link(&self, url: String) -> Result<Option<DeepLinkResult>> {
		Ok(key(&url).ok().map(|key| DeepLinkResult::Manga { key }))
	}
}
register_source!(
	SimplyHentai,
	ListingProvider,
	ImageRequestProvider,
	DeepLinkHandler
);

#[cfg(test)]
mod tests;
