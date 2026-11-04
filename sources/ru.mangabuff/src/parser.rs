use aidoku::{
	Chapter, Manga, MangaPageResult, MangaStatus, Page, PageContent, PageContext, Result, Viewer,
	alloc::{String, Vec, string::ToString},
	helpers::uri::QueryParameters,
	imports::{
		html::{Document, Element, Html},
		std::parse_date,
	},
	prelude::*,
};

pub const BASE_URL: &str = "https://mangabuff.ru";

pub fn check_status(status: i32) -> Result<()> {
	match status {
		200..=299 => Ok(()),
		401 => bail!(
			"MangaBuff: эта глава может содержать контент 18+. Войдите на сайте через настройки источника → «Войти в MangaBuff»."
		),
		403 => bail!(
			"MangaBuff отклонил запрос. Откройте сайт в WebView источника, пройдите проверку сайта и повторите."
		),
		404 => bail!(
			"MangaBuff: материал недоступен или удалён. Для части материалов требуется вход на сайте."
		),
		419 => bail!("MangaBuff: сессия истекла. Откройте сайт в WebView источника и повторите."),
		429 => bail!("MangaBuff: слишком много запросов. Подождите и повторите."),
		_ => bail!("MangaBuff вернул HTTP {status}."),
	}
}

/// Keep the website path as the stable key; never infer identity from titles
/// or chapter numbers, and never turn an arbitrary supplied URL into a request.
pub fn canonical_path(input: &str, chapter: bool) -> Result<String> {
	let value = input.trim();
	let path = value.strip_prefix(BASE_URL).unwrap_or(value);
	let path = path
		.split(['?', '#'])
		.next()
		.unwrap_or_default()
		.trim_end_matches('/');
	let segments: Vec<_> = path.split('/').collect();
	let count = if chapter { 5 } else { 3 };
	if segments.len() != count
		|| !path.starts_with("/manga/")
		|| segments.iter().skip(2).any(|part| {
			part.is_empty()
				|| *part == "."
				|| *part == ".."
				|| part.contains(':')
				|| part.contains('\\')
				|| part.contains('%')
		}) {
		bail!("MangaBuff: некорректная ссылка на произведение или главу.");
	}
	Ok(path.to_string())
}

pub fn catalog_path(sort: &str, page: i32) -> String {
	format!("/manga?sort={sort}&page={}", page.max(1))
}

pub fn search_path(query: Option<&str>, page: i32) -> Result<String> {
	let query = query.unwrap_or_default().trim();
	if query.is_empty() {
		return Ok(catalog_path("real_views", page));
	}
	if query.chars().count() < 3 {
		bail!("Введите не менее 3 символов для поиска MangaBuff.");
	}
	let mut parameters = QueryParameters::new();
	parameters.push("type", Some("manga"));
	parameters.push("q", Some(query));
	let page = page.max(1).to_string();
	parameters.push("page", Some(&page));
	Ok(format!("/search?{parameters}"))
}

pub fn csrf_token(document: &Document) -> Option<String> {
	document
		.select_first("meta[name='csrf-token']")?
		.attr("content")
		.filter(|token| !token.is_empty())
}

fn image_url(element: &Element) -> Option<String> {
	let value = element
		.attr("data-src")
		.filter(|value| !value.trim().is_empty())
		.or_else(|| element.attr("src"))?;
	let value = value.trim();
	if value.starts_with("https://") || value.starts_with("http://") {
		Some(value.to_string())
	} else if value.starts_with("//") {
		Some(format!("https:{value}"))
	} else if value.starts_with('/') {
		Some(format!("{BASE_URL}{value}"))
	} else {
		None
	}
}

pub fn parse_catalog(document: &Document) -> Result<MangaPageResult> {
	let cards = document.select_first(".cards").ok_or_else(|| {
		error!(
			"MangaBuff: каталог не найден. Откройте сайт в WebView источника и проверьте доступ."
		)
	})?;
	let entries = cards
		.select(".cards__item")
		.into_iter()
		.flatten()
		.filter_map(|item| {
			let href = item
				.attr("href")
				.filter(|value| !value.is_empty())
				.or_else(|| item.select_first("a[href]")?.attr("href"))?;
			let key = canonical_path(&href, false).ok()?;
			let title = item
				.select_first(".cards__name")?
				.text()
				.filter(|title| !title.is_empty())?;
			let cover = item
				.select_first("img")
				.and_then(|image| image_url(&image))
				.or_else(|| {
					Some(format!(
						"{BASE_URL}/img/manga/posters/{}.jpg",
						key.rsplit('/').next()?
					))
				});
			Some(Manga {
				url: Some(format!("{BASE_URL}{key}")),
				key,
				title,
				cover,
				..Default::default()
			})
		})
		.collect();
	let has_next_page = document
		.select(".pagination .pagination__button a[href]")
		.into_iter()
		.flatten()
		.any(|link| {
			link.text()
				.is_some_and(|text| text.contains("Вперёд") || text.contains("Вперед"))
		});
	Ok(MangaPageResult {
		entries,
		has_next_page,
	})
}

fn status(value: &str) -> MangaStatus {
	match value.trim().to_lowercase().as_str() {
		"продолжается" => MangaStatus::Ongoing,
		"завершен" | "завершён" => MangaStatus::Completed,
		"заморожен" => MangaStatus::Hiatus,
		"заброшен" => MangaStatus::Cancelled,
		_ => MangaStatus::Unknown,
	}
}

fn readable_description(element: &Element) -> Option<String> {
	// Mark serialized block boundaries before parsing a full document. This
	// needs neither DOM mutations nor the test host's incomplete node casts.
	const BREAK: &str = "\u{E000}";
	let mut html = element.html()?;
	for tag in ["</p>", "</div>", "</li>", "</section>", "</blockquote>"] {
		html = html.replace(tag, &format!("{tag}{BREAK}{BREAK}"));
	}
	for tag in ["<br>", "<br/>", "<br />"] {
		html = html.replace(tag, BREAK);
	}
	let document = Html::parse(format!("<html><body>{html}</body></html>")).ok()?;
	let text = document.select_first("body")?.text()?;
	let mut lines: Vec<String> = Vec::new();
	for line in text.split(BREAK) {
		let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
		if !line.is_empty() || lines.last().is_some_and(|last| !last.is_empty()) {
			lines.push(line);
		}
	}
	Some(lines.join("\n").trim().to_string())
}

pub fn parse_details(document: &Document, mut manga: Manga) -> Result<Manga> {
	manga.title = document
		.select_first("h1, .manga__name, .manga-mobile__name")
		.and_then(|title| title.text())
		.filter(|title| !title.is_empty())
		.ok_or_else(|| error!("MangaBuff: название произведения не найдено."))?;
	manga.url = Some(format!("{BASE_URL}{}", canonical_path(&manga.key, false)?));
	manga.description = document
		.select_first(".manga__description")
		.and_then(|element| readable_description(&element));
	manga.cover = document
		.select_first(".manga__img img, img.manga-mobile__image")
		.and_then(|element| image_url(&element))
		.or(manga.cover);
	let mut tags: Vec<String> = Vec::new();
	for link in document
		.select(".manga__middle-links a, .manga-mobile__info a, .tags .tags__item")
		.into_iter()
		.flatten()
	{
		let Some(text) = link.text().filter(|text| !text.is_empty()) else {
			continue;
		};
		let parsed_status = status(&text);
		if parsed_status != MangaStatus::Unknown {
			manga.status = parsed_status;
		} else if !tags.contains(&text) {
			tags.push(text);
		}
	}
	if tags.iter().any(|tag| tag == "Манхва" || tag == "Маньхуа") {
		manga.viewer = Viewer::Webtoon;
	} else if tags.iter().any(|tag| tag == "Манга") {
		manga.viewer = Viewer::RightToLeft;
	}
	if !tags.is_empty() {
		manga.tags = Some(tags);
	}
	Ok(manga)
}

fn number(text: Option<String>) -> Option<f32> {
	text?
		.split_whitespace()
		.rev()
		.find_map(|part| part.parse::<f32>().ok())
		.filter(|value| value.is_finite())
}

pub fn parse_upload_date(value: &str) -> Option<i64> {
	// Supply an explicit UTC midnight so the same ICU format works with both
	// the app's DateFormatter and the test host's full date-time parser.
	parse_date(format!("{} 00:00:00", value.trim()), "dd.MM.yyyy HH:mm:ss")
}

pub fn parse_chapters(document: &Document) -> Result<Vec<Chapter>> {
	let mut chapters = Vec::new();
	for element in document
		.select("a.chapters__item[href]")
		.into_iter()
		.flatten()
	{
		let Some(key) = element
			.attr("href")
			.and_then(|value| canonical_path(&value, true).ok())
		else {
			continue;
		};
		if chapters.iter().any(|chapter: &Chapter| chapter.key == key) {
			continue;
		}
		let title = element
			.select_first(".chapters__name")
			.and_then(|value| value.text())
			.filter(|title| !title.is_empty());
		let chapter_number = element
			.select_first(".chapters__value")
			.and_then(|value| number(value.text()));
		let volume_number = element
			.select_first(".chapters__volume")
			.and_then(|value| number(value.text()));
		let date_uploaded = element
			.select_first(".chapters__add-date")
			.and_then(|value| value.text())
			.or_else(|| element.attr("data-chapter-date"))
			.and_then(|value| parse_upload_date(&value));
		chapters.push(Chapter {
			url: Some(format!("{BASE_URL}{key}")),
			key,
			title,
			chapter_number,
			volume_number,
			date_uploaded,
			language: Some("ru".into()),
			..Default::default()
		});
	}
	Ok(chapters)
}

pub fn append_unique_chapters(chapters: &mut Vec<Chapter>, remaining: Vec<Chapter>) {
	for chapter in remaining {
		if !chapters.iter().any(|existing| existing.key == chapter.key) {
			chapters.push(chapter);
		}
	}
}

pub fn parse_pages(document: &Document, referer: &str) -> Result<Vec<Page>> {
	let container = document.select_first(".reader__pages").ok_or_else(|| {
		error!(
			"MangaBuff: страницы главы не найдены. Проверьте доступ на сайте через WebView источника."
		)
	})?;
	let pages: Vec<_> = container
		.select("img")
		.into_iter()
		.flatten()
		.filter_map(|image| {
			let url = image_url(&image)?;
			let mut context = PageContext::new();
			context.insert("referer".into(), referer.into());
			Some(Page {
				content: PageContent::url_context(url, context),
				..Default::default()
			})
		})
		.collect();
	if pages.is_empty() {
		bail!("MangaBuff: глава не содержит доступных страниц.");
	}
	Ok(pages)
}
