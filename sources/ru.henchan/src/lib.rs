// Protocol and selectors adapted from Keiyoushi's Apache-2.0 HenChan/MultiChan.
#![no_std]

mod parser;

use aidoku::{
	Chapter, DeepLinkHandler, DeepLinkResult, FilterValue, ImageRequestProvider, Listing,
	ListingProvider, Manga, MangaPageResult, NotificationHandler, Page, PageContext, Result,
	Source,
	alloc::{String, Vec, collections::BTreeSet},
	imports::{
		defaults::{DefaultValue, defaults_get, defaults_get_map, defaults_set},
		html::Document,
		net::{Request, TimeUnit, set_rate_limit},
	},
	prelude::*,
};

const DEFAULT_URL: &str = "https://xxl.hentaichan.live";
struct HenChan;

fn base_url() -> String {
	parser::base_url(
		defaults_get::<String>("baseUrl")
			.as_deref()
			.unwrap_or(DEFAULT_URL),
	)
}

impl HenChan {
	fn request(&self, path: &str) -> Result<Request> {
		let base = base_url();
		let path = parser::canonical_path(path, &base)?;
		let mut request = Request::get(format!("{base}{path}"))?
			.header("Referer", &format!("{base}/"))
			.timeout(30.0);
		if defaults_get::<String>("loginOrigin").as_deref() == Some(base.as_str())
			&& let Some(cookie) = defaults_get_map("login").and_then(parser::cookie_header)
		{
			request = request.header("Cookie", &cookie);
		}
		Ok(request)
	}

	fn document(&self, path: &str) -> Result<Document> {
		let response = self.request(path)?.send()?;
		parser::check_status(response.status_code())?;
		Ok(response.get_html()?)
	}

	fn chapter_list(&self, manga: &Manga, document: &Document) -> Result<Vec<Chapter>> {
		let base = base_url();
		if parser::external_gallery(document) {
			return Ok(aidoku::alloc::vec![parser::single_chapter(
				document, &manga.key, &base
			)]);
		}
		let related = manga.key.replacen("/manga/", "/related/", 1);
		let first = self.request(&related)?.send()?;
		if first.status_code() == 404 {
			return Ok(aidoku::alloc::vec![parser::single_chapter(
				document, &manga.key, &base
			)]);
		}
		parser::check_status(first.status_code())?;
		let mut current = first.get_html()?;
		let mut visited = BTreeSet::new();
		visited.insert(related);
		let mut chapters = Vec::new();
		loop {
			parser::append_unique(&mut chapters, parser::chapters(&current, &base)?);
			let Some(next) = parser::next_related_page(&current) else {
				break;
			};
			let path = parser::canonical_path(&next, &base)?;
			if !visited.insert(path.clone()) {
				bail!("HenChan: повтор страницы связанных глав");
			}
			current = self.document(&path)?;
		}
		if chapters.is_empty() {
			if parser::has_single_related(&current) {
				return Ok(aidoku::alloc::vec![parser::single_chapter(
					document, &manga.key, &base
				)]);
			}
			bail!("HenChan: сайт не вернул связанные главы; повторите обновление");
		}
		chapters.reverse();
		Ok(chapters)
	}
}

impl Source for HenChan {
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
		parser::catalog(
			&self.document(&parser::search_path(query.as_deref(), page, filters))?,
			&base_url(),
			true,
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
		let document = self.document(&manga.key)?;
		if needs_details {
			manga = parser::details(&document, manga, &base_url())?;
		}
		if needs_chapters {
			manga.chapters = Some(self.chapter_list(&manga, &document)?);
		}
		Ok(manga)
	}
	fn get_page_list(&self, _manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let path = chapter.key.replacen("/manga/", "/online/", 1);
		let response = self
			.request(&path)?
			.header("Accept", "image/webp,image/apng,text/html,*/*")
			.send()?;
		parser::check_status(response.status_code())?;
		parser::pages(&response.get_string()?, &base_url(), &chapter.key)
	}
}

impl ListingProvider for HenChan {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		let path = match listing.id.as_str() {
			"popular" => format!("/mostfavorites?offset={}", 20 * i64::from(page.max(1) - 1)),
			"latest" => format!("/manga/newest?offset={}", 20 * i64::from(page.max(1) - 1)),
			_ => bail!("Unknown listing"),
		};
		parser::catalog(&self.document(&path)?, &base_url(), false)
	}
}

impl ImageRequestProvider for HenChan {
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

impl DeepLinkHandler for HenChan {
	fn handle_deep_link(&self, url: String) -> Result<Option<DeepLinkResult>> {
		let Ok(key) = parser::canonical_path(&url, &base_url()) else {
			return Ok(None);
		};
		if key.starts_with("/manga/") && key.ends_with(".html") {
			Ok(Some(DeepLinkResult::Manga { key }))
		} else {
			Ok(None)
		}
	}
}

impl NotificationHandler for HenChan {
	fn handle_notification(&self, notification: String) {
		if notification != "login.changed" {
			return;
		}
		let origin = defaults_get_map("login")
			.filter(|cookies| !cookies.is_empty())
			.map(|_| DefaultValue::String(base_url()))
			.unwrap_or(DefaultValue::Null);
		defaults_set("loginOrigin", origin);
	}
}

register_source!(
	HenChan,
	ListingProvider,
	ImageRequestProvider,
	DeepLinkHandler,
	NotificationHandler
);

#[cfg(test)]
mod tests;
