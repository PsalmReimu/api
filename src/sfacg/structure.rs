use chrono::NaiveDateTime;
use http::StatusCode;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::Error;

#[must_use]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Status {
    pub http_code: u16,
    pub error_code: u16,
    pub msg: Option<String>,
}

impl Status {
    #[must_use]
    pub(crate) fn ok(&self) -> bool {
        self.http_code == StatusCode::OK && self.error_code == 200
    }

    #[must_use]
    pub(crate) fn not_found(&self) -> bool {
        self.http_code == StatusCode::NOT_FOUND && self.error_code == 404
    }

    #[must_use]
    pub(crate) fn unauthorized(&self) -> bool {
        self.http_code == StatusCode::UNAUTHORIZED && self.error_code == 502
    }

    pub(crate) fn check(&self) -> Result<(), Error> {
        if !self.ok() {
            return Err(Error::Http {
                code: StatusCode::from_u16(self.http_code)?,
                msg: self.msg.clone().unwrap(),
            })?;
        }

        Ok(())
    }
}

#[must_use]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LoginRequest {
    pub user_name: String,
    pub pass_word: String,
}

#[must_use]
#[derive(Deserialize)]
pub(crate) struct LoginResponse {
    pub status: Status,
}

#[must_use]
#[derive(Deserialize)]
pub(crate) struct PositionResponse {
    pub status: Status,
}

#[must_use]
#[derive(Deserialize)]
pub(crate) struct UserResponse {
    pub status: Status,
    pub data: Option<UserData>,
}

#[must_use]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UserData {
    pub nick_name: String,
}

#[must_use]
#[derive(Serialize)]
pub(crate) struct NovelsRequest {
    pub expand: Option<&'static str>,
}

#[must_use]
#[derive(Deserialize)]
pub(crate) struct NovelsResponse {
    pub status: Status,
    pub data: Option<NovelsData>,
}

#[must_use]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NovelsData {
    pub novel_name: String,
    pub novel_cover: Url,
    pub author_name: String,
    pub char_count: i32,
    pub is_finish: bool,
    pub add_time: NaiveDateTime,
    pub last_update_time: NaiveDateTime,
    pub expand: NovelsExpand,
}

#[must_use]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NovelsExpand {
    pub type_name: String,
    pub intro: String,
    pub sys_tags: Vec<NovelsSysTag>,
}

#[must_use]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NovelsSysTag {
    pub sys_tag_id: u16,
    pub tag_name: String,
}

#[must_use]
#[derive(Deserialize)]
pub(crate) struct NovelsDirsResponse {
    pub status: Status,
    pub data: Option<NovelsDirsData>,
}

#[must_use]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NovelsDirsData {
    pub volume_list: Vec<NovelsDirsVolumeInfo>,
}

#[must_use]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NovelsDirsVolumeInfo {
    pub title: String,
    pub chapter_list: Vec<NovelsDirsChapterInfo>,
}

#[must_use]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NovelsDirsChapterInfo {
    pub chap_id: u32,
    pub title: String,
    pub char_count: i16,
    pub is_vip: bool,
    pub need_fire_money: i16,
    #[serde(rename = "AddTime")]
    pub add_time: NaiveDateTime,
    pub update_time: Option<NaiveDateTime>,
}

#[must_use]
#[derive(Serialize)]
pub(crate) struct ChapsRequest {
    pub expand: Option<&'static str>,
}

#[must_use]
#[derive(Deserialize)]
pub(crate) struct ChapsResponse {
    pub status: Status,
    pub data: Option<ChapsData>,
}

#[must_use]
#[derive(Deserialize)]
pub(crate) struct ChapsData {
    pub expand: ChapsExpand,
}

#[must_use]
#[derive(Deserialize)]
pub(crate) struct ChapsExpand {
    pub content: String,
}

#[must_use]
#[derive(Serialize)]
pub(crate) struct SearchRequest {
    pub expand: Option<&'static str>,
    pub page: u16,
    pub q: String,
    pub size: u16,
    pub sort: &'static str,
}

#[must_use]
#[derive(Deserialize)]
pub(crate) struct SearchResponse {
    pub status: Status,
    pub data: Option<SearchData>,
}

#[must_use]
#[derive(Deserialize)]
pub(crate) struct SearchData {
    pub novels: Vec<SearchNovelInfo>,
}

#[must_use]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SearchNovelInfo {
    pub novel_id: u32,
}

#[must_use]
#[derive(Serialize)]
pub(crate) struct FavoritesRequest {
    pub expand: Option<&'static str>,
}

#[must_use]
#[derive(Deserialize)]
pub(crate) struct FavoritesResponse {
    pub status: Status,
    pub data: Option<Vec<FavoritesData>>,
}

#[must_use]
#[derive(Deserialize)]
pub(crate) struct FavoritesData {
    pub expand: FavoritesExpand,
}

#[must_use]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum FavoritesExpand {
    Novels(Vec<FavoritesNovelInfo>),
    Albums(Vec<FavoritesNovelInfo>),
    Comics(Vec<FavoritesNovelInfo>),
}

#[must_use]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FavoritesNovelInfo {
    pub novel_id: u32,
}
