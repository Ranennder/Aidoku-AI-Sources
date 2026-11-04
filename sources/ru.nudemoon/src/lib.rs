// Protocol and selectors adapted from Keiyoushi's Apache-2.0 NudeMoon source.
#![no_std]

mod parser;

use aidoku::{
	Chapter, DeepLinkHandler, DeepLinkResult, DynamicFilters, Filter, FilterValue, HashMap,
	ImageRequestProvider, Listing, ListingProvider, Manga, MangaPageResult, Page, PageContext,
	Result, Source, WebLoginHandler,
	alloc::{String, Vec, collections::BTreeSet},
	imports::{
		defaults::{DefaultValue, defaults_get, defaults_get_map, defaults_set},
		html::Document,
		net::{Request, TimeUnit, set_rate_limit},
	},
	prelude::*,
};

const DEFAULT_URL: &str = "https://nude-moon.org";
struct NudeMoon;

fn base_url() -> String {
	parser::base_url(
		defaults_get::<String>("baseUrl")
			.as_deref()
			.unwrap_or(DEFAULT_URL),
	)
}

impl NudeMoon {
	fn request(&self, path: &str) -> Result<Request> {
		let base = base_url();
		let path = parser::canonical_path(path, &base)?;
		let cookies = if defaults_get::<String>("loginOrigin").as_deref() == Some(base.as_str()) {
			defaults_get_map("login").unwrap_or_default()
		} else {
			HashMap::new()
		};
		Ok(Request::get(format!("{base}{path}"))?
			.header("Referer", &format!("{base}/"))
			.header("Cookie", &parser::cookie_header(cookies))
			.timeout(30.0))
	}

	fn document(&self, path: &str, searching: bool) -> Result<Document> {
		let response = self.request(path)?.send()?;
		if response.status_code() == 404 && searching {
			bail!("Nude-Moon: поиск может требовать входа на сайте в настройках источника");
		}
		parser::check_status(response.status_code())?;
		Ok(response.get_html()?)
	}

	fn chapter_list(&self, document: &Document, manga: &Manga) -> Result<Vec<Chapter>> {
		let base = base_url();
		let Some(first) = parser::all_chapters_url(document) else {
			return Ok(aidoku::alloc::vec![parser::single_chapter(
				document, &manga.key, &base
			)?]);
		};
		let mut chapters = Vec::new();
		let mut visited = BTreeSet::new();
		let mut next = Some(first);
		while let Some(url) = next {
			let path = parser::canonical_path(&url, &base)?;
			if !visited.insert(path.clone()) {
				bail!("Nude-Moon: повтор страницы списка глав");
			}
			let document = self.document(&path, false)?;
			parser::append_unique(&mut chapters, parser::chapters(&document, &base)?);
			next = parser::next_page(&document);
		}
		if chapters.is_empty() {
			bail!("Nude-Moon: сайт не вернул список глав; повторите обновление");
		}
		Ok(chapters)
	}
}

impl Source for NudeMoon {
	fn new() -> Self {
		set_rate_limit(1, 1, TimeUnit::Seconds);
		Self
	}

	fn get_search_manga_list(
		&self,
		query: Option<String>,
		page: i32,
		filters: Vec<FilterValue>,
	) -> Result<MangaPageResult> {
		let path = parser::search_path(query.as_deref(), page, filters);
		parser::catalog(
			&self.document(&path, query.as_ref().is_some_and(|q| !q.trim().is_empty()))?,
			&base_url(),
		)
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
		let document = self.document(&manga.key, false)?;
		if needs_details {
			manga = parser::details(&document, manga, &base_url())?;
		}
		if needs_chapters {
			manga.chapters = Some(self.chapter_list(&document, &manga)?);
		}
		Ok(manga)
	}

	fn get_page_list(&self, _manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		parser::pages(
			&self.document(&chapter.key, false)?,
			&base_url(),
			&chapter.key,
		)
	}
}

impl ListingProvider for NudeMoon {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		let sort = match listing.id.as_str() {
			"popular" => "views",
			"latest" => "date",
			_ => bail!("Unknown listing"),
		};
		let path = parser::catalog_path(sort, page);
		parser::catalog(&self.document(&path, false)?, &base_url())
	}
}

impl ImageRequestProvider for NudeMoon {
	fn get_image_request(&self, url: String, context: Option<PageContext>) -> Result<Request> {
		let base = base_url();
		let referer = context
			.as_ref()
			.and_then(|ctx| ctx.get("referer"))
			.and_then(|url| parser::canonical_path(url, &base).ok())
			.map(|path| format!("{base}{path}"))
			.unwrap_or(format!("{base}/"));
		Ok(Request::get(url)?.header("Referer", &referer))
	}
}

impl DeepLinkHandler for NudeMoon {
	fn handle_deep_link(&self, url: String) -> Result<Option<DeepLinkResult>> {
		let base = base_url();
		let Ok(path) = parser::canonical_path(&url, &base) else {
			return Ok(None);
		};
		if path.starts_with('/') && path.ends_with(".html") {
			Ok(Some(DeepLinkResult::Manga { key: path }))
		} else {
			Ok(None)
		}
	}
}

impl WebLoginHandler for NudeMoon {
	fn handle_web_login(&self, key: String, cookies: HashMap<String, String>) -> Result<bool> {
		if key != "login"
			|| !cookies
				.get("fusion_user")
				.is_some_and(|value| parser::valid_cookie_value(value))
		{
			return Ok(false);
		}
		defaults_set("loginOrigin", DefaultValue::String(base_url()));
		Ok(true)
	}
}

impl DynamicFilters for NudeMoon {
	fn get_dynamic_filters(&self) -> Result<Vec<Filter>> {
		let document = self.document("/tags/", false)?;
		let tags = parser::tags(&document);
		if tags.is_empty() {
			return Ok(Vec::new());
		}
		Ok(aidoku::alloc::vec![
			aidoku::MultiSelectFilter {
				id: "tags".into(),
				title: Some("Тэги".into()),
				is_genre: true,
				options: tags.iter().cloned().map(Into::into).collect(),
				ids: Some(tags.into_iter().map(Into::into).collect()),
				..Default::default()
			}
			.into()
		])
	}
}

register_source!(
	NudeMoon,
	ListingProvider,
	ImageRequestProvider,
	DeepLinkHandler,
	WebLoginHandler,
	DynamicFilters
);

#[cfg(test)]
mod tests;
