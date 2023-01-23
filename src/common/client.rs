use std::path::Path;

use async_trait::async_trait;
use chrono::NaiveDateTime;
use image::DynamicImage;
use url::Url;

use crate::Error;

#[must_use]
#[derive(Debug)]
pub struct UserInfo {
    pub nick_name: String,
}

#[must_use]
#[derive(Debug, Default)]
pub struct NovelInfo {
    pub id: u32,
    pub name: String,
    pub author_name: String,
    pub cover_url: Option<Url>,
    pub introduction: Option<Vec<String>>,
    pub word_count: Option<u32>,
    pub finished: Option<bool>,
    pub add_time: Option<NaiveDateTime>,
    pub update_time: Option<NaiveDateTime>,
    pub type_name: Option<String>,
    pub tags: Option<Vec<Tag>>,
}

impl PartialEq for NovelInfo {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

#[must_use]
#[derive(Debug)]
pub struct Tag {
    pub id: Option<u16>,
    pub name: String,
}

pub type VolumeInfos = Vec<VolumeInfo>;

#[must_use]
#[derive(Debug)]
pub struct VolumeInfo {
    pub id: Option<u32>,
    pub title: String,
    pub chapter_infos: Vec<ChapterInfo>,
}

#[must_use]
#[derive(Debug)]
pub struct ChapterInfo {
    pub identifier: Identifier,
    pub title: String,
    pub is_vip: Option<bool>,
    pub auth_access: Option<bool>,
    pub is_valid: Option<bool>,
    pub word_count: Option<u16>,
    pub time: Option<NaiveDateTime>,
}

#[must_use]
#[derive(Debug)]
pub enum Identifier {
    Id(u32),
    Url(Url),
}

impl ToString for Identifier {
    fn to_string(&self) -> String {
        match self {
            Identifier::Id(id) => id.to_string(),
            Identifier::Url(url) => url.to_string(),
        }
    }
}

pub type ContentInfos = Vec<ContentInfo>;

#[must_use]
#[derive(Debug)]
pub enum ContentInfo {
    Text(String),
    Image(Url),
}

#[async_trait]
pub trait Client {
    fn proxy(&mut self, proxy: Url);

    fn no_proxy(&mut self);

    fn cert<T>(&mut self, cert_path: T)
    where
        T: AsRef<Path>;

    async fn add_cookie(&self, cookie_str: &str, url: &Url) -> Result<(), Error>;

    async fn login<T, E>(&self, username: T, password: E) -> Result<(), Error>
    where
        T: AsRef<str> + Send + Sync,
        E: AsRef<str> + Send + Sync;

    async fn user_info(&self) -> Result<Option<UserInfo>, Error>;

    async fn novel_info(&self, id: u32) -> Result<NovelInfo, Error>;

    async fn volume_infos(&self, id: u32) -> Result<VolumeInfos, Error>;

    async fn content_infos(&self, info: &ChapterInfo) -> Result<ContentInfos, Error>;

    async fn image_info(&self, url: &Url) -> Result<DynamicImage, Error>;

    async fn search_infos<T>(&self, text: T, page: u16, size: u16) -> Result<Vec<u32>, Error>
    where
        T: AsRef<str> + Send + Sync;

    async fn favorite_infos(&self) -> Result<Vec<u32>, Error>;
}
