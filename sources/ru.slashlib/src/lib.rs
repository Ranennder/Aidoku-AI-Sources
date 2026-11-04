#![no_std]
use aidoku::{Source, alloc::borrow::Cow, prelude::*};
use libgroup::{Impl, LibGroup, Params};

struct SlashLib;

impl Impl for SlashLib {
	fn new() -> Self {
		Self
	}

	fn params(&self) -> Params {
		Params {
			site_id: Cow::Owned(2),
		}
	}
}

register_source!(
	LibGroup<SlashLib>,
	ListingProvider,
	Home,
	ImageRequestProvider,
	AlternateCoverProvider,
	MigrationHandler,
	NotificationHandler
);

#[cfg(test)]
mod tests {
	use super::*;
	use aidoku_test::aidoku_test;

	#[aidoku_test]
	fn slashlib_uses_its_own_site_in_the_shared_library_protocol() {
		let source = SlashLib::new();
		assert_eq!(*source.params().site_id, 2);
		assert_eq!(
			serde_json::from_str::<serde_json::Value>(include_str!("../res/source.json")).unwrap()
				["info"]["id"],
			"ru.slashlib"
		);
	}
}
