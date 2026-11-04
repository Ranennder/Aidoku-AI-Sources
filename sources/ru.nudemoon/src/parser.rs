use aidoku::{
	Chapter, ContentRating, HashMap, Manga, MangaPageResult, Page, PageContent, PageContext,
	Result,
	alloc::{String, Vec, string::ToString},
	helpers::uri::{QueryParameters, encode_uri_component},
	imports::{
		html::{Document, Element},
		std::parse_date,
	},
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
		bail!("Nude-Moon: ссылка относится к другому сайту");
	}
	Ok(path.into())
}

pub fn absolute_url(value: &str, base: &str) -> Option<String> {
	let value = value.trim();
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

pub fn valid_cookie_value(value: &str) -> bool {
	!value.is_empty()
		&& value
			.bytes()
			.all(|b| (0x21..=0x7e).contains(&b) && !matches!(b, b';' | b'"' | b'\\' | b','))
}

pub fn cookie_header(cookies: HashMap<String, String>) -> String {
	let mut pairs: Vec<_> = cookies
		.into_iter()
		.filter(|(name, value)| {
			name != "NMfYa"
				&& name != "nm_mobile"
				&& name != "Domain"
				&& name
					.bytes()
					.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
				&& valid_cookie_value(value)
		})
		.collect();
	pairs.sort_by(|a, b| a.0.cmp(&b.0));
	let mut header = String::from("NMfYa=1; nm_mobile=1");
	for (name, value) in pairs {
		header.push_str(&format!("; {name}={value}"));
	}
	header
}

pub fn check_status(status: i32) -> Result<()> {
	if !(200..300).contains(&status) {
		bail!("Nude-Moon HTTP {status}: проверьте доступ к сайту и вход в настройках источника");
	}
	Ok(())
}

pub fn catalog_path(sort: &str, page: i32) -> String {
	format!(
		"/all_manga?{sort}&rowstart={}",
		30 * i64::from(page.max(1) - 1)
	)
}

pub fn search_path(query: Option<&str>, page: i32, filters: Vec<aidoku::FilterValue>) -> String {
	let query = query.unwrap_or_default().trim();
	if !query.is_empty() {
		let mut params = QueryParameters::new();
		params.push("stext", Some(query));
		params.push(
			"rowstart",
			Some(&(30 * i64::from(page.max(1) - 1)).to_string()),
		);
		return format!("/search?{params}");
	}
	let mut sort = "views";
	let mut tags = Vec::new();
	for filter in filters {
		match filter {
			aidoku::FilterValue::Sort { index, .. } => {
				sort = match index {
					0 => "date",
					2 => "like",
					3 => "com",
					4 => "pages",
					_ => "views",
				}
			}
			aidoku::FilterValue::MultiSelect { included, .. } => tags.extend(
				included
					.into_iter()
					.map(|tag| encode_uri_component(tag.replace(' ', "_")).to_string()),
			),
			_ => {}
		}
	}
	if tags.is_empty() {
		catalog_path(sort, page)
	} else {
		format!(
			"/tags/{}&{sort}&rowstart={}",
			tags.join("+"),
			30 * i64::from(page.max(1) - 1)
		)
	}
}

fn manga_link(element: &Element) -> Option<Element> {
	element
		.select("a")?
		.find(|link| link.select_first("h2").is_some())
}

pub fn next_page(document: &Document) -> Option<String> {
	document
		.select("a.small")?
		.find(|link| {
			link.text()
				.is_some_and(|text| text.contains('>') || text.contains("След"))
		})
		.and_then(|link| link.attr("abs:href"))
}

pub fn catalog(document: &Document, base: &str) -> Result<MangaPageResult> {
	let entries: Vec<Manga> = document
		.select("table.news_pic2")
		.map(|elements| {
			elements
				.filter_map(|element| {
					let link = manga_link(&element)?;
					let key = canonical_path(&link.attr("abs:href")?, base).ok()?;
					let title = title(&link.text()?);
					Some(Manga {
						key,
						title,
						cover: element
							.select_first("a img")
							.and_then(|img| img.attr("abs:src")),
						content_rating: ContentRating::NSFW,
						..Default::default()
					})
				})
				.collect()
		})
		.unwrap_or_default();
	if entries.is_empty() && document.select_first("table.news_pic2").is_none() {
		let text = document
			.select_first("body")
			.and_then(|el| el.text())
			.unwrap_or_default()
			.to_lowercase();
		if !text.contains("не найден") && !text.contains("ничего не") {
			bail!("Nude-Moon: каталог недоступен; откройте сайт в WebView и проверьте вход");
		}
	}
	Ok(MangaPageResult {
		entries,
		has_next_page: next_page(document).is_some(),
	})
}

fn title(value: &str) -> String {
	value
		.split(" / ")
		.next()
		.unwrap_or(value)
		.split(" №")
		.next()
		.unwrap_or(value)
		.trim()
		.into()
}

pub fn details(document: &Document, mut manga: Manga, base: &str) -> Result<Manga> {
	manga.title = title(
		&document
			.select_first("h1")
			.and_then(|el| el.text())
			.ok_or_else(|| error!("Nude-Moon: сайт не вернул название; проверьте вход"))?,
	);
	manga.cover = document
		.select_first("meta[property='og:image']")
		.and_then(|el| el.attr("content"))
		.and_then(|url| absolute_url(&url, base));
	manga.description = document
		.select_first(".description")
		.and_then(|el| el.text());
	manga.authors = document
		.select("a[href*=mangaka]")
		.map(|elements| elements.filter_map(|el| el.text()).collect());
	manga.tags = document
		.select("div.tag-links a")
		.map(|elements| elements.filter_map(|el| el.text()).collect());
	manga.content_rating = ContentRating::NSFW;
	manga.url = Some(format!("{base}{}", manga.key));
	Ok(manga)
}

pub fn all_chapters_url(document: &Document) -> Option<String> {
	document
		.select("td.button a")?
		.find(|el| el.text().is_some_and(|text| text.contains("Все главы")))
		.and_then(|el| el.attr("abs:href"))
}

pub fn tags(document: &Document) -> Vec<String> {
	let mut tags = Vec::new();
	if let Some(elements) = document.select("input[name*=tag]") {
		for element in elements {
			if let Some(value) = element.attr("value") {
				let value = value.trim();
				if !value.is_empty() && !tags.iter().any(|tag| tag == value) {
					tags.push(value.to_string());
				}
			}
		}
	}
	tags
}

pub fn chapter_number(title: &str) -> Option<f32> {
	let text = title.split_once('№')?.1.trim_start();
	let value: String = text
		.chars()
		.take_while(|c| c.is_ascii_digit() || *c == '-' || *c == '.')
		.collect();
	value.replace('-', ".").parse().ok()
}

pub fn date_from_text(text: &str) -> Option<i64> {
	let months = [
		"янв", "фев", "мар", "апр", "ма", "июн", "июл", "авг", "сен", "окт", "ноя", "дек",
	];
	let words: Vec<_> = text.split_whitespace().collect();
	for trio in words.windows(3) {
		let Some(month) = months
			.iter()
			.position(|prefix| trio[1].to_lowercase().starts_with(*prefix))
		else {
			continue;
		};
		let Some(day) = trio[0]
			.parse::<u8>()
			.ok()
			.filter(|day| (1..=31).contains(day))
		else {
			continue;
		};
		let Some(year) = trio[2]
			.trim_matches(|c: char| !c.is_ascii_digit())
			.parse::<u16>()
			.ok()
			.filter(|year| *year >= 1900)
		else {
			continue;
		};
		return parse_date(
			format!("{year:04}-{:02}-{day:02} 00:00:00", month + 1),
			"yyyy-MM-dd HH:mm:ss",
		);
	}
	None
}

pub fn chapters(document: &Document, base: &str) -> Result<Vec<Chapter>> {
	document
		.select("table.news_pic2")
		.map(|elements| {
			elements
				.filter_map(|element| {
					let link = manga_link(&element)?;
					let name = link.text()?;
					let key = canonical_path(&link.attr("abs:href")?, base).ok()?;
					Some(Chapter {
						key: key.clone(),
						title: Some(name.clone()),
						chapter_number: chapter_number(&name),
						date_uploaded: element
							.select_first("span.small2")
							.and_then(|el| el.text())
							.and_then(|date| date_from_text(&date)),
						scanlators: element
							.select("a[href*=perevod]")
							.map(|els| els.filter_map(|el| el.text()).collect()),
						url: Some(format!("{base}{key}")),
						..Default::default()
					})
				})
				.collect()
		})
		.ok_or_else(|| error!("Nude-Moon: неверный список глав"))
}

pub fn single_chapter(document: &Document, key: &str, base: &str) -> Result<Chapter> {
	if !document
		.select_first("h1")
		.and_then(|heading| heading.text())
		.is_some_and(|title| !title.trim().is_empty())
	{
		bail!(
			"Nude-Moon: сайт не вернул страницу произведения; проверьте вход и повторите обновление"
		);
	}
	Ok(Chapter {
		key: key.into(),
		title: Some("Сингл".into()),
		chapter_number: Some(0.0),
		date_uploaded: document.select("span.small2").and_then(|els| {
			els.filter_map(|el| el.text())
				.find_map(|text| date_from_text(&text))
		}),
		url: Some(format!("{base}{key}")),
		..Default::default()
	})
}

pub fn append_unique(target: &mut Vec<Chapter>, page: Vec<Chapter>) {
	for chapter in page {
		if !target.iter().any(|existing| existing.key == chapter.key) {
			target.push(chapter);
		}
	}
}

pub fn pages(document: &Document, base: &str, key: &str) -> Result<Vec<Page>> {
	let mut context = PageContext::new();
	context.insert("referer".into(), format!("{base}{key}"));
	let images = document
		.select("img[loading=lazy][title]")
		.ok_or_else(|| error!("Nude-Moon: страницы не найдены"))?;
	let pages: Vec<_> = images
		.filter(|img| img.attr("title").is_some_and(|title| !title.is_empty()))
		.map(|img| {
			let url = img
				.attr("abs:data-src")
				.or_else(|| {
					img.attr("data-src")
						.and_then(|url| absolute_url(&url, base))
				})
				.and_then(|url| absolute_url(&url, base))
				.ok_or_else(|| error!("Nude-Moon: отсутствует изображение страницы"))?;
			Ok(Page {
				content: PageContent::url_context(url, context.clone()),
				..Default::default()
			})
		})
		.collect::<Result<_>>()?;
	if pages.is_empty() {
		bail!("Nude-Moon: страницы не найдены; войдите на сайте в настройках источника");
	}
	Ok(pages)
}
