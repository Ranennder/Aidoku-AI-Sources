use aidoku::{
	AidokuError, Result,
	alloc::{String, collections::btree_map::BTreeMap},
	imports::{net::Request, std::current_date},
};
use spin::{Once, RwLock};

use crate::{
	auth::AuthRequest, context::Context, endpoints::Url, json::ResponseJsonExt,
	models::responses::ConstantsResponse, settings::get_image_server_url,
};

const FAILURE_COOLDOWN_SECONDS: i64 = 30;

struct CacheEntry {
	data: BTreeMap<u8, BTreeMap<String, String>>,
	created_at: i64,
}

impl CacheEntry {
	fn new(data: BTreeMap<u8, BTreeMap<String, String>>, now: i64) -> Self {
		Self {
			data,
			created_at: now,
		}
	}

	fn is_expired(&self, now: i64, ttl_seconds: i64) -> bool {
		now - self.created_at > ttl_seconds
	}
}

pub struct ImageServerCache {
	cache: RwLock<Option<CacheEntry>>,
	ttl_seconds: i64,
	now_fn: fn() -> i64,
}

impl ImageServerCache {
	pub fn new_with_ttl(ttl_seconds: i64, now_fn: fn() -> i64) -> Self {
		Self {
			cache: RwLock::new(None),
			ttl_seconds,
			now_fn,
		}
	}

	pub fn new() -> Self {
		fn now() -> i64 {
			current_date()
		}
		Self::new_with_ttl(3600, now)
	}

	pub fn get_base_url(&self, ctx: &Context) -> Result<String> {
		self.get_base_url_with(ctx.site_id, &get_image_server_url(), || self.load_data(ctx))
	}

	fn get_base_url_with(
		&self,
		site_id: u8,
		selected_id: &str,
		load: impl FnOnce() -> Result<BTreeMap<u8, BTreeMap<String, String>>>,
	) -> Result<String> {
		let now = (self.now_fn)();

		// 1. Check cache
		{
			let guard = self.cache.read();
			if let Some(ref entry) = *guard
				&& !entry.is_expired(now, self.ttl_seconds)
			{
				return self.extract_url(&entry.data, &site_id, selected_id);
			}
		}

		// 2. Fetch and update
		let mut guard = self.cache.write();

		// Validate the selected server before replacing a previously usable entry.
		let loaded = load().and_then(|data| {
			let url = self.extract_url(&data, &site_id, selected_id)?;
			Ok((data, url))
		});
		match loaded {
			Ok((data, url)) => {
				// Success: Update cache with new data and fresh timestamp
				let entry = CacheEntry::new(data, now);
				*guard = Some(entry);

				Ok(url)
			}
			Err(error) => {
				// Failure backoff
				if let Some(ref mut entry) = *guard {
					entry.created_at = now - self.ttl_seconds + FAILURE_COOLDOWN_SECONDS;

					self.extract_url(&entry.data, &site_id, selected_id)
				} else {
					Err(error)
				}
			}
		}
	}

	fn load_data(&self, ctx: &Context) -> Result<BTreeMap<u8, BTreeMap<String, String>>> {
		let constants_url = Url::constants_with_fields(&ctx.api_url, &["imageServers"]);

		let response = Request::get(constants_url)?
			.authed(ctx)?
			.parse_json::<ConstantsResponse>()?;

		let mut servers_by_site: BTreeMap<u8, BTreeMap<String, String>> = BTreeMap::new();

		for server in response.data.image_servers.unwrap_or_default() {
			for &site_id in &server.site_ids {
				servers_by_site
					.entry(site_id)
					.or_default()
					.insert(server.id.clone(), server.url.clone());
			}
		}

		Ok(servers_by_site)
	}

	fn extract_url(
		&self,
		data: &BTreeMap<u8, BTreeMap<String, String>>,
		site_id: &u8,
		server_id: &str,
	) -> Result<String> {
		data.get(site_id)
			.and_then(|site_servers| site_servers.get(server_id))
			.filter(|url| url.starts_with("https://") || url.starts_with("http://"))
			.cloned()
			.ok_or_else(|| {
				AidokuError::message(
					"MangaLib: сервер изображений недоступен. Повторите попытку позже.",
				)
			})
	}
}

static IMAGE_SERVER_CACHE: Once<ImageServerCache> = Once::new();

pub fn get_image_server_cache() -> &'static ImageServerCache {
	IMAGE_SERVER_CACHE.call_once(ImageServerCache::new)
}

pub fn get_selected_image_server_url(ctx: &Context) -> Result<String> {
	get_image_server_cache().get_base_url(ctx)
}

#[cfg(test)]
mod test;
