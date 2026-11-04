#![no_std]
use aidoku::prelude::*;
use grouple::{GroupLe, Site};
struct AllHentai;
impl Site for AllHentai {
	const URL: &'static str = "https://20.allhen.online";
	const ADULT: bool = true;
}
register_source!(
	GroupLe<AllHentai>,
	ListingProvider,
	ImageRequestProvider,
	DeepLinkHandler
);
