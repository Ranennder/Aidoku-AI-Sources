use super::*;
use aidoku::alloc::{String, collections::btree_map::BTreeMap, string::ToString};
use aidoku_test::aidoku_test;

fn mock_now() -> i64 {
	1_000_000
}

fn make_cache_with_ttl(ttl: i64) -> ImageServerCache {
	ImageServerCache::new_with_ttl(ttl, mock_now)
}

#[aidoku_test]
fn cache_hit_returns_same_url() {
	let cache = make_cache_with_ttl(3600);
	let mut servers = BTreeMap::new();
	let mut inner = BTreeMap::new();
	inner.insert("server1".to_string(), "http://img.server/1".to_string());
	servers.insert(1u8, inner);

	{
		let mut guard = cache.cache.write();
		*guard = Some(CacheEntry::new(servers.clone(), mock_now()));
	}

	let url = cache.extract_url(&servers, &1u8, "server1").unwrap();
	assert_eq!(url, "http://img.server/1");
}

#[aidoku_test]
fn expired_entry_detected() {
	let cache = make_cache_with_ttl(1); // 1 second TTL

	let mut servers = BTreeMap::new();
	let mut inner = BTreeMap::new();
	inner.insert("server1".to_string(), "http://img.server/1".to_string());
	servers.insert(1u8, inner);

	// Seed with time 10s in past (expired)
	{
		let mut guard = cache.cache.write();
		*guard = Some(CacheEntry::new(servers.clone(), mock_now() - 10));
	}

	// Expiration is tested without DNS or HTTP, so the default suite stays offline.
	let guard = cache.cache.read();
	assert!(
		guard
			.as_ref()
			.unwrap()
			.is_expired(mock_now(), cache.ttl_seconds)
	);
}

#[aidoku_test]
fn missing_server_is_an_error_instead_of_an_empty_page_prefix() {
	let cache = make_cache_with_ttl(3600);
	let servers: BTreeMap<u8, BTreeMap<String, String>> = BTreeMap::new();
	assert!(cache.extract_url(&servers, &99u8, "server1").is_err());
}

fn servers() -> BTreeMap<u8, BTreeMap<String, String>> {
	let mut servers = BTreeMap::new();
	let mut site = BTreeMap::new();
	site.insert("compress".to_string(), "https://images.example".to_string());
	servers.insert(1, site);
	servers
}

#[aidoku_test]
fn cold_constants_failure_does_not_create_a_successful_empty_server_cache() {
	let cache = make_cache_with_ttl(3600);
	assert!(
		cache
			.get_base_url_with(1, "compress", || {
				Err(aidoku::AidokuError::message("HTTP 503"))
			})
			.is_err()
	);
	assert!(cache.cache.read().is_none());
	let recovered = cache
		.get_base_url_with(1, "compress", || Ok(servers()))
		.unwrap();
	assert_eq!(recovered, "https://images.example");
}

#[aidoku_test]
fn expired_valid_server_survives_a_transient_failure_without_repeated_requests() {
	let cache = make_cache_with_ttl(3600);
	*cache.cache.write() = Some(CacheEntry::new(servers(), mock_now() - 3601));
	let fallback = cache
		.get_base_url_with(1, "compress", || {
			Err(aidoku::AidokuError::message("HTTP 503"))
		})
		.unwrap();
	assert_eq!(fallback, "https://images.example");
	assert_eq!(
		cache
			.get_base_url_with(1, "compress", || {
				panic!("failure cooldown must retain the last valid server")
			})
			.unwrap(),
		fallback
	);
}

#[aidoku_test]
fn incomplete_constants_cannot_replace_a_previously_valid_image_server() {
	let cache = make_cache_with_ttl(3600);
	assert!(
		cache
			.get_base_url_with(1, "compress", || Ok(BTreeMap::new()))
			.is_err()
	);
	*cache.cache.write() = Some(CacheEntry::new(servers(), mock_now() - 3601));
	let fallback = cache
		.get_base_url_with(1, "compress", || Ok(BTreeMap::new()))
		.unwrap();
	assert_eq!(fallback, "https://images.example");
}
