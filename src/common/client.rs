use std::{
    ops::{Range, RangeFrom, RangeTo},
    path::Path,
};

use async_trait::async_trait;
use chrono::NaiveDateTime;
use image::DynamicImage;
use url::Url;

use crate::Error;

/// Logged-in user information
#[must_use]
#[derive(Debug)]
pub struct UserInfo {
    /// User's nickname
    pub nickname: String,
}

/// Novel information
#[must_use]
#[derive(Debug, Default)]
pub struct NovelInfo {
    /// Novel id
    pub id: u32,
    /// Novel name
    pub name: String,
    /// Author name
    pub author_name: String,
    /// Url of the novel cover
    pub cover_url: Option<Url>,
    /// Novel introduction
    pub introduction: Option<Vec<String>>,
    /// Novel word count
    pub word_count: Option<u32>,
    /// Is the novel finished
    pub finished: Option<bool>,
    /// Novel creation time
    pub create_time: Option<NaiveDateTime>,
    /// Novel last update time
    pub update_time: Option<NaiveDateTime>,
    /// Novel category
    pub category: Option<Category>,
    /// Novel tags
    pub tags: Option<Vec<Tag>>,
}

impl PartialEq for NovelInfo {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

/// Novel category
#[must_use]
#[derive(Debug, Clone)]
pub struct Category {
    /// Category id
    pub id: Option<u16>,
    /// Category name
    pub name: String,
}

impl ToString for Category {
    fn to_string(&self) -> String {
        self.name.to_string()
    }
}

/// Novel tag
#[must_use]
#[derive(Debug, Clone)]
pub struct Tag {
    /// Tag id
    pub id: Option<u16>,
    /// Tag name
    pub name: String,
}

impl ToString for Tag {
    fn to_string(&self) -> String {
        self.name.to_string()
    }
}

/// Volume information
pub type VolumeInfos = Vec<VolumeInfo>;

/// Volume information
#[must_use]
#[derive(Debug)]
pub struct VolumeInfo {
    /// Volume title
    pub title: String,
    /// Chapter information
    pub chapter_infos: Vec<ChapterInfo>,
}

/// Chapter information
#[must_use]
#[derive(Debug)]
pub struct ChapterInfo {
    /// Chapter identifier
    pub identifier: Identifier,
    /// Chapter title
    pub title: String,
    /// Whether this chapter can only be read by VIP users
    pub is_vip: Option<bool>,
    /// Is the chapter accessible
    pub accessible: Option<bool>,
    /// Is the chapter valid
    pub is_valid: Option<bool>,
    /// Word count
    pub word_count: Option<u16>,
    /// last update time
    pub update_time: Option<NaiveDateTime>,
}

impl ChapterInfo {
    /// Is this chapter available for download
    pub fn can_download(&self) -> bool {
        !crate::is_some_and(self.accessible.as_ref(), |x| !x)
            && !crate::is_some_and(self.is_valid.as_ref(), |x| !x)
    }
}

/// Chapter identifier
#[must_use]
#[derive(Debug)]
pub enum Identifier {
    /// Chapter id
    Id(u32),
    /// Chapter Url
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

/// Content information
pub type ContentInfos = Vec<ContentInfo>;

/// Content information
#[must_use]
#[derive(Debug)]
pub enum ContentInfo {
    /// Text content
    Text(String),
    /// Image content
    Image(Url),
}

#[derive(Default)]
pub struct Options {
    pub is_finished: Option<bool>,
    pub is_vip: Option<bool>,
    pub category: Option<Category>,
    pub tags: Option<Vec<Tag>>,
    pub exclude_tags: Option<Vec<Tag>>,
    pub update_days: Option<u8>,
    pub word_count: Option<WordCountRange>,
}

///
pub enum WordCountRange {
    Range(Range<u32>),
    RangeFrom(RangeFrom<u32>),
    RangeTo(RangeTo<u32>),
}

/// Traits that abstract client behavior
#[async_trait]
pub trait Client {
    /// set proxy
    fn proxy(&mut self, proxy: Url);

    /// Do not use proxy (environment variables used to set proxy are ignored)
    fn no_proxy(&mut self);

    /// Set the certificate path for use with packet capture tools
    fn cert<T>(&mut self, cert_path: T)
    where
        T: AsRef<Path>;

    /// Stop the client, save the data
    fn shutdown(&self) -> Result<(), Error>;

    /// Add cookie
    async fn add_cookie(&self, cookie_str: &str, url: &Url) -> Result<(), Error>;

    /// Login
    async fn login<T, E>(&self, username: T, password: E) -> Result<(), Error>
    where
        T: AsRef<str> + Send + Sync,
        E: AsRef<str> + Send + Sync;

    /// Get the information of the logged-in user, if the information fails to get, it will return None
    async fn user_info(&self) -> Result<Option<UserInfo>, Error>;

    /// Get Novel Information
    async fn novel_info(&self, id: u32) -> Result<Option<NovelInfo>, Error>;

    /// Get volume Information
    async fn volume_infos(&self, id: u32) -> Result<VolumeInfos, Error>;

    /// Get content Information
    async fn content_infos(&self, info: &ChapterInfo) -> Result<ContentInfos, Error>;

    /// Download image
    async fn image(&self, url: &Url) -> Result<DynamicImage, Error>;

    /// Search, return novel id
    async fn search_infos<T>(&self, text: T, page: u16, size: u16) -> Result<Vec<u32>, Error>
    where
        T: AsRef<str> + Send + Sync;

    /// Get the favorite novel of the logged-in user and return the novel id
    async fn favorite_infos(&self) -> Result<Vec<u32>, Error>;

    /// Get category
    async fn categories(&self) -> Result<&Vec<Category>, Error>;

    /// Get all tags
    async fn tags(&self) -> Result<&Vec<Tag>, Error>;

    async fn novels(&self, option: &Options, page: u16, size: u16) -> Result<Vec<u32>, Error>;
}
