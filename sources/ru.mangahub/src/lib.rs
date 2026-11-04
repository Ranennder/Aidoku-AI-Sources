// Protocol and selectors adapted from Keiyoushi's Russian Mangahub (Apache-2.0).
#![no_std]
use aidoku::{
	Chapter, DeepLinkHandler, DeepLinkResult, DynamicFilters, Filter, FilterValue,
	ImageRequestProvider, Listing, ListingProvider, Manga, MangaPageResult, MangaStatus,
	MultiSelectFilter, Page, PageContent, PageContext, Result, Source,
	alloc::{String, Vec, string::ToString},
	helpers::uri::QueryParameters,
	imports::{
		defaults::defaults_get_map,
		html::Document,
		net::{Request, TimeUnit, set_rate_limit},
		std::parse_date,
	},
	prelude::*,
};
const BASE: &str = "https://mangahub.ru";
struct MangaHub;

fn site_path(value: &str) -> Result<String> {
	let path = value.strip_prefix(BASE).unwrap_or(value);
	if !path.starts_with('/')
		|| path.starts_with("//")
		|| path.contains('\\')
		|| path.split('/').any(|part| part == "." || part == "..")
	{
		bail!("MangaHub: некорректная ссылка.");
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
fn request(path: &str) -> Result<Request> {
	let path = site_path(path)?;
	let mut request = Request::get(format!("{BASE}{path}"))?
		.header("Referer", &format!("{BASE}/"))
		.timeout(30.0);
	let mut cookies = aidoku::alloc::vec![String::from("confirm_age=1")];
	if let Some(saved) = defaults_get_map("login") {
		cookies.extend(
			saved
				.into_iter()
				.filter(|(name, value)| {
					name != "confirm_age"
						&& !name.is_empty()
						&& name
							.bytes()
							.all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
						&& value
							.bytes()
							.all(|b| (0x21..=0x7e).contains(&b) && !b";,\"\\".contains(&b))
				})
				.map(|(name, value)| format!("{name}={value}")),
		);
	}
	request.set_header("Cookie", &cookies.join("; "));
	Ok(request)
}
fn document(path: &str) -> Result<Document> {
	let response = request(path)?.send()?;
	match response.status_code() {
		200..=299 => response.get_html().map_err(Into::into),
		401 | 403 => {
			bail!("MangaHub: откройте вход в настройках источника и проверьте доступ к сайту.")
		}
		status => bail!("MangaHub: HTTP {status}."),
	}
}

const FILTER_ORDER: [(&str, &str); 11] = [
	("type", "type"),
	("formats", "formats"),
	("genres", "genres"),
	("status", "status"),
	("translation", "translation"),
	("age", "age"),
	("country", "country"),
	("tags", "tags"),
	("year", "year"),
	("items", "items"),
	("rating", "rating"),
];
fn catalog_path(query: Option<&str>, page: i32, sort: &str, filters: Vec<FilterValue>) -> String {
	let query = query.unwrap_or_default().trim();
	let mut parameters = QueryParameters::new();
	let page = page.max(1).to_string();
	if page != "1" {
		parameters.push("page", Some(&page));
	}
	if !query.is_empty() {
		parameters.push("query", Some(query));
		return format!("/search/title?{parameters}");
	}
	let mut segments: Vec<(String, String)> = Vec::new();
	let mut sort = sort.to_string();
	for filter in filters {
		match filter {
			FilterValue::Select { id, value } if id == "sort" => sort = value,
			FilterValue::MultiSelect {
				id,
				mut included,
				mut excluded,
			} => {
				included.sort();
				excluded.sort();
				// Site requires this exact sorted order inside a path filter.
				if included.iter().chain(&excluded).any(|id| {
					!id.bytes()
						.all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
				}) {
					continue;
				}
				let mut values = Vec::new();
				if !included.is_empty() {
					values.push(included.join("-or-"));
				}
				if !excluded.is_empty() {
					values.push(format!("nor-{}", excluded.join("-nor-")));
				}
				if !values.is_empty() {
					segments.push((id, format!("is-{}", values.join("-"))));
				}
			}
			FilterValue::Range { id, from, to } => {
				let format = |value: f32| {
					if id == "rating" {
						format!("{:.1}", value.clamp(0.0, 10.0))
					} else {
						format!("{}", value.max(0.0) as i64)
					}
				};
				let value = match (from, to) {
					(Some(from), Some(to)) => format!("from-{}-to-{}", format(from), format(to)),
					(Some(from), None) => format!("from-{}", format(from)),
					(None, Some(to)) => format!("to-{}", format(to)),
					_ => continue,
				};
				segments.push((id, value));
			}
			_ => {}
		}
	}
	let mut path = String::from("/explore");
	for (id, prefix) in FILTER_ORDER {
		if let Some((_, value)) = segments.iter().find(|(key, _)| key == id) {
			path.push_str(&format!("/{prefix}-{value}"));
		}
	}
	path.push_str(&format!("/sort-is-{sort}"));
	if !parameters.is_empty() {
		path.push_str(&format!("?{parameters}"));
	}
	path
}

fn parse_catalog(document: &Document) -> Result<MangaPageResult> {
	// The explore/search form remains present when there are no matching titles.
	if document
		.select_first(".item-grid, .pagination, #filter_genres, .search-results, .explore-filters")
		.is_none()
	{
		bail!("MangaHub: каталог не найден, проверьте доступ к сайту через WebView.");
	}
	let entries = document
		.select("div.item-grid")
		.into_iter()
		.flatten()
		.filter_map(|element| {
			let link = element.select_first("a.fw-medium")?;
			let key = site_path(&link.attr("href")?).ok()?;
			Some(Manga {
				key: key.clone(),
				title: link.text()?,
				cover: element
					.select_first("img.item-grid-image")
					.and_then(|element| element.attr("src"))
					.and_then(|value| absolute(&value)),
				url: Some(format!("{BASE}{key}")),
				..Default::default()
			})
		})
		.collect();
	Ok(MangaPageResult {
		entries,
		has_next_page: document
			.select_first(".pagination .page-item:last-child a[href]")
			.is_some(),
	})
}
fn attribute(document: &Document, names: &[&str]) -> Vec<String> {
	let mut values = Vec::new();
	for element in document.select(".attr-name").into_iter().flatten() {
		if element
			.text()
			.is_some_and(|value| names.iter().any(|name| value.contains(name)))
			&& let Some(value) = element.next()
		{
			let links: Vec<_> = value
				.select("a")
				.into_iter()
				.flatten()
				.filter_map(|element| element.text())
				.collect();
			if links.is_empty() {
				if let Some(text) = value.text() {
					values.push(text);
				}
			} else {
				values.extend(links);
			}
		}
	}
	values
}
fn parse_details(document: &Document, mut manga: Manga) -> Result<Manga> {
	manga.title = document
		.select_first(".detail-panel h1")
		.and_then(|element| element.text())
		.filter(|value| !value.is_empty())
		.ok_or_else(|| error!("MangaHub: название не найдено."))?;
	manga.cover = document
		.select_first("img.cover-detail")
		.and_then(|element| element.attr("src"))
		.and_then(|value| absolute(&value))
		.or(manga.cover);
	manga.description = document
		.select_first(".markdown-style.text-expandable-content")
		.and_then(|element| element.text());
	let authors = attribute(document, &["Автор", "Сценарист"]);
	if !authors.is_empty() {
		manga.authors = Some(authors);
	}
	let artists = attribute(document, &["Художник"]);
	if !artists.is_empty() {
		manga.artists = Some(artists);
	}
	let tags: Vec<_> = document
		.select(".tags a")
		.into_iter()
		.flatten()
		.filter_map(|element| element.text())
		.map(|value| value.trim_start_matches('#').to_string())
		.collect();
	if !tags.is_empty() {
		manga.tags = Some(tags);
	}
	let status = attribute(document, &["Томов"]).join(" ").to_lowercase();
	manga.status = if status.contains("продолжается") {
		MangaStatus::Ongoing
	} else if status.contains("приостановлен") {
		MangaStatus::Hiatus
	} else if status.contains("заверш") || status.contains("выпуск прекращён")
	{
		MangaStatus::Completed
	} else {
		MangaStatus::Unknown
	};
	manga.url = Some(format!("{BASE}{}", manga.key));
	Ok(manga)
}
fn parse_chapters(document: &Document) -> Vec<Chapter> {
	document
		.select(".detail-items .detail-item")
		.into_iter()
		.flatten()
		.filter_map(|element| {
			let link = element.select_first("div.align-items-center > a")?;
			let title = link.text()?;
			let key = site_path(&link.attr("href")?).ok()?;
			let number = title
				.split_once("Глава")
				.and_then(|(_, value)| value.split_whitespace().next())
				.and_then(|value| value.parse().ok());
			let date = element
				.select_first("div.text-muted")
				.and_then(|element| element.text())
				.and_then(|value| {
					parse_date(format!("{} 00:00:00", value.trim()), "dd.MM.yyyy HH:mm:ss")
				});
			Some(Chapter {
				key: key.clone(),
				title: Some(title),
				chapter_number: number,
				date_uploaded: date,
				language: Some("ru".into()),
				url: Some(format!("{BASE}{key}")),
				..Default::default()
			})
		})
		.collect()
}
fn parse_pages(document: &Document, referer: &str) -> Result<Vec<Page>> {
	let pages: Vec<_> = document
		.select("img.reader-viewer-img")
		.into_iter()
		.flatten()
		.filter_map(|element| {
			element
				.attr("data-src")
				.filter(|value| !value.trim().is_empty())
				.or_else(|| element.attr("src"))
		})
		.filter_map(|value| absolute(&value))
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
		bail!("MangaHub: страницы главы недоступны, проверьте вход на сайте.");
	}
	Ok(pages)
}
impl Source for MangaHub {
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
			"rating",
			filters,
		))?)
	}
	fn get_manga_update(
		&self,
		mut manga: Manga,
		needs_details: bool,
		needs_chapters: bool,
	) -> Result<Manga> {
		manga.key = site_path(&manga.key)?;
		if needs_details {
			manga = parse_details(&document(&manga.key)?, manga)?;
		}
		if needs_chapters {
			manga.chapters = Some(parse_chapters(&document(&format!(
				"{}/chapters",
				manga.key.trim_end_matches('/')
			))?));
		}
		Ok(manga)
	}
	fn get_page_list(&self, _manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let path = site_path(&chapter.key)?;
		parse_pages(&document(&path)?, &format!("{BASE}{path}"))
	}
}
impl ListingProvider for MangaHub {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		let sort = match listing.id.as_str() {
			"popular" => "rating",
			"latest" => "update",
			_ => bail!("Неизвестный каталог."),
		};
		parse_catalog(&document(&catalog_path(None, page, sort, Vec::new()))?)
	}
}
impl DynamicFilters for MangaHub {
	fn get_dynamic_filters(&self) -> Result<Vec<Filter>> {
		let document = document("/explore")?;
		let mut filters = Vec::new();
		for (id, html_id, title) in [
			("genres", "genres", "Жанры"),
			("tags", "tags", "Теги"),
			("type", "type", "Тип"),
			("status", "status", "Статус"),
			("translation", "statusTranslation", "Перевод"),
			("formats", "formats", "Формат"),
			("age", "ageRating", "Возраст"),
			("country", "country", "Страна"),
		] {
			let mut ids = Vec::new();
			let mut options = Vec::new();
			for element in document
				.select(format!("#filter_{html_id} .filter-item"))
				.into_iter()
				.flatten()
			{
				if let (Some(id), Some(title)) = (
					element
						.select_first("input")
						.and_then(|element| element.attr("value")),
					element
						.select_first("label")
						.and_then(|element| element.text()),
				) {
					ids.push(id.into());
					options.push(title.into());
				}
			}
			if !ids.is_empty() {
				filters.push(
					MultiSelectFilter {
						id: id.into(),
						title: Some(title.into()),
						options,
						ids: Some(ids),
						can_exclude: true,
						..Default::default()
					}
					.into(),
				);
			}
		}
		Ok(filters)
	}
}
impl ImageRequestProvider for MangaHub {
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
impl DeepLinkHandler for MangaHub {
	fn handle_deep_link(&self, url: String) -> Result<Option<DeepLinkResult>> {
		let Ok(path) = site_path(&url) else {
			return Ok(None);
		};
		let path = path.split('?').next().unwrap_or(&path);
		let parts: Vec<_> = path.trim_matches('/').split('/').collect();
		if parts.len() == 2 && parts[0] == "title" && !parts[1].is_empty() {
			Ok(Some(DeepLinkResult::Manga {
				key: format!("/title/{}", parts[1]),
			}))
		} else {
			Ok(None)
		}
	}
}
register_source!(
	MangaHub,
	ListingProvider,
	DynamicFilters,
	ImageRequestProvider,
	DeepLinkHandler
);
#[cfg(test)]
mod tests;
