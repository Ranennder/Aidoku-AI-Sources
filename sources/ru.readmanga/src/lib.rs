#![no_std]
use aidoku::prelude::*;
use grouple::{GroupLe, Site};
struct ReadManga;
impl Site for ReadManga {
	const URL: &'static str = "https://a.zazaza.me";
	const ADULT: bool = false;
}
register_source!(
	GroupLe<ReadManga>,
	ListingProvider,
	ImageRequestProvider,
	DeepLinkHandler
);
