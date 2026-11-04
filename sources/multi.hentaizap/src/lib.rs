// Selectors/protocol adapted from Keiyoushi HentaiZap/GalleryAdults (UPSTREAM.md).
#![no_std]
use aidoku::{
	Chapter, DeepLinkHandler, DeepLinkResult, FilterValue, ImageRequestProvider, Listing,
	ListingProvider, Manga, MangaPageResult, MangaStatus, Page, PageContent, PageContext, Result,
	Source,
	alloc::{String, Vec, collections::BTreeMap, string::ToString},
	helpers::uri::QueryParameters,
	imports::{
		defaults::defaults_get,
		html::{Document, Element},
		net::Request,
	},
	prelude::*,
};
const BASE: &str = "https://hentaizap.com";

fn language(code: &str) -> &'static str {
	match code {
		"ja" => "japanese",
		"es" => "spanish",
		"fr" => "french",
		"ko" => "korean",
		"de" => "german",
		"ru" => "russian",
		"all" => "",
		"speechless" => "speechless",
		_ => "english",
	}
}
fn selected_language() -> &'static str {
	let override_language = defaults_get::<String>("siteLanguage").unwrap_or_else(|| "auto".into());
	if override_language == "auto" {
		language(&defaults_get::<String>("language").unwrap_or_else(|| "en".into()))
	} else {
		language(&override_language)
	}
}
fn gallery_key(value: &str) -> Result<String> {
	let path = value
		.strip_prefix(BASE)
		.unwrap_or(value)
		.split(['?', '#'])
		.next()
		.unwrap_or_default()
		.trim_end_matches('/');
	let parts: Vec<_> = path.split('/').collect();
	if parts.len() != 3
		|| !parts[0].is_empty()
		|| parts[1] != "gallery"
		|| parts[2].is_empty()
		|| !parts[2]
			.bytes()
			.all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
	{
		bail!("HentaiZap: invalid gallery URL.");
	}
	Ok(format!("/gallery/{}/", parts[2]))
}
fn absolute(value: &str) -> Option<String> {
	let value = value.trim();
	if value.starts_with("https://") || value.starts_with("http://") {
		Some(value.into())
	} else if value.starts_with("//") {
		Some(format!("https:{value}"))
	} else if value.starts_with('/') {
		Some(format!("{BASE}{value}"))
	} else {
		None
	}
}
fn image_url(element: &Element) -> Option<String> {
	[
		"data-cfsrc",
		"data-src",
		"data-lazy-src",
		"data-original",
		"srcset",
		"src",
	]
	.iter()
	.find_map(|name| {
		element
			.attr(*name)
			.filter(|s| !s.is_empty() && !s.starts_with("data:"))
			.and_then(|s| {
				s.split_whitespace()
					.next()
					.and_then(|url| absolute(url.trim_end_matches(',')))
			})
	})
}
fn fetch(url: &str) -> Result<Document> {
	let response = Request::get(url)?
		.header("Referer", &format!("{BASE}/"))
		.timeout(30.0)
		.send()?;
	if !(200..300).contains(&response.status_code()) {
		bail!(
			"HentaiZap HTTP {}. Check access in the source WebView.",
			response.status_code()
		);
	}
	let document = response.get_html()?;
	check_document(&document)?;
	Ok(document)
}
fn check_document(document: &Document) -> Result<()> {
	if document
		.select_first("#challenge-running, #cf-challenge-running, .cf-browser-verification")
		.is_some()
	{
		bail!("HentaiZap: complete the website check in the source WebView.");
	}
	Ok(())
}
fn listing_url(page: i32, latest: bool, lang: &str) -> String {
	let language_path = if lang.is_empty() {
		String::new()
	} else {
		format!("language/{lang}/")
	};
	format!(
		"{BASE}/{language_path}{}?page={}",
		if latest { "" } else { "popular/" },
		page.max(1)
	)
}
fn search_url(query: &str, page: i32, lang: &str, filters: Vec<FilterValue>) -> String {
	// The site excludes speechless galleries from its normal search endpoint.
	if lang == "speechless" {
		let latest = filters.iter().any(
			|filter| matches!(filter, FilterValue::Select { id, value } if id == "sort" && value != "pp"),
		);
		return listing_url(page, latest, lang);
	}
	let mut params = QueryParameters::new();
	let mut sort = String::from("pp");
	let mut categories = Vec::new();
	for filter in filters {
		match filter {
			FilterValue::Select { id, value }
				if id == "sort" && ["pp", "lt", "dl", "tr"].contains(&value.as_str()) =>
			{
				sort = value
			}
			FilterValue::MultiSelect { id, included, .. } if id == "category" => {
				categories = included
			}
			_ => {}
		}
	}
	for name in ["pp", "lt", "dl", "tr"] {
		params.push(name, Some(if sort == name { "1" } else { "0" }));
	}
	for (name, code) in [
		("english", "en"),
		("japanese", "jp"),
		("spanish", "es"),
		("french", "fr"),
		("korean", "kr"),
		("german", "de"),
		("russian", "ru"),
	] {
		params.push(
			code,
			Some(if lang.is_empty() || lang == name {
				"1"
			} else {
				"0"
			}),
		);
	}
	for name in ["m", "d", "w", "i", "a", "g"] {
		params.push(
			name,
			Some(if categories.iter().any(|c| c == name) {
				"1"
			} else {
				"0"
			}),
		);
	}
	let terms = query
		.split(',')
		.map(|term| term.split_whitespace().collect::<Vec<_>>().join("+"))
		.collect::<Vec<_>>()
		.join(",");
	params.push("key", Some(&terms));
	params.push("page", Some(&page.max(1).to_string()));
	format!("{BASE}/search/?{params}")
}
fn parse_catalog(
	document: &Document,
	lang: &str,
	filter_language: bool,
) -> Result<MangaPageResult> {
	check_document(document)?;
	let has_next_page = document
		.select_first(".pagination li.active + li:not(.disabled), a[rel='next']")
		.is_some();
	let mut entries = Vec::new();
	for element in document.select(".hz-gallery-card").into_iter().flatten() {
		let Some(href) = element
			.select_first(".hz-gallery-card__cover")
			.and_then(|a| a.attr("href"))
		else {
			continue;
		};
		let Ok(key) = gallery_key(&href) else {
			continue;
		};
		let Some(title) = element
			.select_first(".hz-gallery-card__title a")
			.and_then(|e| e.text())
			.filter(|s| !s.is_empty())
		else {
			continue;
		};
		let entry_language = element
			.select_first(".hz-gallery-card__flag")
			.and_then(|e| e.attr("href"))
			.and_then(|s| s.trim_end_matches('/').rsplit('/').next().map(String::from))
			.unwrap_or_default();
		if filter_language
			&& !lang.is_empty()
			&& !entry_language.is_empty()
			&& entry_language != lang
			&& entry_language != "speechless"
		{
			continue;
		}
		let cover = element
			.select_first(".hz-gallery-card__media.thumb img")
			.and_then(|e| image_url(&e));
		if !entries.iter().any(|manga: &Manga| manga.key == key) {
			entries.push(Manga {
				url: Some(format!("{BASE}{key}")),
				key,
				title,
				cover,
				..Default::default()
			});
		}
	}
	Ok(MangaPageResult {
		entries,
		has_next_page,
	})
}
fn info(document: &Element, label: &str) -> Vec<String> {
	let mut values = Vec::new();
	for group in document
		.select("div.hz-gallery-entity-group")
		.into_iter()
		.flatten()
	{
		if !group
			.text()
			.unwrap_or_default()
			.trim_start()
			.starts_with(&format!("{label}:"))
		{
			continue;
		}
		for tag in group.select("a.hz-gallery-tag").into_iter().flatten() {
			if let Some(name) = tag
				.select_first(".hz-gallery-tag__name")
				.and_then(|e| e.text())
				.or_else(|| tag.own_text())
				.filter(|s| !s.trim().is_empty())
			{
				let name: String = name.trim().into();
				if !values.contains(&name) {
					values.push(name);
				}
			}
		}
	}
	values
}
fn details(
	document: &Document,
	mut manga: Manga,
	wants_details: bool,
	wants_chapters: bool,
) -> Result<Manga> {
	let canonical = gallery_key(&manga.key)?;
	let element = document
		.select_first(".hz-gallery-details")
		.ok_or_else(|| {
			error!("HentaiZap: gallery details are unavailable. Check access in WebView.")
		})?;
	if wants_details {
		manga.title = element
			.select_first("h1")
			.and_then(|e| e.text())
			.filter(|s| !s.is_empty())
			.ok_or_else(|| error!("HentaiZap: missing gallery title."))?;
		manga.cover = document
			.select_first(".hz-gallery-cover img")
			.and_then(|e| image_url(&e));
		let mut authors = info(&element, "Artists");
		if authors.is_empty() {
			authors = info(&element, "Groups");
		}
		manga.authors = Some(authors.clone());
		manga.artists = Some(authors);
		manga.tags = Some(info(&element, "Tags"));
		let mut description = Vec::new();
		for label in [
			"Parodies",
			"Characters",
			"Groups",
			"Languages",
			"Categories",
			"Category",
		] {
			let values = info(&element, label);
			if !values.is_empty() {
				description.push(format!("{label}: {}", values.join(", ")));
			}
		}
		manga.description = Some(description.join("\n\n"));
		manga.url = Some(format!("{BASE}{canonical}"));
		manga.status = MangaStatus::Completed;
	}
	if wants_chapters {
		manga.chapters = Some(aidoku::alloc::vec![Chapter {
			key: canonical.clone(),
			url: Some(format!("{BASE}{canonical}")),
			title: Some("Chapter".into()),
			chapter_number: Some(1.0),
			scanlators: Some(info(&element, "Groups")),
			..Default::default()
		}]);
	}
	Ok(manga)
}
fn numeric_attribute(document: &Document, selector: &str, attribute: &str) -> Result<usize> {
	let number = document
		.select_first(selector)
		.and_then(|e| e.attr(attribute))
		.and_then(|s| s.parse::<usize>().ok())
		.filter(|n| *n > 0 && *n <= 10_000)
		.ok_or_else(|| error!("HentaiZap: missing or invalid gallery pagination."))?;
	Ok(number)
}
fn page(content: String, referer: &str) -> Page {
	let mut context = PageContext::new();
	context.insert("referer".into(), referer.into());
	Page {
		content: PageContent::url_context(content, context),
		..Default::default()
	}
}
fn json_images(document: &Document, total: usize, referer: &str) -> Option<Vec<Page>> {
	let raw = document
		.select("script")
		.into_iter()
		.flatten()
		.filter_map(|script| script.html())
		.find_map(|script| {
			script
				.split_once("$.parseJSON('")
				.and_then(|(_, body)| body.split_once("');").map(|(json, _)| String::from(json)))
		})?;
	let map: BTreeMap<String, String> = serde_json::from_str(&raw).ok()?;
	if map.len() != total {
		return None;
	}
	let input = |id: &str| {
		document
			.select_first(format!("input[id={id}]"))
			.and_then(|e| e.attr("value"))
	};
	let directory = input("load_dir")?;
	let id = input("load_id")?;
	if directory.is_empty()
		|| id.is_empty()
		|| directory.contains([':', '\\'])
		|| id.contains([':', '\\', '/'])
	{
		return None;
	}
	let server = if let Some(number) =
		input("load_server").filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
	{
		format!("https://m{number}.hentaizap.com")
	} else {
		let cover = document
			.select_first(".hz-gallery-cover img")
			.and_then(|e| image_url(&e))?;
		let host = cover
			.strip_prefix("https://")
			.or_else(|| cover.strip_prefix("http://"))?
			.split('/')
			.next()?;
		if host.is_empty() || host.contains('@') {
			return None;
		}
		format!("https://{host}")
	};
	let mut values = Vec::new();
	for (number, value) in map {
		let number: usize = number.parse().ok()?;
		if number == 0 || number > total {
			return None;
		}
		let extension = match value.split(',').next()? {
			"j" => "jpg",
			"p" => "png",
			"b" => "bmp",
			"g" => "gif",
			"w" => "webp",
			_ => return None,
		};
		values.push((
			number,
			format!(
				"{server}/{}/{id}/{number}.{extension}",
				directory.trim_matches('/')
			),
		));
	}
	values.sort_by_key(|(number, _)| *number);
	if values
		.iter()
		.enumerate()
		.any(|(index, (number, _))| *number != index + 1)
	{
		return None;
	}
	Some(
		values
			.into_iter()
			.map(|(_, url)| page(url, referer))
			.collect(),
	)
}
fn pages(document: &Document, referer: &str) -> Result<Vec<Page>> {
	let total = numeric_attribute(document, "[data-total-pages]", "data-total-pages")?;
	let id = document
		.select_first("[data-gallery-id]")
		.and_then(|e| e.attr("data-gallery-id"))
		.filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
		.ok_or_else(|| error!("HentaiZap: gallery ID unavailable."))?;
	if let Some(pages) = json_images(document, total, referer) {
		return Ok(pages);
	}
	// Exact reader URLs; never guess full-size image paths from thumbnails.
	Ok((1..=total)
		.map(|index| page(format!("{BASE}/g/{id}/{index}/"), referer))
		.collect())
}
fn reader_image(document: &Document) -> Result<String> {
	document
		.select_first("img#gimg, img#fimg, img#readerImg")
		.and_then(|image| image_url(&image))
		.ok_or_else(|| error!("HentaiZap: full-size page unavailable. Check access in WebView."))
}
fn is_reader_url(url: &str) -> bool {
	let Some(path) = url.strip_prefix(BASE) else {
		return false;
	};
	let segments: Vec<_> = path.trim_matches('/').split('/').collect();
	segments.len() == 3
		&& segments[0] == "g"
		&& segments[1..]
			.iter()
			.all(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
}

struct HentaiZap;
impl Source for HentaiZap {
	fn new() -> Self {
		Self
	}
	fn get_search_manga_list(
		&self,
		query: Option<String>,
		page_number: i32,
		filters: Vec<FilterValue>,
	) -> Result<MangaPageResult> {
		let query = query.unwrap_or_default();
		if !query.trim().is_empty() && query.trim().bytes().all(|b| b.is_ascii_digit()) {
			let canonical = gallery_key(&format!("/gallery/{}/", query.trim()))?;
			let manga = details(
				&fetch(&format!("{BASE}{canonical}"))?,
				Manga {
					key: canonical,
					..Default::default()
				},
				true,
				false,
			)?;
			return Ok(MangaPageResult {
				entries: aidoku::alloc::vec![manga],
				has_next_page: false,
			});
		}
		let lang = selected_language();
		parse_catalog(
			&fetch(&search_url(&query, page_number, lang, filters))?,
			lang,
			true,
		)
	}
	fn get_manga_update(
		&self,
		manga: Manga,
		wants_details: bool,
		wants_chapters: bool,
	) -> Result<Manga> {
		if !wants_details && !wants_chapters {
			return Ok(manga);
		}
		let canonical = gallery_key(&manga.key)?;
		details(
			&fetch(&format!("{BASE}{canonical}"))?,
			manga,
			wants_details,
			wants_chapters,
		)
	}
	fn get_page_list(&self, _manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let canonical = gallery_key(&chapter.key)?;
		let referer = format!("{BASE}{canonical}");
		pages(&fetch(&referer)?, &referer)
	}
}
impl ListingProvider for HentaiZap {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		if listing.id != "popular" && listing.id != "latest" {
			bail!("HentaiZap: unknown listing.");
		}
		let lang = selected_language();
		parse_catalog(
			&fetch(&listing_url(page, listing.id == "latest", lang))?,
			lang,
			false,
		)
	}
}
impl ImageRequestProvider for HentaiZap {
	fn get_image_request(&self, url: String, context: Option<PageContext>) -> Result<Request> {
		let referer = context
			.as_ref()
			.and_then(|context| context.get("referer"))
			.filter(|value| value.starts_with(&format!("{BASE}/")))
			.map(String::as_str)
			.unwrap_or(BASE);
		if is_reader_url(&url) {
			let resolved = reader_image(&fetch(&url)?)?;
			Ok(Request::get(resolved)?.header("Referer", &url))
		} else {
			Ok(Request::get(url)?.header("Referer", referer))
		}
	}
}
impl DeepLinkHandler for HentaiZap {
	fn handle_deep_link(&self, url: String) -> Result<Option<DeepLinkResult>> {
		Ok(gallery_key(&url)
			.ok()
			.map(|key| DeepLinkResult::Manga { key }))
	}
}
register_source!(
	HentaiZap,
	ListingProvider,
	ImageRequestProvider,
	DeepLinkHandler
);
#[cfg(test)]
mod tests;
