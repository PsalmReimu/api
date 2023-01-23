mod utils;

use std::{
    io::Cursor,
    path::{Path, PathBuf},
};

use async_trait::async_trait;
use chrono::NaiveDateTime;
use http::StatusCode;
use image::{io::Reader, DynamicImage};
use serde::{Deserialize, Serialize};
use tokio::sync::OnceCell;
use tracing::{info, warn};
use url::Url;

use crate::{
    here, ChapterInfo, Client, ContentInfo, ContentInfos, Error, ErrorLocation, FindImageResult,
    FindTextResult, HTTPClient, Identifier, Location, NovelDB, NovelInfo, Tag, Timing, UserInfo,
    VolumeInfo, VolumeInfos,
};

/// Sfacg client, use it to access Apis
#[must_use]
pub struct SfacgClient {
    proxy: Option<Url>,
    no_proxy: bool,
    cert_path: Option<PathBuf>,

    client: OnceCell<HTTPClient>,
    client_rss: OnceCell<HTTPClient>,

    db: OnceCell<NovelDB>,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    http_code: u16,
    error_code: u16,
    msg_type: u16,
    msg: Option<String>,
}

impl Status {
    fn ok(&self) -> bool {
        self.http_code == StatusCode::OK && self.error_code == 200
    }

    fn unauthorized(&self) -> bool {
        self.http_code == StatusCode::UNAUTHORIZED && self.error_code == 502
    }

    fn check(&self) -> Result<(), Error> {
        if !self.ok() {
            return Err(Error::Http {
                code: StatusCode::from_u16(self.http_code).location(here!())?,
                msg: self.msg.clone().expect("The error message does not exist"),
            })
            .location(here!())?;
        }

        Ok(())
    }
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoginRequest {
    user_name: String,
    pass_word: String,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct LoginResponse {
    status: Status,
    data: Option<String>,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct PositionResponse {
    status: Status,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct UserResponse {
    status: Status,
    data: Option<UserData>,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserData {
    nick_name: String,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct NovelsRequest {
    expand: Option<&'static str>,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct NovelsResponse {
    status: Status,
    data: Option<NovelsData>,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NovelsData {
    novel_id: u32,
    novel_name: String,
    novel_cover: Url,
    author_name: String,
    char_count: i32,
    is_finish: bool,
    add_time: NaiveDateTime,
    last_update_time: NaiveDateTime,
    expand: NovelsExpand,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NovelsExpand {
    type_name: String,
    intro: String,
    sys_tags: Vec<NovelsSysTag>,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NovelsSysTag {
    tag_name: String,
    sys_tag_id: u16,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct NovelsDirsResponse {
    status: Status,
    data: Option<NovelsDirsData>,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NovelsDirsData {
    volume_list: Vec<NovelsDirsVolumeInfo>,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NovelsDirsVolumeInfo {
    volume_id: u32,
    title: String,
    chapter_list: Vec<NovelsDirsChapterInfo>,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NovelsDirsChapterInfo {
    chap_id: u32,
    title: String,
    char_count: u16,
    is_vip: bool,
    need_fire_money: u16,
    #[serde(rename = "AddTime")]
    add_time: NaiveDateTime,
    update_time: Option<NaiveDateTime>,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct ChapsRequest {
    expand: Option<&'static str>,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct ChapsResponse {
    status: Status,
    data: Option<ChapsData>,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct ChapsData {
    expand: ChapsExpand,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct ChapsExpand {
    content: String,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct SearchRequest {
    expand: Option<&'static str>,
    page: u16,
    q: String,
    size: u16,
    sort: &'static str,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct SearchResponse {
    status: Status,
    data: Option<SearchData>,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct SearchData {
    novels: Vec<SearchNovelInfo>,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchNovelInfo {
    novel_id: u32,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct FavoritesRequest {
    expand: Option<&'static str>,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct FavoritesResponse {
    status: Status,
    data: Option<Vec<FavoritesData>>,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct FavoritesData {
    name: String,
    expand: FavoritesExpand,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum FavoritesExpand {
    Novels(Vec<FavoritesNovelsData>),
    Albums(Vec<FavoritesNovelsData>),
    Comics(Vec<FavoritesNovelsData>),
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FavoritesNovelsData {
    novel_id: u32,
}

#[async_trait]
impl Client for SfacgClient {
    fn proxy(&mut self, proxy: Url) {
        self.proxy = Some(proxy);
    }

    fn no_proxy(&mut self) {
        self.no_proxy = true;
    }

    fn cert<T>(&mut self, cert_path: T)
    where
        T: AsRef<Path>,
    {
        self.cert_path = Some(cert_path.as_ref().to_path_buf());
    }

    async fn add_cookie(&self, cookie_str: &str, url: &Url) -> Result<(), Error> {
        self.client()
            .await
            .location(here!())?
            .add_cookie(cookie_str, url)
            .location(here!())?;

        Ok(())
    }

    async fn login<T, E>(&self, username: T, password: E) -> Result<(), Error>
    where
        T: AsRef<str> + Send + Sync,
        E: AsRef<str> + Send + Sync,
    {
        let mut timing = Timing::new();

        let response = self
            .post(
                "/sessions",
                &LoginRequest {
                    user_name: username.as_ref().to_string(),
                    pass_word: password.as_ref().to_string(),
                },
            )
            .await
            .location(here!())?
            .json::<LoginResponse>()
            .await
            .location(here!())?;
        response.status.check().location(here!())?;

        info!("Time spent on `/sessions`: {}", timing.elapsed()?);

        let response = self
            .get("/position")
            .await
            .location(here!())?
            .json::<PositionResponse>()
            .await
            .location(here!())?;
        response.status.check().location(here!())?;

        info!("Time spent on `/position`: {}", timing.elapsed()?);

        Ok(())
    }

    async fn user_info(&self) -> Result<Option<UserInfo>, Error> {
        let mut timing = Timing::new();

        let response = self
            .get("/user")
            .await
            .location(here!())?
            .json::<UserResponse>()
            .await
            .location(here!())?;
        if response.status.unauthorized() {
            return Ok(None);
        }
        response.status.check().location(here!())?;

        let info = UserInfo {
            nickname: response
                .data
                .expect("Api error, no `data` field")
                .nick_name
                .trim()
                .to_string(),
        };

        info!("Time spent on `/user`: {}", timing.elapsed()?);

        Ok(Some(info))
    }

    async fn novel_info(&self, id: u32) -> Result<NovelInfo, Error> {
        let mut timing = Timing::new();

        let response = self
            .get_query(
                format!("/novels/{}", id),
                &NovelsRequest {
                    expand: Some("intro,typeName,sysTags"),
                },
            )
            .await
            .location(here!())?
            .json::<NovelsResponse>()
            .await
            .location(here!())?;
        response.status.check().location(here!())?;

        let novel_data = response.data.expect("Api error, no `data` field");

        let mut tags = vec![];
        for tag in novel_data.expand.sys_tags {
            tags.push(Tag {
                id: Some(tag.sys_tag_id),
                name: tag.tag_name.trim().to_string(),
            });
        }
        let tags = if tags.is_empty() { None } else { Some(tags) };

        let introduction = novel_data
            .expand
            .intro
            .lines()
            .map(|line| line.trim().to_string())
            .filter(|line| !line.is_empty())
            .collect::<Vec<String>>();
        let introduction = if introduction.is_empty() {
            None
        } else {
            Some(introduction)
        };

        // FIXME
        // e.g. sfacg novel-id 539889, char count is negative number
        let word_count = if novel_data.char_count < 0 {
            0
        } else {
            novel_data.char_count as u32
        };

        let novel_info = NovelInfo {
            id: novel_data.novel_id,
            name: novel_data.novel_name.trim().to_string(),
            author_name: novel_data.author_name.trim().to_string(),
            cover_url: Some(novel_data.novel_cover),
            introduction,
            word_count: Some(word_count),
            finished: Some(novel_data.is_finish),
            create_time: Some(novel_data.add_time),
            update_time: Some(novel_data.last_update_time),
            genre: Some(novel_data.expand.type_name.trim().to_string()),
            tags,
        };

        info!(
            "Time spent on `{}`: {}",
            format!("/novels/{}", id),
            timing.elapsed()?
        );

        Ok(novel_info)
    }

    async fn volume_infos(&self, id: u32) -> Result<VolumeInfos, Error> {
        let mut timing = Timing::new();

        let response = self
            .get(format!("/novels/{}/dirs", id))
            .await
            .location(here!())?
            .json::<NovelsDirsResponse>()
            .await
            .location(here!())?;
        response.status.check().location(here!())?;

        let mut volumes = VolumeInfos::new();

        for volume in response
            .data
            .expect("Api error, no `data` field")
            .volume_list
        {
            let mut volume_info = VolumeInfo {
                id: Some(volume.volume_id),
                title: volume.title.trim().to_string(),
                chapter_infos: vec![],
            };

            for chapter in volume.chapter_list {
                let update_time = if chapter.update_time.is_some() {
                    chapter.update_time
                } else {
                    Some(chapter.add_time)
                };

                let chapter_info = ChapterInfo {
                    identifier: Identifier::Id(chapter.chap_id),
                    title: chapter.title.trim().to_string(),
                    word_count: Some(chapter.char_count),
                    update_time,
                    is_vip: Some(chapter.is_vip),
                    accessible: Some(chapter.need_fire_money == 0),
                    is_valid: Some(true),
                };

                volume_info.chapter_infos.push(chapter_info);
            }

            volumes.push(volume_info);
        }

        info!(
            "Time spent on `{}`: {}",
            format!("/novels/{}/dirs", id),
            timing.elapsed()?
        );

        Ok(volumes)
    }

    async fn content_infos(&self, info: &ChapterInfo) -> Result<ContentInfos, Error> {
        let mut timing = Timing::new();

        let content;

        match self
            .db()
            .await
            .location(here!())?
            .find_text(info)
            .await
            .location(here!())?
        {
            FindTextResult::Ok(str) => {
                content = str;
            }
            other => {
                let response = self
                    .get_query(
                        format!("/Chaps/{}", info.identifier.to_string()),
                        &ChapsRequest {
                            expand: Some("content"),
                        },
                    )
                    .await
                    .location(here!())?
                    .json::<ChapsResponse>()
                    .await
                    .location(here!())?;
                response.status.check().location(here!())?;

                content = response
                    .data
                    .expect("Api error, no `data` field")
                    .expand
                    .content;

                match other {
                    FindTextResult::None => self
                        .db()
                        .await
                        .location(here!())?
                        .insert_text(info, &content)
                        .await
                        .location(here!())?,
                    FindTextResult::Outdate => self
                        .db()
                        .await
                        .location(here!())?
                        .update_text(info, &content)
                        .await
                        .location(here!())?,
                    FindTextResult::Ok(_) => (),
                }
            }
        }

        let mut content_infos = ContentInfos::new();

        for line in content
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
        {
            if line.starts_with("[img") {
                match SfacgClient::parser_image_url(line) {
                    Ok(url) => content_infos.push(ContentInfo::Image(url)),
                    Err(error) => warn!("{}", error),
                }
            } else {
                content_infos.push(ContentInfo::Text(line.to_string()));
            }
        }

        info!(
            "Time spent on `{}`: {}",
            format!("/Chaps/{}", info.identifier.to_string()),
            timing.elapsed()?
        );

        Ok(content_infos)
    }

    async fn image_info(&self, url: &Url) -> Result<DynamicImage, Error> {
        let mut timing = Timing::new();

        let image = match self
            .db()
            .await
            .location(here!())?
            .find_image(url)
            .await
            .location(here!())?
        {
            FindImageResult::Ok(image) => Ok(image),
            FindImageResult::None => {
                let response = self.get_rss(url).await.location(here!())?;

                if response.status() != StatusCode::OK {
                    return Err(Error::Http {
                        code: response.status(),
                        msg: "Image download failed".to_string(),
                    });
                }

                let bytes = response.bytes().await.location(here!())?;

                self.db()
                    .await
                    .location(here!())?
                    .insert_image(url, &bytes)
                    .await
                    .location(here!())?;

                let image = Reader::new(Cursor::new(bytes))
                    .with_guessed_format()?
                    .decode()
                    .location(here!())?;

                Ok(image)
            }
        };

        info!(
            "Time spent on download image: `{}`: {}",
            url,
            timing.elapsed()?
        );

        image
    }

    async fn search_infos<T>(&self, text: T, page: u16, size: u16) -> Result<Vec<u32>, Error>
    where
        T: AsRef<str> + Send + Sync,
    {
        let mut timing = Timing::new();

        let response = self
            .get_query(
                "/search/novels/result/new",
                &SearchRequest {
                    expand: None,
                    page,
                    q: text.as_ref().to_string(),
                    size,
                    sort: "hot",
                },
            )
            .await
            .location(here!())?
            .json::<SearchResponse>()
            .await
            .location(here!())?;
        response.status.check().location(here!())?;

        let mut result = Vec::new();

        if response.data.is_some() {
            for novel_info in response.data.unwrap().novels {
                result.push(novel_info.novel_id);
            }
        }

        info!(
            "Time spent on `/search/novels/result/new`: {}",
            timing.elapsed()?
        );

        Ok(result)
    }

    async fn favorite_infos(&self) -> Result<Vec<u32>, Error> {
        let mut timing = Timing::new();

        let response = self
            .get_query(
                "/user/Pockets",
                &FavoritesRequest {
                    expand: Some("novels,albums,comics"),
                },
            )
            .await
            .location(here!())?
            .json::<FavoritesResponse>()
            .await
            .location(here!())?;
        response.status.check().location(here!())?;

        let mut result = Vec::new();

        if response.data.is_some() {
            for data in response.data.unwrap() {
                if let FavoritesExpand::Novels(novels) = data.expand {
                    for novel_info in novels {
                        result.push(novel_info.novel_id);
                    }
                }
            }
        }

        info!("Time spent on `/user/Pockets`: {}", timing.elapsed()?);

        Ok(result)
    }
}

impl SfacgClient {
    fn parser_image_url(line: &str) -> Result<Url, Error> {
        let begin = line.find("https");
        let end = line.find("[/img]");

        let begin = begin
            .ok_or(Error::NovelApi(format!(
                "Image insertion format is incorrect: {}",
                line
            )))
            .location(here!())?;
        let end = end
            .ok_or(Error::NovelApi(format!(
                "Image insertion format is incorrect: {}",
                line
            )))
            .location(here!())?;

        let url = line
            .chars()
            .skip(begin)
            .take(end - begin)
            .collect::<String>()
            .trim()
            .to_string();

        Ok(Url::parse(&url).location(here!())?)
    }
}
