use aidoku::{
	HashMap,
	alloc::{String, Vec},
	imports::defaults::defaults_get_map,
	prelude::*,
};

use crate::parser::BASE_URL;

pub fn stored_login_cookie() -> Option<String> {
	cookie_header(defaults_get_map("mangabuffLogin")?)
}

/// The login WebView stores cookies separately from URLSession. Transfer only
/// MangaBuff's session/remember-me credentials, never third-party tracking data.
/// CSRF comes from the current HTML, and URLSession manages its own guard cookies.
pub fn cookie_header(cookies: HashMap<String, String>) -> Option<String> {
	let mut cookies: Vec<_> = cookies
		.into_iter()
		.filter(|(name, value)| {
			(name == "mangabuff_session" || name.starts_with("remember_web_"))
				&& name
					.bytes()
					.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
				&& !value.is_empty()
				&& value.bytes().all(|byte| {
					(0x21..=0x7e).contains(&byte) && !matches!(byte, b';' | b',' | b'"' | b'\\')
				})
		})
		.collect();
	// The map order is unspecified. A stable snapshot prevents unrelated order
	// changes from invalidating an otherwise current CSRF token.
	cookies.sort_by(|left, right| left.0.cmp(&right.0));
	if cookies.is_empty() {
		return None;
	}
	Some(
		cookies
			.into_iter()
			.map(|(name, value)| format!("{name}={value}"))
			.collect::<Vec<_>>()
			.join("; "),
	)
}

pub fn cookie_for_url(url: &str, cookie: Option<String>) -> Option<String> {
	if url == BASE_URL
		|| url
			.strip_prefix(BASE_URL)
			.is_some_and(|path| path.starts_with('/'))
	{
		cookie
	} else {
		None
	}
}
