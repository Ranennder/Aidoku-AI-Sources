// MangaBuff's site protocol/selectors are adapted from Keiyoushi's Apache-2.0
// MangaBuff extension. See the repository NOTICE for pinned upstream provenance.
#![no_std]

use aidoku::{
	Chapter, DeepLinkHandler, DeepLinkResult, FilterValue, ImageRequestProvider, Listing,
	ListingProvider, Manga, MangaPageResult, Page, PageContext, Result, Source,
	alloc::{String, Vec},
	imports::{
		html::{Document, Html},
		net::{Request, Response, TimeUnit, set_rate_limit},
	},
	prelude::*,
};
use core::cell::RefCell;
use serde::Deserialize;

mod parser;
use parser::{BASE_URL, canonical_path, parse_catalog, parse_chapters, parse_details, parse_pages};

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/132.0.0.0 Safari/537.36";

struct MangaBuff {
	csrf_token: RefCell<Option<String>>,
}

impl MangaBuff {
	fn request(url: &str) -> Result<Request> {
		Ok(Request::get(url)?
			.header("Referer", &format!("{BASE_URL}/"))
			.header("User-Agent", USER_AGENT)
			.timeout(30.0))
	}

	fn document(&self, path: &str) -> Result<Document> {
		let response = Self::request(&format!("{BASE_URL}{path}"))?.send()?;
		parser::check_status(response.status_code())?;
		let document = response.get_html()?;
		self.remember_token(&document);
		Ok(document)
	}

	fn remember_token(&self, document: &Document) {
		if let Some(token) = parser::csrf_token(document) {
			*self.csrf_token.borrow_mut() = Some(token);
		}
	}

	fn token(&self) -> Result<String> {
		if let Some(token) = self.csrf_token.borrow().clone() {
			return Ok(token);
		}
		self.document("/")?;
		self.csrf_token.borrow().clone().ok_or_else(|| {
			error!(
				"MangaBuff: не найден CSRF-токен. Откройте сайт в WebView источника и повторите."
			)
		})
	}

	fn chapters_response(&self, manga_id: &str) -> Result<Response> {
		let token = self.token()?;
		Ok(Request::post(format!("{BASE_URL}/chapters/load"))?
			.header("User-Agent", USER_AGENT)
			.header("Referer", &format!("{BASE_URL}/"))
			.header("Origin", BASE_URL)
			.header("X-Requested-With", "XMLHttpRequest")
			.header("X-CSRF-TOKEN", &token)
			.header(
				"Content-Type",
				"application/x-www-form-urlencoded; charset=UTF-8",
			)
			.body(format!("manga_id={manga_id}"))
			.timeout(30.0)
			.send()?)
	}

	fn remaining_chapters(&self, manga_id: &str) -> Result<Vec<Chapter>> {
		// Laravel rejects an expired CSRF session before doing this read-only
		// operation. Refresh once on 419; never repeat transport failures.
		let response = self.chapters_response(manga_id)?;
		let response = if response.status_code() == 419 {
			drop(response);
			*self.csrf_token.borrow_mut() = None;
			self.chapters_response(manga_id)?
		} else {
			response
		};
		parser::check_status(response.status_code())?;
		#[derive(Deserialize)]
		struct ChapterResponse {
			content: String,
		}
		let data: ChapterResponse = response.get_json_owned()?;
		let document = Html::parse_fragment_with_url(data.content, BASE_URL)?;
		parse_chapters(&document)
	}
}

impl Source for MangaBuff {
	fn new() -> Self {
		// Source API calls are paced; image requests are returned to the app's
		// image pipeline rather than downloaded through the WASM network API.
		set_rate_limit(1, 1, TimeUnit::Seconds);
		Self {
			csrf_token: RefCell::new(None),
		}
	}

	fn get_search_manga_list(
		&self,
		query: Option<String>,
		page: i32,
		_filters: Vec<FilterValue>,
	) -> Result<MangaPageResult> {
		let path = parser::search_path(query.as_deref(), page)?;
		parse_catalog(&self.document(&path)?)
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
		let path = canonical_path(&manga.key, false)?;
		let document = self.document(&path)?;
		if needs_details {
			manga = parse_details(&document, manga)?;
		}
		if needs_chapters {
			let mut chapters = parse_chapters(&document)?;
			if document.select_first(".load-chapters-trigger").is_some() {
				let manga_id = document
					.select_first(".manga")
					.and_then(|element| element.attr("data-id"))
					.filter(|id| !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit()))
					.ok_or_else(|| error!("MangaBuff: не найден ID для загрузки всех глав."))?;
				let remaining = self.remaining_chapters(&manga_id)?;
				if remaining.is_empty() {
					bail!(
						"MangaBuff: сайт не вернул оставшиеся главы. Повторите обновление списка глав."
					);
				}
				parser::append_unique_chapters(&mut chapters, remaining);
			}
			manga.chapters = Some(chapters);
		}
		Ok(manga)
	}

	fn get_page_list(&self, _manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let path = canonical_path(&chapter.key, true)?;
		parse_pages(&self.document(&path)?, &format!("{BASE_URL}{path}"))
	}
}

impl ListingProvider for MangaBuff {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		let sort = match listing.id.as_str() {
			"popular" => "real_views",
			"latest" => "updated_at",
			_ => bail!("MangaBuff: неизвестный каталог."),
		};
		parse_catalog(&self.document(&parser::catalog_path(sort, page))?)
	}
}

impl ImageRequestProvider for MangaBuff {
	fn get_image_request(&self, url: String, context: Option<PageContext>) -> Result<Request> {
		let referer = context
			.as_ref()
			.and_then(|context| context.get("referer"))
			.and_then(|value| canonical_path(value, true).ok())
			.map(|path| format!("{BASE_URL}{path}"))
			.unwrap_or_else(|| format!("{BASE_URL}/"));
		Ok(Request::get(url)?
			.header("Referer", &referer)
			.header("User-Agent", USER_AGENT))
	}
}

impl DeepLinkHandler for MangaBuff {
	fn handle_deep_link(&self, url: String) -> Result<Option<DeepLinkResult>> {
		if let Ok(key) = canonical_path(&url, false) {
			return Ok(Some(DeepLinkResult::Manga { key }));
		}
		if let Ok(key) = canonical_path(&url, true) {
			let mut segments = key.trim_start_matches('/').split('/');
			let manga_key = format!("/manga/{}", segments.nth(1).unwrap_or_default());
			return Ok(Some(DeepLinkResult::Chapter { manga_key, key }));
		}
		Ok(None)
	}
}

register_source!(
	MangaBuff,
	ListingProvider,
	ImageRequestProvider,
	DeepLinkHandler
);

#[cfg(test)]
mod tests;
