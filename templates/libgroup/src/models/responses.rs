use aidoku::alloc::{String, Vec};
use serde::{Deserialize, Serialize};

use crate::models::{common::LibGroupMeta, manga::LibGroupCoverItem, user::LibGroupUser};

use super::{
	chapter::{LibGroupChapterData, LibGroupChapterListItem},
	constants::LibGroupConstantsData,
	manga::LibGroupManga,
};

#[derive(Default, Deserialize)]
pub struct UserResponse {
	pub data: LibGroupUser,
}

#[derive(Default, Deserialize)]
pub struct MangaListResponse {
	pub data: Vec<LibGroupManga>,
	pub meta: LibGroupMeta,
}

#[derive(Default, Deserialize)]
pub struct MangaDetailResponse {
	pub data: LibGroupManga,
}

#[derive(Default, Deserialize)]
pub struct ChaptersResponse {
	pub data: Vec<LibGroupChapterListItem>,
}

#[derive(Default, Deserialize)]
pub struct ChapterResponse {
	pub data: Option<LibGroupChapterData>,
}

#[derive(Default, Deserialize)]
pub struct MangaCoversResponse {
	pub data: Vec<LibGroupCoverItem>,
}

#[derive(Default, Deserialize)]
pub struct ConstantsResponse {
	pub data: LibGroupConstantsData,
}

#[derive(Deserialize, Serialize)]
pub struct TokenResponse {
	pub access_token: Option<String>,
	pub refresh_token: Option<String>,
	pub expires_in: Option<i64>,
	pub token_type: Option<String>,
}

#[derive(Deserialize)]
pub struct LocalStorageWrapper {
	pub token: TokenResponse,
}
