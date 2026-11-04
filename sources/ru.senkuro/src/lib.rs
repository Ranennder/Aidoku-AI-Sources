#![no_std]

mod graphql;
mod helpers;
mod models;

use aidoku::{
	Chapter, DeepLinkHandler, DeepLinkResult, FilterValue, Home, HomeLayout, Listing,
	ListingProvider, Manga, MangaPageResult, Page, Result, Source,
	alloc::{String, Vec},
	imports::std::send_partial_result,
	prelude::*,
};

use crate::graphql::{
	CHAPTERS_QUERY, ChaptersVariables, HOME_LATEST_QUERY, HOME_NEW_QUERY, HOME_POPULAR_QUERY,
	HomeVariables, MANGA_QUERY, OrderBy, READER_QUERY, ReaderVariables, SEARCH_QUERY,
	SearchVariables, SlugVariables,
};
use crate::models::{
	ChapterConnection, ChaptersData, HomeMangasData, MangaData, MangaInfo, ReaderData, SearchData,
};

const BASE_URL: &str = "https://senkuro.me";
const API_URL: &str = "https://api.senkuro.me/graphql";
const AUTH_KEY: &str = "senkuro_login";
const APP_ID: &str = "4026531840100";

struct Senkuro;

impl Senkuro {
	fn home_manga_page(&self, query: &str, page: i32) -> Result<MangaPageResult> {
		let mut cursor: Option<String> = None;
		let mut current_page = 1;
		loop {
			let data: HomeMangasData = self.graphql(
				query,
				HomeVariables {
					after: cursor.as_deref(),
				},
			)?;
			let connection = data.mangas;
			let has_next_page = connection.page_info.has_next_page;
			let next_cursor = connection.page_info.end_cursor;

			if current_page == page {
				return Ok(MangaPageResult {
					entries: connection
						.edges
						.into_iter()
						.map(|edge| Self::home_manga(edge.node))
						.collect(),
					has_next_page,
				});
			}

			if !has_next_page {
				return Ok(MangaPageResult {
					entries: Vec::new(),
					has_next_page: false,
				});
			}

			cursor = Some(
				next_cursor.ok_or_else(|| error!("Senkuro: Home listing has no next cursor"))?,
			);
			current_page += 1;
		}
	}

	fn chapter_list(&self, manga: &MangaInfo) -> Result<Vec<Chapter>> {
		let branch = manga
			.branches
			.iter()
			.find(|branch| branch.primary_branch)
			.or_else(|| manga.branches.first())
			.ok_or_else(|| error!("Senkuro: у тайтла нет ветки перевода"))?;

		let mut chapters = Vec::new();
		let mut cursor: Option<String> = None;

		loop {
			let data: ChaptersData = self.graphql(
				CHAPTERS_QUERY,
				ChaptersVariables {
					branch_id: &branch.id,
					number: None,
					after: cursor.as_deref(),
					order_by: OrderBy {
						field: "NUMBER",
						direction: "DESC",
					},
				},
			)?;

			let ChapterConnection { edges, page_info } = data.manga_chapters;
			let has_next_page = page_info.has_next_page;
			let next_cursor = page_info.end_cursor;

			chapters.extend(edges.into_iter().map(|edge| {
				let chapter = edge.node;
				let scanlators = branch
					.team_activities
					.iter()
					.filter(|activity| chapter.team_ids.contains(&activity.team.id))
					.map(|activity| activity.team.name.clone())
					.collect::<Vec<_>>();

				Chapter {
					key: chapter.slug,
					title: chapter.name,
					chapter_number: chapter.number.parse::<f32>().ok(),
					volume_number: chapter.volume.parse::<f32>().ok(),
					scanlators: if scanlators.is_empty() {
						None
					} else {
						Some(scanlators)
					},
					url: Some(format!("{BASE_URL}/manga/{}/chapters", manga.slug)),
					locked: false,
					..Default::default()
				}
			}));

			if !has_next_page {
				break;
			}

			let next_cursor = next_cursor
				.ok_or_else(|| error!("Senkuro: у списка глав нет следующего курсора"))?;
			if cursor.as_deref() == Some(next_cursor.as_str()) {
				return Err(error!("Senkuro: пагинация списка глав зациклилась"));
			}
			cursor = Some(next_cursor);
		}

		Ok(chapters)
	}
}

impl Source for Senkuro {
	fn new() -> Self {
		Self
	}

	fn get_search_manga_list(
		&self,
		query: Option<String>,
		_page: i32,
		_filters: Vec<FilterValue>,
	) -> Result<MangaPageResult> {
		let Some(query) = query else {
			return Ok(MangaPageResult::default());
		};

		let data: SearchData = self.graphql(
			SEARCH_QUERY,
			SearchVariables {
				query: &query,
				search_type: "MANGA",
			},
		)?;

		Ok(MangaPageResult {
			entries: data
				.search
				.edges
				.into_iter()
				.map(|edge| self.search_manga(edge.node))
				.collect(),
			has_next_page: false,
		})
	}

	fn get_manga_update(
		&self,
		mut manga: Manga,
		needs_details: bool,
		needs_chapters: bool,
	) -> Result<Manga> {
		let data: MangaData = self.graphql(MANGA_QUERY, SlugVariables { slug: &manga.key })?;
		let mut remote = data.manga;

		if needs_details {
			manga.title = Self::title(&remote.original_name.content, &remote.titles);
			manga.cover = Self::cover(remote.cover.take());
			manga.url = Some(format!("{BASE_URL}/manga/{}", remote.slug));
			manga.status = Self::status(&remote.manga_status);
			manga.content_rating = Self::rating(&remote.rating);
			let authors =
				Self::staff_names(&remote.main_staff, &["AUTHOR", "WRITER", "STORY", "SCRIPT"]);
			manga.authors = if authors.is_empty() {
				None
			} else {
				Some(authors)
			};
			let artists = Self::staff_names(
				&remote.main_staff,
				&["ART", "ARTIST", "ILLUSTRATOR", "DRAWER"],
			);
			manga.artists = if artists.is_empty() {
				None
			} else {
				Some(artists)
			};
			manga.description = self.fetch_description(&remote.slug);

			let mut tags = Vec::new();
			for label in &remote.labels {
				let label_title = Self::title(&label.slug, &label.titles);
				if !label_title.is_empty() {
					tags.push(label_title);
				}
			}
			manga.tags = Some(tags);
		}

		if needs_chapters {
			if needs_details {
				send_partial_result(&manga);
			}
			manga.chapters = Some(self.chapter_list(&remote)?);
		}

		Ok(manga)
	}

	fn get_page_list(&self, _manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let data: ReaderData = self.graphql(
			READER_QUERY,
			ReaderVariables {
				slug: &chapter.key,
				cdn_quality: "red",
			},
		)?;

		Self::reader_pages(data)
	}
}

impl ListingProvider for Senkuro {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		match listing.id.as_str() {
			"latest-updates" => self.home_manga_page(HOME_LATEST_QUERY, page),
			"new-titles" => self.home_manga_page(HOME_NEW_QUERY, page),
			"popular-day" => Ok(MangaPageResult {
				entries: self.home_static_list(HOME_POPULAR_QUERY)?,
				has_next_page: false,
			}),
			"recommendations" => Ok(MangaPageResult {
				entries: self.home_recommendations()?,
				has_next_page: false,
			}),
			_ => bail!("Senkuro: неизвестная Home-секция"),
		}
	}
}

impl Home for Senkuro {
	fn get_home(&self) -> Result<HomeLayout> {
		let latest = self.home_manga_page(HOME_LATEST_QUERY, 1)?.entries;
		let popular = self.home_static_list(HOME_POPULAR_QUERY)?;
		let new_titles = self.home_manga_page(HOME_NEW_QUERY, 1)?.entries;

		if latest.is_empty() || popular.is_empty() || new_titles.is_empty() {
			bail!("Senkuro: Home не вернула обязательные секции");
		}

		let mut components = Vec::new();
		components.push(Self::home_component(
			"Свежие обновления",
			"latest-updates",
			latest,
		));
		components.push(Self::home_component(
			"Популярное за день",
			"popular-day",
			popular,
		));
		components.push(Self::home_component(
			"Новые тайтлы",
			"new-titles",
			new_titles,
		));

		let recommendations = self.home_recommendations()?;
		if !recommendations.is_empty() {
			components.push(Self::home_component(
				"Рекомендации",
				"recommendations",
				recommendations,
			));
		}

		Ok(HomeLayout { components })
	}
}

impl DeepLinkHandler for Senkuro {
	fn handle_deep_link(&self, url: String) -> Result<Option<DeepLinkResult>> {
		let prefix = format!("{BASE_URL}/");
		let Some(path) = url.strip_prefix(prefix.as_str()) else {
			return Ok(None);
		};

		let mut parts = path.split('/').filter(|part| !part.is_empty());
		if parts.next() != Some("manga") {
			return Ok(None);
		}

		let Some(slug) = parts.next().and_then(|slug| slug.split(['?', '#']).next()) else {
			return Ok(None);
		};

		Ok(Some(DeepLinkResult::Manga { key: slug.into() }))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::graphql::GraphqlResponse;
	use aidoku_test::aidoku_test;

	#[aidoku_test]
	fn decodes_senkuro_chapter_detail_contract() {
		let payload = r#"{
          "data": {
            "manga": {
              "slug": "demo",
              "original_name": {"lang": "EN", "content": "Demo"},
              "titles": [],
              "manga_status": "ONGOING",
              "rating": "EXPLICIT",
              "mainStaff": [],
              "branches": [{"id": "branch-1", "primaryBranch": true, "teamActivities": []}],
              "cover": null,
              "labels": []
            }
          }
        }"#;

		let parsed: GraphqlResponse<MangaData> = serde_json::from_str(payload).unwrap();
		assert_eq!(parsed.data.unwrap().manga.branches[0].id, "branch-1");
	}

	#[aidoku_test]
	fn localized_metadata_prefers_russian_and_preserves_original_fallback() {
		let titles = aidoku::alloc::vec![
			crate::models::Localized {
				lang: "EN".into(),
				content: "English".into()
			},
			crate::models::Localized {
				lang: "RU".into(),
				content: "Русское название".into()
			},
		];
		assert_eq!(Senkuro::title("Original", &titles), "Русское название");
		assert_eq!(Senkuro::title("Original", &[]), "Original");
		assert_eq!(Senkuro::status("FINISHED"), aidoku::MangaStatus::Completed);
		assert_eq!(Senkuro::rating("GENERAL"), aidoku::ContentRating::Safe);
	}

	#[aidoku_test]
	fn reader_preserves_page_order_and_rejects_missing_images_without_truncating() {
		let payload = r#"{"mangaChapter":{"pages":[{"image":{"original":{"url":"https://images.example/2.jpg?v=1"}}},{"image":{"original":{"url":"https://images.example/1.gif"}}}]}}"#;
		let pages = Senkuro::reader_pages(serde_json::from_str(payload).unwrap()).unwrap();
		assert_eq!(
			pages[0].content,
			aidoku::PageContent::url("https://images.example/2.jpg?v=1")
		);
		assert_eq!(
			pages[1].content,
			aidoku::PageContent::url("https://images.example/1.gif")
		);
		for payload in [
			r#"{"mangaChapter":{"pages":[]}}"#,
			r#"{"mangaChapter":{"pages":[{"image":null}]}}"#,
		] {
			assert!(Senkuro::reader_pages(serde_json::from_str(payload).unwrap()).is_err());
		}
	}

	#[aidoku_test]
	fn chapter_pagination_decodes_continuation_without_losing_decimal_identity() {
		let data: ChaptersData = serde_json::from_str(r#"{"mangaChapters":{"edges":[{"node":{"slug":"exact-id","team_ids":["team-a"],"name":null,"number":"12.50","volume":"2"}}],"pageInfo":{"hasNextPage":true,"endCursor":"next-page"}}}"#).unwrap();
		assert_eq!(data.manga_chapters.edges[0].node.slug, "exact-id");
		assert_eq!(data.manga_chapters.edges[0].node.number, "12.50");
		assert!(data.manga_chapters.page_info.has_next_page);
		assert_eq!(
			data.manga_chapters.page_info.end_cursor.as_deref(),
			Some("next-page")
		);
	}

	#[aidoku_test]
	fn deep_links_require_the_actual_site_host_and_keep_query_out_of_keys() {
		let source = Senkuro;
		assert_eq!(
			source
				.handle_deep_link(format!("{BASE_URL}/manga/demo?from=link#top"))
				.unwrap(),
			Some(DeepLinkResult::Manga { key: "demo".into() })
		);
		assert_eq!(
			source
				.handle_deep_link(format!("{BASE_URL}.example/manga/demo"))
				.unwrap(),
			None
		);
	}
}

register_source!(Senkuro, ListingProvider, Home, DeepLinkHandler);
