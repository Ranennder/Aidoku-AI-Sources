use aidoku::{
	Chapter, ContentRating, FilterValue, HashMap, Manga, MangaPageResult, MangaStatus, Page,
	PageContent, PageContext, Result,
	alloc::{String, Vec, string::ToString},
	helpers::uri::{QueryParameters, encode_uri_component},
	imports::html::Document,
	prelude::*,
};

pub fn base_url(value: &str) -> String {
	let value = value.trim().trim_end_matches('/');
	if value
		.strip_prefix("https://")
		.is_some_and(|host| !host.is_empty() && !host.contains(['/', '?', '#', '@', '\\', ' ']))
	{
		value.into()
	} else {
		crate::DEFAULT_URL.into()
	}
}

pub fn canonical_path(value: &str, base: &str) -> Result<String> {
	let path = value
		.strip_prefix(base)
		.unwrap_or(value)
		.split('#')
		.next()
		.unwrap_or_default();
	if !path.starts_with('/')
		|| path.starts_with("//")
		|| path.contains(['\\', '\r', '\n'])
		|| path.split('/').any(|part| part == "." || part == "..")
	{
		bail!("HenChan: ссылка относится к другому сайту");
	}
	Ok(path.into())
}

fn absolute_url(value: &str, base: &str) -> Option<String> {
	if value.starts_with("https://") || value.starts_with("http://") {
		Some(value.into())
	} else if value.starts_with("//") {
		Some(format!("https:{value}"))
	} else if value.starts_with('/') {
		Some(format!("{base}{value}"))
	} else {
		None
	}
}

pub fn cookie_header(cookies: HashMap<String, String>) -> Option<String> {
	let mut pairs: Vec<_> = cookies
		.into_iter()
		.filter(|(name, value)| {
			!name.is_empty()
				&& name
					.bytes()
					.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
				&& !value.is_empty()
				&& value
					.bytes()
					.all(|b| (0x21..=0x7e).contains(&b) && !matches!(b, b';' | b'"' | b'\\' | b','))
		})
		.collect();
	pairs.sort_by(|a, b| a.0.cmp(&b.0));
	if pairs.is_empty() {
		None
	} else {
		Some(
			pairs
				.into_iter()
				.map(|(name, value)| format!("{name}={value}"))
				.collect::<Vec<_>>()
				.join("; "),
		)
	}
}

pub fn check_status(status: i32) -> Result<()> {
	if !(200..300).contains(&status) {
		bail!("HenChan HTTP {status}: проверьте доступ к сайту и вход в настройках источника");
	}
	Ok(())
}

pub fn search_path(query: Option<&str>, page: i32, filters: Vec<FilterValue>) -> String {
	let query = query.unwrap_or_default().trim();
	if !query.is_empty() {
		let mut params = QueryParameters::new();
		params.push("do", Some("search"));
		params.push("subaction", Some("search"));
		params.push("story", Some(query));
		params.push("search_start", Some(&page.max(1).to_string()));
		return format!("/?{params}");
	}
	let mut index = 1;
	let mut ascending = false;
	let mut tags = Vec::new();
	for filter in filters {
		match filter {
			FilterValue::Sort {
				index: selected,
				ascending: order,
				..
			} => {
				index = selected;
				ascending = order;
			}
			FilterValue::MultiSelect {
				included, excluded, ..
			} => {
				tags.extend(
					included
						.into_iter()
						.map(|tag| encode_uri_component(tag).to_string()),
				);
				tags.extend(
					excluded
						.into_iter()
						.map(|tag| format!("-{}", encode_uri_component(tag))),
				);
			}
			_ => {}
		}
	}
	let offset = 20 * i64::from(page.max(1) - 1);
	if tags.is_empty() {
		let order = match (index, ascending) {
			(0, true) => "manga/new&n=dateasc",
			(0, false) => "manga/new",
			(2, true) => "manga/new&n=abcdesc",
			(2, false) => "manga/new&n=abcasc",
			(_, true) => "manga/new&n=favasc",
			_ => "mostfavorites&sort=manga",
		};
		format!("/{order}?offset={offset}")
	} else {
		let order = match (index, ascending) {
			(0, true) => "&n=dateasc",
			(0, false) => "",
			(2, true) => "&n=abcdesc",
			(2, false) => "&n=abcasc",
			(_, true) => "&n=favasc",
			_ => "&n=favdesc",
		};
		format!("/tags/{}&sort=manga{order}?offset={offset}", tags.join("+"))
	}
}

fn next_link(document: &Document, selector: &str, search: bool) -> Option<String> {
	document
		.select(selector)?
		.find(|link| {
			link.text().is_some_and(|text| {
				text.contains("Вперед")
					|| text.contains("Вперёд")
					|| (search && text.contains("Далее"))
			})
		})
		.and_then(|link| link.attr("abs:href").or_else(|| link.attr("href")))
}

pub fn next_related_page(document: &Document) -> Option<String> {
	next_link(document, "div#pagination_related a", false)
}

pub fn thumbnail(value: &str, base: &str) -> Option<String> {
	let url = absolute_url(value, base)?;
	let parts: Vec<_> = url.split('/').collect();
	Some(
		parts
			.into_iter()
			.map(|part| {
				if part.to_lowercase().starts_with("manganew_thumbs") {
					"showfull_retina/manga"
				} else {
					part
				}
			})
			.collect::<Vec<_>>()
			.join("/")
			.replace(
				&format!("_{}", base.trim_start_matches("https://")),
				"_hentaichan.ru",
			),
	)
}

pub fn catalog(document: &Document, base: &str, search: bool) -> Result<MangaPageResult> {
	let entries: Vec<Manga> = document
		.select("div.content_row")
		.map(|elements| {
			elements
				.filter_map(|element| {
					if search
						&& element.select("div.item").is_some_and(|mut els| {
							els.any(|el| el.own_text().is_some_and(|text| text.contains("Тип")))
						}) {
						return None;
					}
					let link = element.select_first("h2 > a")?;
					let key = canonical_path(&link.attr("abs:href")?, base).ok()?;
					let title = element
						.attr("title")
						.filter(|name| !name.trim().is_empty())
						.or_else(|| link.text())?;
					Some(Manga {
						key,
						title,
						cover: element
							.select_first("img")
							.and_then(|img| img.attr("abs:src"))
							.and_then(|url| thumbnail(&url, base)),
						content_rating: ContentRating::NSFW,
						..Default::default()
					})
				})
				.collect()
		})
		.unwrap_or_default();
	if entries.is_empty() && document.select_first("div.content_row").is_none() {
		let text = document
			.select_first("body")
			.and_then(|el| el.text())
			.unwrap_or_default()
			.to_lowercase();
		if !search || (!text.contains("не найден") && !text.contains("ничего не")) {
			bail!("HenChan: каталог недоступен; откройте сайт в WebView и проверьте домен");
		}
	}
	let has_next_page = next_link(document, "a", search).is_some()
		|| (search
			&& document.select("a[onclick]").is_some_and(|mut els| {
				els.any(|link| link.text().is_some_and(|text| text.contains("Далее")))
			}));
	Ok(MangaPageResult {
		entries,
		has_next_page,
	})
}

pub fn details(document: &Document, mut manga: Manga, base: &str) -> Result<Manga> {
	let title = document
		.select_first("title")
		.and_then(|el| el.text())
		.ok_or_else(|| error!("HenChan: сайт не вернул название; проверьте вход"))?;
	manga.title = title.split(" »").next().unwrap_or(&title).trim().into();
	manga.cover = document
		.select_first("img#cover")
		.and_then(|el| el.attr("abs:src"))
		.and_then(|url| thumbnail(&url, base));
	manga.description = document
		.select_first("div#description")
		.and_then(|el| el.own_text());
	manga.authors = document
		.select("#info_wrap tr, #info_wrap > div")
		.map(|elements| {
			elements
				.filter(|el| el.text().is_some_and(|text| text.contains("Автор")))
				.filter_map(|el| el.select_first(".item2").and_then(|el| el.text()))
				.collect()
		});
	manga.tags = document
		.select(".sidetags ul a:last-child")
		.map(|elements| elements.filter_map(|el| el.text()).collect());
	let information = document
		.select_first("#info_wrap")
		.and_then(|el| el.text())
		.unwrap_or_default();
	manga.status = if information.contains("перевод завершен")
		|| information.contains("перевод завершён")
	{
		MangaStatus::Completed
	} else if information.contains("перевод продолжается") {
		MangaStatus::Ongoing
	} else {
		MangaStatus::Unknown
	};
	manga.content_rating = ContentRating::NSFW;
	manga.url = Some(format!("{base}{}", manga.key));
	Ok(manga)
}

pub fn external_gallery(document: &Document) -> bool {
	document
		.select_first("img#cover")
		.and_then(|el| el.attr("src"))
		.is_some_and(|url| url.contains("/manganew_thumbs_blur/"))
}
pub fn has_single_related(document: &Document) -> bool {
	document
		.select_first("#right")
		.and_then(|el| el.text())
		.is_some_and(|text| text.contains(" похожий на "))
}

fn chapter_number(value: &str) -> Option<f32> {
	let lower = value.to_lowercase();
	let start = lower
		.find("глава ")
		.map(|i| i + "глава ".len())
		.or_else(|| lower.find("часть ").map(|i| i + "часть ".len()))?;
	lower[start..]
		.chars()
		.take_while(|c| c.is_ascii_digit() || *c == '.')
		.collect::<String>()
		.parse()
		.ok()
}

pub fn chapters(document: &Document, base: &str) -> Result<Vec<Chapter>> {
	document
		.select(".related")
		.map(|elements| {
			elements
				.filter_map(|element| {
					let link = element.select_first("h2 a")?;
					let title = link
						.attr("title")
						.filter(|text| !text.trim().is_empty())
						.or_else(|| link.text())?;
					let key = canonical_path(&link.attr("abs:href")?, base).ok()?;
					Some(Chapter {
						key: key.clone(),
						title: Some(title.clone()),
						chapter_number: chapter_number(&title),
						url: Some(format!("{base}{key}")),
						..Default::default()
					})
				})
				.collect()
		})
		.ok_or_else(|| error!("HenChan: неверный список связанных глав"))
}

pub fn single_chapter(document: &Document, key: &str, base: &str) -> Chapter {
	Chapter {
		key: key.into(),
		title: document
			.select_first("a.title_top_a")
			.and_then(|el| el.text())
			.or_else(|| Some("Глава".into())),
		chapter_number: Some(1.0),
		url: Some(format!("{base}{key}")),
		..Default::default()
	}
}

pub fn append_unique(target: &mut Vec<Chapter>, page: Vec<Chapter>) {
	for chapter in page {
		if !target.iter().any(|old| old.key == chapter.key) {
			target.push(chapter);
		}
	}
}

pub fn pages(html: &str, base: &str, key: &str) -> Result<Vec<Page>> {
	let marker = html
		.find("\"fullimg\"")
		.ok_or_else(|| error!("HenChan: список изображений не найден; проверьте вход"))?;
	let rest = html[marker + "\"fullimg\"".len()..]
		.trim_start()
		.strip_prefix(':')
		.ok_or_else(|| error!("HenChan: неверный список изображений"))?
		.trim_start();
	if !rest.starts_with('[') {
		bail!("HenChan: неверный список изображений");
	}
	let start = 0;
	let mut quoted = false;
	let mut escaped = false;
	let mut end = None;
	for (index, ch) in rest[start + 1..].char_indices() {
		if escaped {
			escaped = false;
			continue;
		}
		if quoted && ch == '\\' {
			escaped = true;
			continue;
		}
		if ch == '"' {
			quoted = !quoted;
		}
		if !quoted && ch == ']' {
			end = Some(start + 1 + index);
			break;
		}
	}
	let end = end.ok_or_else(|| error!("HenChan: незавершенный список изображений"))?;
	let body = rest[start + 1..end].trim().trim_end_matches(',');
	let urls: Vec<String> = serde_json::from_str(&format!("[{body}]"))?;
	if urls.is_empty() {
		bail!("HenChan: пустой список изображений; проверьте вход");
	}
	let mut context = PageContext::new();
	context.insert("referer".into(), format!("{base}{key}"));
	urls.into_iter()
		.map(|url| {
			let url = absolute_url(&url, base)
				.ok_or_else(|| error!("HenChan: некорректный адрес страницы"))?;
			Ok(Page {
				content: PageContent::url_context(url, context.clone()),
				..Default::default()
			})
		})
		.collect()
}
