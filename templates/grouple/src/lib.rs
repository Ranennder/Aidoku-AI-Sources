// GroupLe protocol adapted from Keiyoushi, Apache-2.0; see source UPSTREAM-NOTICE.
#![no_std]

use aidoku::{
	Chapter, ContentRating, DeepLinkHandler, DeepLinkResult, FilterValue, ImageRequestProvider,
	Listing, ListingProvider, Manga, MangaPageResult, MangaStatus, Page, PageContent, PageContext,
	Result, Source,
	alloc::{String, Vec, vec},
	helpers::uri::encode_uri_component,
	imports::{
		defaults::defaults_get_map,
		html::Document,
		net::{Request, TimeUnit, set_rate_limit},
		std::parse_date,
	},
	prelude::*,
};
use core::marker::PhantomData;
use serde::Deserialize;

macro_rules! ensure {
	($condition:expr, $($message:tt)*) => {
		if !$condition { bail!($($message)*); }
	};
}

pub trait Site {
	const URL: &'static str;
	const ADULT: bool;
}

pub struct GroupLe<S: Site>(PhantomData<S>);

fn site_path(url: &str, base: &str) -> Result<String> {
	let path = url.strip_prefix(base).unwrap_or(url);
	ensure!(
		path.starts_with('/') && !path.starts_with("//") && !path.contains(['\r', '\n', '\\']),
		"Некорректный адрес источника."
	);
	Ok(path.split('#').next().unwrap_or(path).into())
}

fn absolute(url: &str, base: &str) -> Option<String> {
	let url = url.trim();
	if url.starts_with("https://") || url.starts_with("http://") {
		Some(url.into())
	} else if url.starts_with("//") {
		Some(format!("https:{url}"))
	} else if url.starts_with('/') {
		Some(format!("{base}{url}"))
	} else {
		None
	}
}

fn chapter_path(url: &str, base: &str) -> Result<String> {
	let path = site_path(url, base)?;
	Ok(path.split('?').next().unwrap_or(&path).into())
}

fn reader_path(path: &str, hash: Option<&str>) -> String {
	let path = path.split('?').next().unwrap_or(path);
	match hash {
		Some(hash) => format!("{path}?mtr=true&d={hash}"),
		None => format!("{path}?mtr=true"),
	}
}

fn login_cookie() -> Option<String> {
	let mut entries: Vec<_> = defaults_get_map("login")?
		.into_iter()
		.filter(|(name, value)| {
			matches!(
				name.as_str(),
				"remember_me" | "JSESSIONID" | "SESSION" | "cf_clearance"
			) && !value.is_empty()
				&& value.bytes().all(|byte| {
					(0x21..=0x7e).contains(&byte) && !matches!(byte, b';' | b',' | b'"' | b'\\')
				})
		})
		.collect();
	entries.sort_by(|a, b| a.0.cmp(&b.0));
	(!entries.is_empty()).then(|| {
		entries
			.into_iter()
			.map(|(k, v)| format!("{k}={v}"))
			.collect::<Vec<_>>()
			.join("; ")
	})
}

impl<S: Site> GroupLe<S> {
	fn request(&self, url: &str) -> Result<Request> {
		let mut request = Request::get(url)?
			.header("Referer", &format!("{}/", S::URL))
			.timeout(30.0);
		if url
			.strip_prefix(S::URL)
			.is_some_and(|path| path.is_empty() || path.starts_with('/'))
			&& let Some(cookie) = login_cookie()
		{
			request.set_header("Cookie", &cookie);
		}
		Ok(request)
	}

	fn document(&self, path: &str) -> Result<Document> {
		let response = self
			.request(&format!("{}{}", S::URL, site_path(path, S::URL)?))?
			.send()?;
		ensure!(
			response.status_code() != 404,
			"Страница недоступна. Если требуется вход, авторизуйтесь в настройках источника и обновите главы."
		);
		ensure!(
			(200..300).contains(&response.status_code()),
			"Ошибка сайта: HTTP {}",
			response.status_code()
		);
		if let Some(url) = response.get_url() {
			site_path(&url, S::URL)?;
		}
		Ok(response.get_html()?)
	}

	fn catalog(&self, query: Option<&str>, sort: &str, page: i32) -> Result<MangaPageResult> {
		let offset = i64::from(page.max(1) - 1) * 50;
		let mut url = format!(
			"{}/api/catalog/search?offset={offset}&sortType={sort}",
			S::URL
		);
		if let Some(query) = query.filter(|q| !q.trim().is_empty()) {
			url.push_str(&format!("&q={}", encode_uri_component(query.trim())));
		}
		let response = self
			.request(&url)?
			.header("Accept", "application/json, text/plain, */*")
			.send()?;
		ensure!(
			(200..300).contains(&response.status_code()),
			"Ошибка каталога: HTTP {}",
			response.status_code()
		);
		let result: Catalog = response.get_json_owned()?;
		Ok(result.into_page(S::URL, S::ADULT))
	}
}

impl<S: Site> Source for GroupLe<S> {
	fn new() -> Self {
		set_rate_limit(2, 1, TimeUnit::Seconds);
		Self(PhantomData)
	}
	fn get_search_manga_list(
		&self,
		query: Option<String>,
		page: i32,
		_filters: Vec<FilterValue>,
	) -> Result<MangaPageResult> {
		self.catalog(query.as_deref(), "RATING", page)
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
		let path = site_path(&manga.key, S::URL)?;
		let document = self.document(&path)?;
		if needs_details {
			manga = parse_details(&document, manga, S::URL, S::ADULT)?;
		}
		if needs_chapters {
			manga.chapters = Some(parse_chapters(&document, S::URL)?);
		}
		Ok(manga)
	}
	fn get_page_list(&self, manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let path = chapter_path(&chapter.key, S::URL)?;
		// The website requires the current user's hash for restricted chapters.
		// Refresh it after login/account changes without changing chapter identity
		// or persisting an account's hash in chapter keys and saved chapter URLs.
		let hash = if login_cookie().is_some() {
			user_hash(&self.document(&manga.key)?)
		} else {
			None
		};
		let reader = reader_path(&path, hash.as_deref());
		parse_pages(
			&self.document(&reader)?,
			S::URL,
			&format!("{}{reader}", S::URL),
		)
	}
}

impl<S: Site> ListingProvider for GroupLe<S> {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		self.catalog(
			None,
			match listing.id.as_str() {
				"popular" => "RATING",
				"latest" => "DATE_UPDATE",
				_ => bail!("Неизвестный каталог."),
			},
			page,
		)
	}
}

impl<S: Site> ImageRequestProvider for GroupLe<S> {
	fn get_image_request(&self, url: String, context: Option<PageContext>) -> Result<Request> {
		let referer = context
			.and_then(|mut c| c.remove("referer"))
			.filter(|s| site_path(s, S::URL).is_ok())
			.unwrap_or_else(|| format!("{}/", S::URL));
		Ok(self.request(&url)?.header("Referer", &referer))
	}
}

impl<S: Site> DeepLinkHandler for GroupLe<S> {
	fn handle_deep_link(&self, url: String) -> Result<Option<DeepLinkResult>> {
		let Ok(key) = site_path(&url, S::URL) else {
			return Ok(None);
		};
		let clean = key.split('?').next().unwrap_or(&key);
		let parts: Vec<_> = clean.trim_matches('/').split('/').collect();
		if parts.len() == 1 && !parts[0].is_empty() {
			return Ok(Some(DeepLinkResult::Manga { key }));
		}
		if parts.len() >= 3 && parts[1].starts_with("vol") {
			return Ok(Some(DeepLinkResult::Chapter {
				manga_key: format!("/{}", parts[0]),
				key,
			}));
		}
		Ok(None)
	}
}

#[derive(Deserialize)]
struct Catalog {
	total: u64,
	offset: u64,
	limit: u64,
	list: Vec<CatalogItem>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogItem {
	name: String,
	pic_url: Option<String>,
	element_id: ElementId,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ElementId {
	link_name: String,
}

impl Catalog {
	fn into_page(self, base: &str, adult: bool) -> MangaPageResult {
		MangaPageResult {
			has_next_page: self.offset.saturating_add(self.limit) < self.total,
			entries: self
				.list
				.into_iter()
				.filter(|item| {
					!item.name.trim().is_empty()
						&& !item.element_id.link_name.contains(['/', '?', '#', '\\'])
				})
				.map(|item| {
					let key = format!("/{}", item.element_id.link_name);
					Manga {
						url: Some(format!("{base}{key}")),
						key,
						title: item.name,
						cover: item.pic_url.and_then(|u| absolute(&u, base)),
						content_rating: if adult {
							ContentRating::NSFW
						} else {
							ContentRating::Unknown
						},
						..Default::default()
					}
				})
				.collect(),
		}
	}
}

fn text(document: &Document, selector: &str) -> Option<String> {
	document
		.select_first(selector)
		.and_then(|el| el.text())
		.filter(|s| !s.trim().is_empty())
}

fn parse_details(document: &Document, mut manga: Manga, base: &str, adult: bool) -> Result<Manga> {
	manga.title = text(document, ".cr-hero-names__main")
		.or_else(|| {
			document
				.select_first("meta[itemprop=name]")
				.and_then(|el| el.attr("content"))
		})
		.filter(|s| !s.trim().is_empty())
		.ok_or_else(|| {
			error!("Не удалось прочитать карточку. Откройте сайт источника и проверьте вход.")
		})?;
	manga.url = Some(format!("{base}{}", site_path(&manga.key, base)?));
	manga.cover = document
		.select_first(".cr-hero-poster__img, .cr-hero-overlay__bg")
		.and_then(|el| {
			["src", "data-src", "data-original", "data-bg"]
				.into_iter()
				.find_map(|attr| el.attr(attr).and_then(|s| absolute(&s, base)))
		});
	manga.description = text(document, ".cr-description__content");
	manga.tags = document.select(".creation-element-tags__item span:not(.text-secondary), .cr-tags__item span:not(.text-secondary)").map(|es| es.filter_map(|e| e.text()).filter(|s| !s.trim().is_empty()).collect());
	let mut authors = Vec::new();
	let mut artists = Vec::new();
	if let Some(persons) = document.select(".cr-main-person-item") {
		for person in persons {
			let role = person
				.select_first(".cr-main-person-item__role")
				.and_then(|e| e.text())
				.unwrap_or_default()
				.to_lowercase();
			let name = person
				.select_first(".cr-main-person-item__name")
				.and_then(|e| e.text())
				.unwrap_or_default();
			if name.is_empty() {
				continue;
			}
			if role.contains("автор") || role.contains("сценар") {
				authors.push(name.clone());
			}
			if role.contains("худож") || role.contains("иллюст") {
				artists.push(name);
			}
		}
	}
	manga.authors = (!authors.is_empty()).then_some(authors);
	manga.artists = (!artists.is_empty()).then_some(artists);
	if let Some(details) = document.select(".cr-info-details-item") {
		for row in details {
			let label = row
				.select_first(".cr-info-details-item__title")
				.and_then(|e| e.text())
				.unwrap_or_default()
				.to_lowercase();
			if label.contains("выпуск") {
				let value = row
					.select_first(".cr-info-details-item__status")
					.and_then(|e| e.text())
					.unwrap_or_default()
					.to_lowercase();
				manga.status = if value.contains("заверш") {
					MangaStatus::Completed
				} else if value.contains("продолж") || value.contains("начат") {
					MangaStatus::Ongoing
				} else if value.contains("приост") || value.contains("заморож") {
					MangaStatus::Hiatus
				} else {
					MangaStatus::Unknown
				};
			}
		}
	}
	if adult {
		manga.content_rating = ContentRating::NSFW;
	}
	Ok(manga)
}

fn user_hash(document: &Document) -> Option<String> {
	document.select("script")?.find_map(|s| {
		let script = s.html()?;
		let tail = script.split_once("user_hash")?.1;
		let start = tail.find(['\'', '"'])?;
		let (hash, _) = js_string(&tail[start..])?;
		(!hash.is_empty()
			&& hash.len() <= 256
			&& hash
				.bytes()
				.all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'))
		.then_some(hash)
	})
}

fn parse_chapters(document: &Document, base: &str) -> Result<Vec<Chapter>> {
	if let Some(message) = text(document, ".alert.alert-warning")
		&& message.contains("копирайт")
	{
		bail!("Главы удалены по требованию правообладателя.");
	}
	let mut result = Vec::new();
	if let Some(rows) = document.select("tr.item-row") {
		for row in rows {
			if row.select_first("td.date.text-info").is_some() {
				continue;
			}
			let Some(link) = row.select_first("a.chapter-link") else {
				continue;
			};
			let Some(href) = link.attr("href") else {
				continue;
			};
			let key = chapter_path(&href, base)?;
			if result.iter().any(|ch: &Chapter| ch.key == key) {
				continue;
			}
			let chapter_number = row
				.select_first("td.item-title")
				.and_then(|e| e.attr("data-num"))
				.and_then(|s| s.parse::<f32>().ok())
				.map(|n| n / 10.0);
			let date = row
				.select("td.d-none")
				.and_then(|mut es| es.next_back())
				.and_then(|e| e.text())
				.and_then(|s| parse_date(format!("{} 00:00:00", s.trim()), "dd.MM.yy HH:mm:ss"));
			let scanlator = link
				.attr("title")
				.map(|s| s.replace(" (Переводчик)", "").replace(" Переводчик", ""));
			result.push(Chapter {
				url: Some(format!("{base}{key}")),
				key,
				title: link.own_text(),
				chapter_number,
				date_uploaded: date,
				scanlators: scanlator.filter(|s| !s.is_empty()).map(|s| vec![s]),
				language: Some("ru".into()),
				..Default::default()
			});
		}
	}
	if result.is_empty() {
		let scripts = document
			.select("script")
			.map(|es| es.filter_map(|e| e.html()).collect::<Vec<_>>().join("\n"))
			.unwrap_or_default();
		if scripts.contains("blockedForAnonymous")
			|| document.select_first("form[action*='login']").is_some()
		{
			bail!("Для просмотра глав войдите через настройки источника, затем обновите мангу.");
		}
	}
	Ok(result)
}

// Parse only quoted data, without evaluating a site's JavaScript.
fn js_string(input: &str) -> Option<(String, usize)> {
	let quote = *input.as_bytes().first()?;
	if !matches!(quote, b'\'' | b'"') {
		return None;
	}
	let mut output = String::new();
	let mut iter = input[1..].char_indices();
	while let Some((index, ch)) = iter.next() {
		if ch as u32 == u32::from(quote) {
			return Some((output, index + 2));
		}
		if ch != '\\' {
			output.push(ch);
			continue;
		}
		let (_, escaped) = iter.next()?;
		match escaped {
			'/' | '\\' | '\'' | '"' => output.push(escaped),
			'n' => output.push('\n'),
			'r' => output.push('\r'),
			't' => output.push('\t'),
			'u' => {
				let mut value = 0;
				for _ in 0..4 {
					value = value * 16 + iter.next()?.1.to_digit(16)?;
				}
				output.push(char::from_u32(value)?);
			}
			_ => return None,
		}
	}
	None
}

fn image_triples(script: &str, base: &str) -> Vec<String> {
	let Some(start) = script
		.find("rm_h.readerInit(")
		.or_else(|| script.find("rm_h.readerDoInit("))
	else {
		return Vec::new();
	};
	let script = &script[start..];
	let end = script.find(");").unwrap_or(script.len());
	let mut remaining = &script[..end];
	let mut images = Vec::new();
	while let Some(start) = remaining.find('[') {
		remaining = &remaining[start + 1..];
		let mut tail = remaining.trim_start();
		let mut values = Vec::new();
		for index in 0..3 {
			let Some((value, length)) = js_string(tail) else {
				break;
			};
			values.push(value);
			tail = tail[length..].trim_start();
			if index < 2 {
				let Some(next) = tail.strip_prefix(',') else {
					break;
				};
				tail = next.trim_start();
			}
		}
		if values.len() != 3 {
			continue;
		}
		let [host, middle, end] = [&values[0], &values[1], &values[2]];
		let image = if middle.is_empty() && end.starts_with("/static/") {
			format!("{base}{end}")
		} else if middle.ends_with("/manga/") {
			format!("{host}{end}")
		} else {
			format!("{middle}{host}{end}")
		};
		if let Some(mut url) = absolute(&image, base) {
			if url.contains("one-way.work/") {
				url = url.split('?').next().unwrap_or(&url).into();
			}
			images.push(url.replace("//resh", "//h"));
		}
		remaining = tail;
	}
	images
}

fn parse_pages(document: &Document, base: &str, referer: &str) -> Result<Vec<Page>> {
	ensure!(
		document.select_first("form.purchase-form").is_none(),
		"Глава требует покупки на сайте."
	);
	if let Some(alert) = text(document, "div.alert, h1")
		&& (alert.contains("премиум") || alert.contains("платная"))
	{
		bail!("Глава требует доступа на сайте.");
	}
	let mut urls = Vec::new();
	if let Some(scripts) = document.select("script") {
		for script in scripts {
			if let Some(data) = script.html() {
				urls = image_triples(&data, base);
				if !urls.is_empty() {
					break;
				}
			}
		}
	}
	ensure!(
		!urls.is_empty(),
		"Страницы не найдены. Проверьте вход и доступ к главе на сайте источника."
	);
	Ok(urls
		.into_iter()
		.map(|url| {
			let mut context = PageContext::new();
			context.insert("referer".into(), referer.into());
			Page {
				content: PageContent::url_context(url, context),
				..Default::default()
			}
		})
		.collect())
}

#[cfg(test)]
mod tests;
