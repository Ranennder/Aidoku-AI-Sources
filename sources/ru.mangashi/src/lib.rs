// Protocol and selectors adapted from Keiyoushi MangaShi (Apache-2.0).
#![no_std]
use aidoku::{
	Chapter, DeepLinkHandler, DeepLinkResult, DynamicFilters, Filter, FilterValue,
	ImageRequestProvider, Listing, ListingProvider, Manga, MangaPageResult, MangaStatus,
	MultiSelectFilter, Page, PageContent, PageContext, Result, Source, Viewer,
	alloc::{String, Vec, string::ToString},
	helpers::uri::QueryParameters,
	imports::{
		defaults::defaults_get_map,
		html::{Document, Element},
		net::{Request, TimeUnit, set_rate_limit},
		std::{current_date, parse_date},
	},
	prelude::*,
};

const BASE: &str = "https://manga-shi.org";
struct MangaShi;

fn site_path(value: &str) -> Result<String> {
	let path = value.strip_prefix(BASE).unwrap_or(value);
	if !path.starts_with('/')
		|| path.starts_with("//")
		|| path.contains('\\')
		|| path.split('/').any(|part| part == ".." || part == ".")
	{
		bail!("Manga-shi: некорректная ссылка.");
	}
	Ok(path.split('#').next().unwrap_or(path).into())
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

fn image(element: &Element) -> Option<String> {
	element
		.attr("data-src")
		.filter(|value| !value.trim().is_empty())
		.or_else(|| element.attr("src"))
		.and_then(|value| absolute(&value))
}

fn request(path: &str) -> Result<Request> {
	let path = site_path(path)?;
	let mut request = Request::get(format!("{BASE}{path}"))?
		.header("Referer", &format!("{BASE}/"))
		.timeout(30.0);
	if let Some(cookies) = defaults_get_map("login") {
		let cookies: Vec<_> = cookies
			.into_iter()
			.filter(|(name, value)| {
				!name.is_empty()
					&& name
						.bytes()
						.all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
					&& value
						.bytes()
						.all(|b| (0x21..=0x7e).contains(&b) && !b";,\"\\".contains(&b))
			})
			.map(|(name, value)| format!("{name}={value}"))
			.collect();
		if !cookies.is_empty() {
			request.set_header("Cookie", &cookies.join("; "));
		}
	}
	Ok(request)
}

fn document(path: &str) -> Result<Document> {
	let response = request(path)?.send()?;
	match response.status_code() {
		200..=299 => response.get_html().map_err(Into::into),
		401 | 403 => {
			bail!("Manga-shi: откройте вход в настройках источника и проверьте доступ к сайту.")
		}
		status => bail!("Manga-shi: HTTP {status}."),
	}
}

fn catalog_path(query: Option<&str>, page: i32, sort: &str, filters: Vec<FilterValue>) -> String {
	let mut pairs: Vec<(String, String)> = aidoku::alloc::vec![("sort".into(), sort.into())];
	for filter in filters {
		match filter {
			FilterValue::Select { id, value } => {
				pairs.retain(|(key, _)| key != &id);
				pairs.push((id, value));
			}
			FilterValue::MultiSelect {
				id,
				included,
				excluded,
			} if id == "tag" => {
				pairs.extend(included.into_iter().map(|value| ("tag".into(), value)));
				pairs.extend(
					excluded
						.into_iter()
						.map(|value| ("exclude_tag".into(), value)),
				);
			}
			_ => {}
		}
	}
	pairs.push(("q".into(), query.unwrap_or_default().trim().into()));
	pairs.push(("page".into(), page.max(1).to_string()));
	let mut parameters = QueryParameters::new();
	for (key, value) in &pairs {
		parameters.push(key, Some(value));
	}
	format!("/catalog/?{parameters}")
}

fn parse_catalog(document: &Document) -> Result<MangaPageResult> {
	let grid = document
		.select_first("#manga-grid")
		.ok_or_else(|| error!("Manga-shi: каталог не найден, проверьте сайт через WebView."))?;
	let mut entries = Vec::new();
	for element in grid.select("a[href*='/manga/']").into_iter().flatten() {
		let Some(key) = element
			.attr("href")
			.and_then(|value| site_path(&value).ok())
		else {
			continue;
		};
		if entries.iter().any(|manga: &Manga| manga.key == key) {
			continue;
		}
		let Some(title) = element
			.select_first("h3, h4, .title")
			.and_then(|value| value.text())
			.filter(|value| !value.is_empty())
		else {
			continue;
		};
		entries.push(Manga {
			key: key.clone(),
			title,
			url: Some(format!("{BASE}{key}")),
			cover: element
				.select_first("img")
				.and_then(|element| image(&element)),
			..Default::default()
		});
	}
	Ok(MangaPageResult {
		has_next_page: entries.len() >= 20,
		entries,
	})
}

fn parse_details(document: &Document, mut manga: Manga) -> Result<Manga> {
	manga.title = document
		.select_first("h1")
		.and_then(|element| element.text())
		.filter(|value| !value.is_empty())
		.ok_or_else(|| error!("Manga-shi: название не найдено."))?;
	manga.url = Some(format!("{BASE}{}", manga.key));
	manga.cover = document
		.select_first("meta[property='og:image']")
		.and_then(|element| element.attr("content"))
		.and_then(|value| absolute(&value))
		.or(manga.cover);
	manga.description = document
		.select_first("div.leading-relaxed[x-ref*='descDesktop']")
		.and_then(|element| element.text());
	let authors: Vec<_> = document
		.select("a[href*='?author=']")
		.into_iter()
		.flatten()
		.filter_map(|element| element.text())
		.collect();
	if !authors.is_empty() {
		manga.authors = Some(authors);
	}
	let badges: Vec<_> = document
		.select("span.tracking-widest")
		.into_iter()
		.flatten()
		.filter_map(|element| element.text())
		.collect();
	for badge in &badges {
		let value = badge.to_lowercase();
		if value.contains("онгоинг")
			|| value.contains("выпускается")
			|| value.contains("продолжается")
		{
			manga.status = MangaStatus::Ongoing;
		} else if value.contains("заверш") {
			manga.status = MangaStatus::Completed;
		} else if value.contains("заморож")
			|| value.contains("приостановл")
			|| value.contains("хиатус")
		{
			manga.status = MangaStatus::Hiatus;
		} else if value.contains("заброш") {
			manga.status = MangaStatus::Cancelled;
		}
		if value.contains("манхва") || value.contains("маньхуа") {
			manga.viewer = Viewer::Webtoon;
		} else if value.contains("манга") {
			manga.viewer = Viewer::RightToLeft;
		}
	}
	let mut tags = Vec::new();
	for element in document
		.select("a[href*='manga-genre'], a[href*='?tag=']")
		.into_iter()
		.flatten()
	{
		if let Some(value) = element
			.text()
			.map(|value| value.trim_start_matches('#').trim().to_string())
			.filter(|value| !value.is_empty())
			&& !tags.contains(&value)
		{
			tags.push(value);
		}
	}
	if !tags.is_empty() {
		manga.tags = Some(tags);
	}
	Ok(manga)
}

fn chapter_date(value: &str, now: i64) -> Option<i64> {
	let value = value.trim();
	if let Some(date) = parse_date(format!("{value} 00:00:00"), "dd.MM.yyyy HH:mm:ss") {
		return Some(date);
	}
	let lower = value.to_lowercase();
	let amount = lower.split_whitespace().next()?.parse::<i64>().ok()?;
	let seconds = if lower.contains("сек") {
		1
	} else if lower.contains("мин") {
		60
	} else if lower.contains("час") {
		3600
	} else if lower.contains("недел") {
		604800
	} else if lower.contains("дн") || lower.contains("день") || lower.contains("дня") {
		86400
	} else {
		return None;
	};
	amount
		.checked_mul(seconds)
		.and_then(|offset| now.checked_sub(offset))
}

fn parse_chapters(document: &Document, now: i64) -> Vec<Chapter> {
	let mut result = Vec::new();
	for element in document
		.select("a[href*='/glava-'], a[href*='/glava_']")
		.into_iter()
		.flatten()
	{
		let Some(title) = element
			.select_first("span.chapter-title")
			.and_then(|element| element.text())
		else {
			continue;
		};
		let Some(key) = element
			.attr("href")
			.and_then(|value| site_path(&value).ok())
		else {
			continue;
		};
		let number = key
			.rsplit_once("/glava-")
			.or_else(|| key.rsplit_once("/glava_"))
			.and_then(|(_, number)| number.trim_end_matches('/').replace(',', ".").parse().ok());
		let date = element
			.select("span span")
			.into_iter()
			.flatten()
			.filter_map(|element| element.text())
			.filter_map(|value| chapter_date(&value, now))
			.next_back();
		result.push(Chapter {
			key: key.clone(),
			title: Some(title),
			chapter_number: number,
			date_uploaded: date,
			language: Some("ru".into()),
			url: Some(format!("{BASE}{key}")),
			..Default::default()
		});
	}
	result
}

fn parse_pages(document: &Document, referer: &str) -> Result<Vec<Page>> {
	let pages: Vec<_> = document
		.select("img.reader-image")
		.into_iter()
		.flatten()
		.filter_map(|element| image(&element))
		.map(|url| {
			let mut context = PageContext::new();
			context.insert("referer".into(), referer.into());
			Page {
				content: PageContent::url_context(url, context),
				..Default::default()
			}
		})
		.collect();
	if pages.is_empty() {
		bail!("Manga-shi: страницы не найдены. Проверьте доступ к главе на сайте.");
	}
	Ok(pages)
}

impl Source for MangaShi {
	fn new() -> Self {
		set_rate_limit(2, 1, TimeUnit::Seconds);
		Self
	}
	fn get_search_manga_list(
		&self,
		query: Option<String>,
		page: i32,
		filters: Vec<FilterValue>,
	) -> Result<MangaPageResult> {
		parse_catalog(&document(&catalog_path(
			query.as_deref(),
			page,
			"popular",
			filters,
		))?)
	}
	fn get_manga_update(
		&self,
		mut manga: Manga,
		needs_details: bool,
		needs_chapters: bool,
	) -> Result<Manga> {
		if !needs_details && !needs_chapters {
			return Ok(manga);
		}
		manga.key = site_path(&manga.key)?;
		let doc = document(&manga.key)?;
		if needs_details {
			manga = parse_details(&doc, manga)?;
		}
		if needs_chapters {
			let now = current_date();
			let mut chapters = parse_chapters(&doc, now);
			let mut next = doc
				.select_first("#chapters-load-more a[hx-get]")
				.and_then(|element| element.attr("hx-get"));
			let mut visited = Vec::new();
			while let Some(url) = next {
				let path = site_path(&url)?;
				if visited.contains(&path) {
					bail!("Manga-shi: сайт повторил страницу списка глав.");
				}
				visited.push(path.clone());
				let fragment = document(&path)?;
				for chapter in parse_chapters(&fragment, now) {
					if !chapters.iter().any(|existing| existing.key == chapter.key) {
						chapters.push(chapter);
					}
				}
				next = fragment
					.select("nav.flex a[href]")
					.into_iter()
					.flatten()
					.filter(|element| element.text().is_some_and(|value| value.contains("Дальше")))
					.filter_map(|element| element.attr("href"))
					.next_back()
					.map(|value| {
						if value.starts_with('?') {
							format!("{}{}", path.split('?').next().unwrap_or(&path), value)
						} else {
							value
						}
					});
			}
			manga.chapters = Some(chapters);
		}
		Ok(manga)
	}
	fn get_page_list(&self, _manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let path = site_path(&chapter.key)?;
		parse_pages(&document(&path)?, &format!("{BASE}{path}"))
	}
}

impl ListingProvider for MangaShi {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		let sort = match listing.id.as_str() {
			"popular" => "popular",
			"latest" => "updated",
			_ => bail!("Неизвестный каталог."),
		};
		parse_catalog(&document(&catalog_path(None, page, sort, Vec::new()))?)
	}
}
impl DynamicFilters for MangaShi {
	fn get_dynamic_filters(&self) -> Result<Vec<Filter>> {
		let document = document("/catalog/")?;
		let mut ids = Vec::new();
		let mut options = Vec::new();
		for element in document
			.select(".genre-flyout .group")
			.into_iter()
			.flatten()
		{
			if let (Some(id), Some(title)) = (
				element
					.select_first("input")
					.and_then(|value| value.attr("value")),
				element.select_first("span").and_then(|value| value.text()),
			) {
				ids.push(id.into());
				options.push(title.into());
			}
		}
		Ok(aidoku::alloc::vec![
			MultiSelectFilter {
				id: "tag".into(),
				title: Some("Жанры".into()),
				options,
				ids: Some(ids),
				can_exclude: true,
				..Default::default()
			}
			.into()
		])
	}
}
impl ImageRequestProvider for MangaShi {
	fn get_image_request(&self, url: String, context: Option<PageContext>) -> Result<Request> {
		let referer = context
			.as_ref()
			.and_then(|context| context.get("referer"))
			.and_then(|value| site_path(value).ok())
			.map(|path| format!("{BASE}{path}"))
			.unwrap_or_else(|| format!("{BASE}/"));
		let request = if url
			.strip_prefix(BASE)
			.is_some_and(|path| path.starts_with('/'))
		{
			request(&url)?
		} else {
			Request::get(url)?
		};
		Ok(request.header("Referer", &referer))
	}
}
impl DeepLinkHandler for MangaShi {
	fn handle_deep_link(&self, url: String) -> Result<Option<DeepLinkResult>> {
		let Ok(path) = site_path(&url) else {
			return Ok(None);
		};
		let path = path.split('?').next().unwrap_or(&path);
		let parts: Vec<_> = path.trim_matches('/').split('/').collect();
		if parts.len() == 2 && parts[0] == "manga" && !parts[1].is_empty() {
			Ok(Some(DeepLinkResult::Manga {
				key: format!("/manga/{}/", parts[1]),
			}))
		} else {
			Ok(None)
		}
	}
}
register_source!(
	MangaShi,
	ListingProvider,
	DynamicFilters,
	ImageRequestProvider,
	DeepLinkHandler
);
#[cfg(test)]
mod tests;
