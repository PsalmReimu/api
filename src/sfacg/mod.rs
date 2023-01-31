mod structure;
mod utils;

use std::{
    io::Cursor,
    path::{Path, PathBuf},
};

use async_trait::async_trait;
use image::{io::Reader, DynamicImage};
use tokio::sync::OnceCell;
use tracing::warn;
use url::Url;

use crate::{
    ChapterInfo, Client, ContentInfo, ContentInfos, Error, FindImageResult, FindTextResult,
    HTTPClient, Identifier, NovelDB, NovelInfo, Tag, UserInfo, VolumeInfo, VolumeInfos,
};
use structure::*;

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

    fn shutdown(&self) -> Result<(), Error> {
        self.client.get().unwrap().shutdown()
    }

    async fn add_cookie(&self, cookie_str: &str, url: &Url) -> Result<(), Error> {
        Ok(self.client().await?.add_cookie(cookie_str, url)?)
    }

    async fn login<T, E>(&self, username: T, password: E) -> Result<(), Error>
    where
        T: AsRef<str> + Send + Sync,
        E: AsRef<str> + Send + Sync,
    {
        let response = self
            .post(
                "/sessions",
                &LoginRequest {
                    user_name: username.as_ref().to_string(),
                    pass_word: password.as_ref().to_string(),
                },
            )
            .await?
            .json::<LoginResponse>()
            .await?;
        response.status.check()?;

        let response = self
            .get("/position")
            .await?
            .json::<PositionResponse>()
            .await?;
        response.status.check()?;

        Ok(())
    }

    async fn user_info(&self) -> Result<Option<UserInfo>, Error> {
        let response = self.get("/user").await?.json::<UserResponse>().await?;
        if response.status.unauthorized() {
            return Ok(None);
        }
        response.status.check()?;

        let user_info = UserInfo {
            nickname: response.data.unwrap().nick_name.trim().to_string(),
        };

        Ok(Some(user_info))
    }

    async fn novel_info(&self, id: u32) -> Result<Option<NovelInfo>, Error> {
        assert!(id <= i32::MAX as u32);

        let response = self
            .get_query(
                format!("/novels/{id}"),
                &NovelsRequest {
                    expand: Some("intro,typeName,sysTags"),
                },
            )
            .await?
            .json::<NovelsResponse>()
            .await?;
        if response.status.not_found() {
            return Ok(None);
        }
        response.status.check()?;

        let novel_data = response.data.unwrap();

        let word_count = if novel_data.char_count <= 0 {
            None
        } else {
            Some(novel_data.char_count as u32)
        };

        let novel_info = NovelInfo {
            id,
            name: novel_data.novel_name.trim().to_string(),
            author_name: novel_data.author_name.trim().to_string(),
            cover_url: Some(novel_data.novel_cover),
            introduction: SfacgClient::parse_intro(novel_data.expand.intro),
            word_count,
            finished: Some(novel_data.is_finish),
            create_time: Some(novel_data.add_time),
            update_time: Some(novel_data.last_update_time),
            genre: Some(novel_data.expand.type_name.trim().to_string()),
            tags: SfacgClient::parse_tags(novel_data.expand.sys_tags),
        };

        Ok(Some(novel_info))
    }

    async fn volume_infos(&self, id: u32) -> Result<VolumeInfos, Error> {
        assert!(id <= i32::MAX as u32);

        let response = self
            .get(format!("/novels/{id}/dirs"))
            .await?
            .json::<NovelsDirsResponse>()
            .await?;
        response.status.check()?;

        let mut volumes = VolumeInfos::new();
        for volume in response.data.unwrap().volume_list {
            let mut volume_info = VolumeInfo {
                title: volume.title.trim().to_string(),
                chapter_infos: vec![],
            };

            for chapter in volume.chapter_list {
                let update_time = if chapter.update_time.is_some() {
                    chapter.update_time
                } else {
                    Some(chapter.add_time)
                };

                let word_count = if chapter.char_count <= 0 {
                    None
                } else {
                    Some(chapter.char_count as u16)
                };

                let chapter_info = ChapterInfo {
                    identifier: Identifier::Id(chapter.chap_id),
                    title: chapter.title.trim().to_string(),
                    word_count,
                    update_time,
                    is_vip: Some(chapter.is_vip),
                    accessible: Some(chapter.need_fire_money == 0),
                    is_valid: None,
                };

                volume_info.chapter_infos.push(chapter_info);
            }

            volumes.push(volume_info);
        }

        Ok(volumes)
    }

    async fn content_infos(&self, info: &ChapterInfo) -> Result<ContentInfos, Error> {
        let content;

        match self.db().await?.find_text(info).await? {
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
                    .await?
                    .json::<ChapsResponse>()
                    .await?;
                response.status.check()?;

                content = response.data.unwrap().expand.content;

                match other {
                    FindTextResult::None => self.db().await?.insert_text(info, &content).await?,
                    FindTextResult::Outdate => self.db().await?.update_text(info, &content).await?,
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
                if let Some(url) = SfacgClient::parse_image_url(line) {
                    content_infos.push(ContentInfo::Image(url));
                }
            } else {
                content_infos.push(ContentInfo::Text(line.to_string()));
            }
        }

        Ok(content_infos)
    }

    async fn image_info(&self, url: &Url) -> Result<DynamicImage, Error> {
        match self.db().await?.find_image(url).await? {
            FindImageResult::Ok(image) => Ok(image),
            FindImageResult::None => {
                let response = self.get_rss(url).await?;
                let bytes = response.bytes().await?;

                self.db().await?.insert_image(url, &bytes).await?;

                Ok(Reader::new(Cursor::new(bytes))
                    .with_guessed_format()?
                    .decode()?)
            }
        }
    }

    async fn search_infos<T>(&self, text: T, page: u16, size: u16) -> Result<Vec<u32>, Error>
    where
        T: AsRef<str> + Send + Sync,
    {
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
            .await?
            .json::<SearchResponse>()
            .await?;
        response.status.check()?;

        let mut result = Vec::new();
        if response.data.is_some() {
            for novel_info in response.data.unwrap().novels {
                result.push(novel_info.novel_id);
            }
        }

        Ok(result)
    }

    async fn favorite_infos(&self) -> Result<Vec<u32>, Error> {
        let response = self
            .get_query(
                "/user/Pockets",
                &FavoritesRequest {
                    expand: Some("novels,albums,comics"),
                },
            )
            .await?
            .json::<FavoritesResponse>()
            .await?;
        response.status.check()?;

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

        Ok(result)
    }
}

impl SfacgClient {
    fn parse_tags(sys_tags: Vec<NovelsSysTag>) -> Option<Vec<Tag>> {
        let mut result = vec![];
        for tag in sys_tags {
            result.push(Tag {
                id: Some(tag.sys_tag_id),
                name: tag.tag_name.trim().to_string(),
            });
        }

        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }

    fn parse_intro(intro: String) -> Option<Vec<String>> {
        let introduction = intro
            .lines()
            .map(|line| line.trim().to_string())
            .filter(|line| !line.is_empty())
            .collect::<Vec<String>>();

        if introduction.is_empty() {
            None
        } else {
            Some(introduction)
        }
    }

    fn parse_image_url(line: &str) -> Option<Url> {
        let begin = line.find("https");
        let end = line.find("[/img]");

        if begin.is_none() || end.is_none() {
            warn!("Image URL format is incorrect: {line}");
        }

        let begin = begin.unwrap();
        let end = end.unwrap();

        let url = line
            .chars()
            .skip(begin)
            .take(end - begin)
            .collect::<String>()
            .trim()
            .to_string();

        match Url::parse(&url) {
            Ok(url) => Some(url),
            Err(error) => {
                warn!("Image URL parse failed: {error}, content: {line}");
                None
            }
        }
    }
}
