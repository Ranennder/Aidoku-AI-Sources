use aidoku::{AidokuError, Result, imports::net::Response, prelude::*};
use serde::de::DeserializeOwned;

pub trait ResponseJsonExt {
	fn parse_json<T: DeserializeOwned>(self) -> Result<T>;
}

impl ResponseJsonExt for Response {
	fn parse_json<T: DeserializeOwned>(self) -> Result<T> {
		let status = self.status_code();
		let bytes = self.get_data()?;
		decode_response(status, &bytes)
	}
}

/// Error envelopes can resemble partial manga records. Never turn an HTTP
/// failure into a successful empty update that replaces existing metadata.
fn decode_response<T: DeserializeOwned>(status: i32, bytes: &[u8]) -> Result<T> {
	if !(200..300).contains(&status) {
		let message = match status {
			401 => "Войдите в SocialLib в настройках источника и повторите запрос.",
			403 => "Проверьте доступ к сайту через WebView источника.",
			404 => "Материал недоступен или удалён.",
			429 => "Слишком много запросов. Подождите и повторите.",
			_ => "Повторите запрос позже.",
		};
		return Err(AidokuError::message(format!(
			"MangaLib HTTP {status}. {message}"
		)));
	}
	serde_json::from_slice::<T>(bytes).map_err(|error| {
		AidokuError::message(format!(
			"MangaLib: некорректный JSON (строка {}, столбец {}, {} байт).",
			error.line(),
			error.column(),
			bytes.len()
		))
	})
}

#[cfg(test)]
mod test;
