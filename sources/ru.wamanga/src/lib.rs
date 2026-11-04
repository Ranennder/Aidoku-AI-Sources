// Protocol adapted from Keiyoushi WaManga; pinned attribution in UPSTREAM.md.
#![no_std]
use aidoku::{
	Chapter, DeepLinkHandler, DeepLinkResult, FilterValue, ImageRequestProvider, Listing,
	ListingProvider, Manga, MangaPageResult, MangaStatus, Page, PageContent, PageContext, Result,
	Source,
	alloc::{String, Vec, string::ToString},
	helpers::uri::QueryParameters,
	imports::net::Request,
	prelude::*,
};
use serde::Deserialize;
use serde_json::{Map, Value};
const BASE: &str = "https://wamanga.ru";
const INVALIDATED: &str = "x-sveltekit-invalidated=001";

fn key(value: &str) -> Result<String> {
	let value = value
		.strip_prefix(BASE)
		.unwrap_or(value)
		.trim_start_matches('/');
	let value = value
		.split(['?', '#'])
		.next()
		.unwrap_or_default()
		.trim_end_matches('/');
	let parts: Vec<_> = value.split('/').collect();
	if parts.len() != 2
		|| !["manga", "manhwa", "manhua", "comic", "manuscript"].contains(&parts[0])
		|| parts
			.iter()
			.any(|p| p.is_empty() || *p == "." || *p == ".." || p.contains([':', '\\', '%']))
	{
		bail!("WaManga: некорректная ссылка на произведение.");
	}
	Ok(value.into())
}
fn absolute(value: &str) -> Result<String> {
	if value.starts_with("https://") || value.starts_with("http://") {
		return Ok(value.into());
	}
	if value.is_empty() || value.starts_with("//") || value.contains([':', '\\']) {
		bail!("WaManga: некорректный URL изображения.");
	}
	Ok(format!("{BASE}/{}", value.trim_start_matches('/')))
}
fn position(value: &Value) -> Result<String> {
	let string = value
		.as_str()
		.map(String::from)
		.unwrap_or_else(|| value.to_string());
	if string.is_empty()
		|| !string.chars().all(|c| c.is_ascii_digit() || c == '.')
		|| string
			.parse::<f64>()
			.ok()
			.filter(|n| n.is_finite())
			.is_none()
	{
		bail!("WaManga: некорректный номер главы или страницы.");
	}
	Ok(string)
}

/// A bounded devalue expansion. Integers inside pool objects/arrays are refs;
/// primitive values stored at those indices remain primitive values.
fn resolve(pool: &[Value], value: &Value, stack: &mut Vec<usize>, depth: usize) -> Result<Value> {
	if depth > 96 {
		bail!("WaManga: вложенность данных слишком велика.");
	}
	match value {
		Value::Object(object) => {
			let mut result = Map::new();
			for (key, value) in object {
				result.insert(key.clone(), reference(pool, value, stack, depth + 1)?);
			}
			Ok(Value::Object(result))
		}
		Value::Array(values) => values
			.iter()
			.map(|v| reference(pool, v, stack, depth + 1))
			.collect::<Result<Vec<_>>>()
			.map(Value::Array),
		_ => Ok(value.clone()),
	}
}
fn reference(pool: &[Value], value: &Value, stack: &mut Vec<usize>, depth: usize) -> Result<Value> {
	let Some(index) = value.as_i64() else {
		return resolve(pool, value, stack, depth);
	};
	if index < 0 {
		return Ok(Value::Null);
	}
	let index = index as usize;
	if stack.contains(&index) {
		bail!("WaManga: циклические данные страницы.");
	}
	let value = pool
		.get(index)
		.ok_or_else(|| error!("WaManga: ссылка за пределами данных страницы."))?;
	stack.push(index);
	let result = resolve(pool, value, stack, depth);
	stack.pop();
	result
}
fn decode(raw: &str) -> Result<Value> {
	let first = raw
		.lines()
		.find(|line| !line.trim().is_empty())
		.ok_or_else(|| error!("WaManga: пустой ответ."))?;
	let envelope: Value = serde_json::from_str(first)?;
	let node = envelope
		.get("nodes")
		.and_then(Value::as_array)
		.and_then(|nodes| {
			nodes
				.iter()
				.rev()
				.find(|node| node.get("type").and_then(Value::as_str) == Some("data"))
		})
		.ok_or_else(|| {
			error!("WaManga: данные страницы не найдены; проверьте доступ через WebView.")
		})?;
	let pool = node
		.get("data")
		.and_then(Value::as_array)
		.filter(|p| !p.is_empty())
		.ok_or_else(|| error!("WaManga: отсутствуют данные страницы."))?;
	resolve(pool, &pool[0], &mut aidoku::alloc::vec![0], 0)
}
fn fetch<T: serde::de::DeserializeOwned>(url: &str) -> Result<T> {
	let response = Request::get(url)?
		.header("Referer", &format!("{BASE}/"))
		.header("X-Requested-With", "XMLHttpRequest")
		.timeout(30.0)
		.send()?;
	if !(200..300).contains(&response.status_code()) {
		bail!(
			"WaManga HTTP {}. Проверьте доступ через WebView источника.",
			response.status_code()
		);
	}
	let raw = String::from_utf8(response.get_data()?)
		.map_err(|_| error!("WaManga: ответ не в UTF-8."))?;
	Ok(serde_json::from_value(decode(&raw)?)?)
}

fn catalog_url(query: &str, page: i32, default_sort: &str, filters: Vec<FilterValue>) -> String {
	let mut params = QueryParameters::new();
	params.push(
		"offset",
		Some(&((i64::from(page.max(1)) - 1) * 24).to_string()),
	);
	params.push("limit", Some("24"));
	params.push("x-sveltekit-invalidated", Some("001"));
	params.push("sortKey", Some(default_sort));
	params.push("sortDescending", Some("true"));
	if !query.trim().is_empty() {
		params.push("query", Some(query.trim()));
	}
	for filter in filters {
		match filter {
			FilterValue::Sort {
				id,
				index,
				ascending,
			} if id == "sort" => {
				let keys = ["updatedAt", "likes", "createdAt", "views", "alphabetical"];
				let sort = keys
					.get(index.max(0) as usize)
					.copied()
					.unwrap_or("updatedAt");
				params.set("sortKey", Some(sort));
				params.set(
					"sortDescending",
					Some(if ascending { "false" } else { "true" }),
				);
			}
			FilterValue::MultiSelect { id, included, .. }
				if ["types", "statuses", "translationStatuses", "pegiRatings"]
					.contains(&id.as_str()) =>
			{
				for value in included {
					params.push(&id, Some(&value));
				}
			}
			FilterValue::Text { id, value } if id == "releaseYearFrom" || id == "releaseYearTo" => {
				if value.trim().len() == 4
					&& let Ok(year) = value.trim().parse::<i32>()
				{
					params.push(&id, Some(&year.clamp(1990, 2100).to_string()));
				}
			}
			_ => {}
		}
	}
	format!("{BASE}/catalog/__data.json?{params}")
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Record {
	slug: String,
	title: String,
	#[serde(rename = "type")]
	kind: String,
	#[serde(default)]
	cover_url: Option<String>,
	#[serde(default)]
	description: Option<String>,
	#[serde(default)]
	authors: Option<Vec<String>>,
	#[serde(default)]
	artists: Option<Vec<String>>,
	#[serde(default)]
	genres: Vec<String>,
	#[serde(default)]
	status_title: Option<String>,
	#[serde(default)]
	chapters: Vec<ChapterData>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChapterData {
	position: Value,
	#[serde(default)]
	created_at: Option<String>,
}
impl Record {
	fn into_manga(self, details: bool, chapters: bool, mut original: Manga) -> Result<Manga> {
		let canonical = key(&format!("{}/{}", self.kind, self.slug))?;
		if details {
			original.key = canonical.clone();
			original.url = Some(format!("{BASE}/{canonical}"));
			original.title = self.title;
			original.cover = self
				.cover_url
				.filter(|url| !url.is_empty())
				.map(|url| absolute(&url))
				.transpose()?;
			original.description = self.description;
			original.tags = Some(self.genres);
			original.authors = self.authors.map(|items| {
				items
					.into_iter()
					.filter(|name| !name.trim().is_empty() && name != "N/A")
					.collect()
			});
			original.artists = self.artists.map(|items| {
				items
					.into_iter()
					.filter(|name| !name.trim().is_empty() && name != "N/A")
					.collect()
			});
			original.status = match self
				.status_title
				.as_deref()
				.map(str::to_lowercase)
				.as_deref()
			{
				Some("ongoing") => MangaStatus::Ongoing,
				Some("completed") => MangaStatus::Completed,
				Some("paused" | "on hiatus" | "hiatus") => MangaStatus::Hiatus,
				Some("discontinued" | "cancelled" | "abandoned") => MangaStatus::Cancelled,
				_ => MangaStatus::Unknown,
			};
		}
		if chapters {
			original.chapters = Some(
				self.chapters
					.into_iter()
					.map(|chapter| {
						let number = position(&chapter.position)?;
						Ok(Chapter {
							key: format!("{canonical}/{number}"),
							url: Some(format!("{BASE}/{canonical}/glava-{number}")),
							title: Some(format!("Глава {number}")),
							chapter_number: number.parse().ok(),
							language: Some("ru".into()),
							date_uploaded: chapter
								.created_at
								.and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
								.map(|date| date.timestamp()),
							..Default::default()
						})
					})
					.collect::<Result<_>>()?,
			);
		}
		Ok(original)
	}
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Catalog {
	initial_mangas: Vec<Record>,
}
fn parse_catalog(data: Catalog) -> Result<MangaPageResult> {
	let has_next_page = data.initial_mangas.len() >= 24;
	Ok(MangaPageResult {
		entries: data
			.initial_mangas
			.into_iter()
			.map(|record| record.into_manga(true, false, Manga::default()))
			.collect::<Result<_>>()?,
		has_next_page,
	})
}
#[derive(Deserialize)]
struct Details {
	manga: Record,
}
#[derive(Deserialize)]
struct Pages {
	chapter: Files,
}
#[derive(Deserialize)]
struct Files {
	files: Vec<File>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct File {
	disk_file: String,
	position: Value,
}
fn parse_pages(data: Pages, referer: &str) -> Result<Vec<Page>> {
	let mut files: Vec<_> = data
		.chapter
		.files
		.into_iter()
		.map(|file| {
			Ok((
				position(&file.position)?
					.parse::<f64>()
					.map_err(|_| error!("WaManga: некорректный порядок страниц."))?,
				file.disk_file,
			))
		})
		.collect::<Result<_>>()?;
	files.sort_by(|left, right| left.0.total_cmp(&right.0));
	if files.is_empty() {
		bail!("WaManga: у главы нет доступных страниц.");
	}
	files
		.into_iter()
		.map(|(_, file)| {
			let mut context = PageContext::new();
			context.insert("referer".into(), referer.into());
			Ok(Page {
				content: PageContent::url_context(absolute(&file)?, context),
				..Default::default()
			})
		})
		.collect()
}

struct WaManga;
impl Source for WaManga {
	fn new() -> Self {
		Self
	}
	fn get_search_manga_list(
		&self,
		query: Option<String>,
		page: i32,
		filters: Vec<FilterValue>,
	) -> Result<MangaPageResult> {
		parse_catalog(fetch(&catalog_url(
			query.as_deref().unwrap_or_default(),
			page,
			"updatedAt",
			filters,
		))?)
	}
	fn get_manga_update(&self, manga: Manga, details: bool, chapters: bool) -> Result<Manga> {
		if !details && !chapters {
			return Ok(manga);
		}
		let canonical = key(&manga.key)?;
		let response: Details = fetch(&format!("{BASE}/{canonical}/__data.json?{INVALIDATED}"))?;
		response.manga.into_manga(details, chapters, manga)
	}
	fn get_page_list(&self, _manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let (manga, number) = chapter
			.key
			.rsplit_once('/')
			.ok_or_else(|| error!("WaManga: некорректная глава."))?;
		let manga = key(manga)?;
		let number = position(&Value::String(number.into()))?;
		let referer = format!("{BASE}/{manga}/glava-{number}");
		parse_pages(
			fetch(&format!("{referer}/__data.json?{INVALIDATED}"))?,
			&referer,
		)
	}
}
impl ListingProvider for WaManga {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		let sort = match listing.id.as_str() {
			"popular" => "likes",
			"latest" => "updatedAt",
			_ => bail!("WaManga: неизвестный каталог."),
		};
		parse_catalog(fetch(&catalog_url("", page, sort, Vec::new()))?)
	}
}
impl ImageRequestProvider for WaManga {
	fn get_image_request(&self, url: String, context: Option<PageContext>) -> Result<Request> {
		let referer = context
			.as_ref()
			.and_then(|c| c.get("referer"))
			.filter(|value| value.starts_with(&format!("{BASE}/")))
			.map(String::as_str)
			.unwrap_or(BASE);
		Ok(Request::get(url)?.header("Referer", referer))
	}
}
impl DeepLinkHandler for WaManga {
	fn handle_deep_link(&self, url: String) -> Result<Option<DeepLinkResult>> {
		Ok(key(&url).ok().map(|key| DeepLinkResult::Manga { key }))
	}
}
register_source!(
	WaManga,
	ListingProvider,
	ImageRequestProvider,
	DeepLinkHandler
);
#[cfg(test)]
mod tests;
