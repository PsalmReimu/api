mod utils;

use std::{
    io::{self, Cursor, Write},
    path::{Path, PathBuf},
    str::FromStr,
    time::{SystemTime, UNIX_EPOCH},
};

use ahash::AHashMap;
use async_trait::async_trait;
use boring::{
    hash::{self, MessageDigest},
    sha,
};
use chrono::NaiveDateTime;
use hex_simd::AsciiCase;
use http::StatusCode;
use image::{io::Reader, DynamicImage};
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot, OnceCell};
use tracing::{info, warn};
use url::Url;
use warp::{http::Response, Filter};

use crate::{
    here, ChapterInfo, Client, ContentInfo, ContentInfos, Error, ErrorLocation, FindImageResult,
    FindTextResult, HTTPClient, Identifier, Location, NovelDB, NovelInfo, Tag, Timing, UserInfo,
    VolumeInfo, VolumeInfos,
};

/// Ciweimao client, use it to access Apis
#[must_use]
pub struct CiweimaoClient {
    proxy: Option<Url>,
    no_proxy: bool,
    cert_path: Option<PathBuf>,

    client: OnceCell<HTTPClient>,
    client_rss: OnceCell<HTTPClient>,

    db: OnceCell<NovelDB>,

    account: RwLock<Option<String>>,
    login_token: RwLock<Option<String>>,
}

fn check_response(code: &str, tip: &Option<String>) -> Result<(), Error> {
    if code != "100000" {
        Err(Error::NovelApi(
            tip.as_ref()
                .expect("The error message does not exist")
                .to_string(),
        ))
    } else {
        Ok(())
    }
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct UserInfoRequest {
    app_version: String,
    device_token: String,
    account: String,
    login_token: String,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct UserInfoResponse {
    code: String,
    tip: Option<String>,
    data: Option<UserInfoData>,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct UserInfoData {
    reader_info: UserInfoReaderInfo,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct UserInfoReaderInfo {
    reader_name: String,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct NovelInfoRequest {
    app_version: String,
    device_token: String,
    account: String,
    login_token: String,
    book_id: u32,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct NovelInfoResponse {
    code: String,
    tip: Option<String>,
    data: Option<NovelInfoData>,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct NovelInfoData {
    book_info: NovelInfoBookInfo,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct NovelInfoBookInfo {
    book_name: String,
    author_name: String,
    cover: String,
    description: String,
    total_word_count: String,
    up_status: String,
    newtime: String,
    uptime: String,
    category_index: String,
    tag: String,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct VolumesRequest {
    app_version: String,
    device_token: String,
    account: String,
    login_token: String,
    book_id: u32,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct VolumesResponse {
    code: String,
    tip: Option<String>,
    data: Option<VolumesData>,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct VolumesData {
    chapter_list: Vec<VolumesVolumeInfo>,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct VolumesVolumeInfo {
    division_name: String,
    chapter_list: Vec<VolumesChapterInfo>,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct VolumesChapterInfo {
    chapter_id: String,
    chapter_title: String,
    word_count: String,
    mtime: String,
    is_valid: String,
    auth_access: String,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct ChapsRequest {
    app_version: String,
    device_token: String,
    account: String,
    login_token: String,
    chapter_id: String,
    chapter_command: String,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct ChapsResponse {
    code: String,
    tip: Option<String>,
    data: Option<ChapsData>,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct ChapsData {
    chapter_info: ChapsInfo,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct ChapsInfo {
    txt_content: String,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct SearchRequest {
    app_version: String,
    device_token: String,
    account: String,
    login_token: String,
    key: String,
    count: u16,
    page: u16,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct SearchResponse {
    code: String,
    tip: Option<String>,
    data: Option<SearchData>,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct SearchData {
    book_list: Vec<SearchNovelInfo>,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct SearchNovelInfo {
    book_id: String,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct FavoritesRequest {
    app_version: String,
    device_token: String,
    account: String,
    login_token: String,
    shelf_id: u32,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct FavoritesResponse {
    code: String,
    tip: Option<String>,
    data: Option<FavoritesData>,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct FavoritesData {
    book_list: Vec<FavoritesInfo>,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct FavoritesInfo {
    book_info: FavoritesNovelInfo,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct FavoritesNovelInfo {
    book_id: String,
}

static CATEGORIES: Lazy<AHashMap<u8, &str>> = Lazy::new(|| {
    AHashMap::from([
        (0, "全部分类"),
        (1, "灵异未知"),
        (3, "游戏竞技"),
        (5, "仙侠武侠"),
        (6, "科幻无限"),
        (8, "玄幻奇幻"),
        (11, "女频"),
        (24, "免费同人"),
        (27, "都市青春"),
        (30, "历史军事"),
    ])
});

#[async_trait]
impl Client for CiweimaoClient {
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

        let verify_type = self.verify_type(&username).await.location(here!())?;
        let (account, login_token);

        match verify_type {
            VerifyType::None => {
                info!("No verification required");
                (account, login_token) = self.no_verification_login(username, password).await?;
            }
            VerifyType::Geetest => {
                info!("Verify with Geetest");
                (account, login_token) = self.geetest_login(username, password).await?;
            }
            VerifyType::VerifyCode => {
                info!("Verify with SMS verification code");
                (account, login_token) = self.sms_login(username, password).await?;
            }
        };

        *self.account.write() = Some(account);
        *self.login_token.write() = Some(login_token);

        info!("Time spent on login: {}", timing.elapsed()?);

        Ok(())
    }

    async fn user_info(&self) -> Result<Option<UserInfo>, Error> {
        let mut timing = Timing::new();

        if self.account().is_empty() || self.login_token().is_empty() {
            return Ok(None);
        }

        let response: UserInfoResponse = self
            .post(
                "/reader/get_my_info",
                &UserInfoRequest {
                    app_version: CiweimaoClient::APP_VERSION.to_string(),
                    device_token: CiweimaoClient::DEVICE_TOKEN.to_string(),
                    account: self.account(),
                    login_token: self.login_token(),
                },
            )
            .await
            .location(here!())?;
        if response.code == CiweimaoClient::LOGIN_EXPIRED {
            return Ok(None);
        }
        check_response(&response.code, &response.tip).location(here!())?;

        let data = response.data.unwrap().reader_info;
        let user_info = UserInfo {
            nickname: data.reader_name,
        };

        info!("Time spent on `/reader/get_my_info`: {}", timing.elapsed()?);

        Ok(Some(user_info))
    }

    async fn novel_info(&self, id: u32) -> Result<Option<NovelInfo>, Error> {
        let mut timing = Timing::new();

        let response: NovelInfoResponse = self
            .post(
                "/book/get_info_by_id",
                &NovelInfoRequest {
                    app_version: CiweimaoClient::APP_VERSION.to_string(),
                    device_token: CiweimaoClient::DEVICE_TOKEN.to_string(),
                    account: self.account(),
                    login_token: self.login_token(),
                    book_id: id,
                },
            )
            .await
            .location(here!())?;
        if response.code == CiweimaoClient::NOT_FOUND {
            return Ok(None);
        }
        check_response(&response.code, &response.tip).location(here!())?;

        let data = response.data.unwrap().book_info;
        let novel_info = NovelInfo {
            id,
            name: data.book_name,
            author_name: data.author_name,
            cover_url: CiweimaoClient::parse_url(&data.cover),
            introduction: CiweimaoClient::parse_introduction(&data.description),
            word_count: CiweimaoClient::parse_number(&data.total_word_count),
            finished: CiweimaoClient::parse_bool(&data.up_status),
            create_time: CiweimaoClient::parse_data_time(&data.newtime),
            update_time: CiweimaoClient::parse_data_time(&data.uptime),
            genre: CiweimaoClient::parse_genre(&data.category_index),
            tags: CiweimaoClient::parse_tags(&data.tag),
        };

        info!(
            "Time spent on `/book/get_info_by_id`: {}",
            timing.elapsed()?
        );

        Ok(Some(novel_info))
    }

    async fn volume_infos(&self, id: u32) -> Result<VolumeInfos, Error> {
        let mut timing = Timing::new();

        let response: VolumesResponse = self
            .post(
                "/chapter/get_updated_chapter_by_division_new",
                &VolumesRequest {
                    app_version: CiweimaoClient::APP_VERSION.to_string(),
                    device_token: CiweimaoClient::DEVICE_TOKEN.to_string(),
                    account: self.account(),
                    login_token: self.login_token(),
                    book_id: id,
                },
            )
            .await
            .location(here!())?;
        check_response(&response.code, &response.tip).location(here!())?;

        let mut volume_infos = VolumeInfos::new();
        for item in response.data.unwrap().chapter_list {
            let mut volume_info = VolumeInfo {
                title: item.division_name,
                chapter_infos: Vec::new(),
            };

            for chapter in item.chapter_list {
                let chapter_info = ChapterInfo {
                    identifier: Identifier::Id(
                        chapter.chapter_id.parse::<u32>().location(here!())?,
                    ),
                    title: chapter.chapter_title,
                    word_count: CiweimaoClient::parse_number(&chapter.word_count),
                    update_time: CiweimaoClient::parse_data_time(&chapter.mtime),
                    is_vip: None,
                    accessible: CiweimaoClient::parse_bool(&chapter.auth_access),
                    is_valid: CiweimaoClient::parse_bool(&chapter.is_valid),
                };

                volume_info.chapter_infos.push(chapter_info);
            }

            volume_infos.push(volume_info);
        }

        info!(
            "Time spent on `/chapter/get_updated_chapter_by_division_new`: {}",
            timing.elapsed()?
        );

        Ok(volume_infos)
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
                let cmd = self
                    .chapter_cmd(info.identifier.to_string())
                    .await
                    .location(here!())?;
                let aes_key = sha::sha256(cmd.as_bytes());

                let response: ChapsResponse = self
                    .post(
                        "/chapter/get_cpt_ifm",
                        &ChapsRequest {
                            app_version: CiweimaoClient::APP_VERSION.to_string(),
                            device_token: CiweimaoClient::DEVICE_TOKEN.to_string(),
                            account: self.account(),
                            login_token: self.login_token(),
                            chapter_id: info.identifier.to_string(),
                            chapter_command: cmd,
                        },
                    )
                    .await
                    .location(here!())?;
                check_response(&response.code, &response.tip).location(here!())?;

                let conetent = CiweimaoClient::aes_256_cbc_base64_decrypt(
                    aes_key,
                    response.data.unwrap().chapter_info.txt_content,
                )
                .location(here!())?;
                content = simdutf8::basic::from_utf8(&conetent)?.to_string();

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
            if line.starts_with("<img") {
                if let Some(url) = CiweimaoClient::parse_image_url(line) {
                    content_infos.push(ContentInfo::Image(url));
                }
            } else {
                content_infos.push(ContentInfo::Text(line.to_string()));
            }
        }

        info!(
            "Time spent on `/chapter/get_cpt_ifm`: {}",
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

        let response: SearchResponse = self
            .post(
                "/bookcity/get_filter_search_book_list",
                &SearchRequest {
                    app_version: CiweimaoClient::APP_VERSION.to_string(),
                    device_token: CiweimaoClient::DEVICE_TOKEN.to_string(),
                    account: self.account(),
                    login_token: self.login_token(),
                    key: text.as_ref().to_string(),
                    count: size,
                    page,
                },
            )
            .await
            .location(here!())?;
        check_response(&response.code, &response.tip).location(here!())?;

        let mut result = Vec::new();
        if response.data.is_some() {
            for novel_info in response.data.unwrap().book_list {
                result.push(novel_info.book_id.parse::<u32>().location(here!())?);
            }
        }

        info!(
            "Time spent on `/bookcity/get_filter_search_book_list`: {}",
            timing.elapsed()?
        );

        Ok(result)
    }

    async fn favorite_infos(&self) -> Result<Vec<u32>, Error> {
        let mut timing = Timing::new();

        let shelf_id = self.shelf_list().await.location(here!())?;

        let response: FavoritesResponse = self
            .post(
                "/bookshelf/get_shelf_book_list_new",
                &FavoritesRequest {
                    app_version: CiweimaoClient::APP_VERSION.to_string(),
                    device_token: CiweimaoClient::DEVICE_TOKEN.to_string(),
                    account: self.account(),
                    login_token: self.login_token(),
                    shelf_id,
                },
            )
            .await
            .location(here!())?;
        check_response(&response.code, &response.tip).location(here!())?;

        let mut result = Vec::new();
        let data = response.data.unwrap().book_list;

        for novel_info in data {
            result.push(
                novel_info
                    .book_info
                    .book_id
                    .parse::<u32>()
                    .location(here!())?,
            );
        }

        info!(
            "Time spent on `/bookshelf/get_shelf_book_list_new`: {}",
            timing.elapsed()?
        );

        Ok(result)
    }
}

#[must_use]
#[derive(Debug)]
enum VerifyType {
    None,
    Geetest,
    VerifyCode,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct UseGeetestRequest {
    app_version: String,
    device_token: String,
    login_name: String,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct UseGeetestResponse {
    code: String,
    tip: Option<String>,
    data: Option<UseGeetestData>,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct UseGeetestData {
    need_use_geetest: String,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct GeetestInfoRequest {
    t: u64,
    user_id: String,
}

#[must_use]
#[derive(Debug, Clone, Serialize, Deserialize)]
struct GeetestInfoResponse {
    success: u8,
    gt: String,
    challenge: String,
    new_captcha: bool,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct SendVerifyCodeRequest {
    account: String,
    app_version: String,
    device_token: String,
    hashvalue: String,
    login_name: String,
    timestamp: String,
    verify_type: String,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct SendVerifyCodeResponse {
    code: String,
    tip: Option<String>,
    data: Option<SendVerifyCodeData>,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct SendVerifyCodeData {
    to_code: String,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct LoginRequest {
    app_version: String,
    device_token: String,
    login_name: String,
    passwd: String,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct LoginCaptchaRequest {
    app_version: String,
    device_token: String,
    login_name: String,
    passwd: String,
    geetest_seccode: String,
    geetest_validate: String,
    geetest_challenge: String,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct LoginSMSRequest {
    app_version: String,
    device_token: String,
    login_name: String,
    passwd: String,
    to_code: String,
    ver_code: String,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct LoginResponse {
    code: String,
    tip: Option<String>,
    data: Option<LoginData>,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct LoginData {
    login_token: String,
    reader_info: LoginReaderInfo,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct LoginReaderInfo {
    account: String,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct ChapterCmdRequest {
    app_version: String,
    device_token: String,
    account: String,
    login_token: String,
    chapter_id: String,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct ChapterCmdResponse {
    code: String,
    tip: Option<String>,
    data: Option<ChapterCmdData>,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct ChapterCmdData {
    command: String,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct ShelfListRequest {
    app_version: String,
    device_token: String,
    account: String,
    login_token: String,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct ShelfListResponse {
    code: String,
    tip: Option<String>,
    data: Option<ShelfListData>,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct ShelfListData {
    shelf_list: Vec<ShelfList>,
}

#[must_use]
#[derive(Debug, Serialize, Deserialize)]
struct ShelfList {
    shelf_id: String,
}

impl CiweimaoClient {
    async fn verify_type<T>(&self, username: T) -> Result<VerifyType, Error>
    where
        T: AsRef<str>,
    {
        let response: UseGeetestResponse = self
            .post(
                "/signup/use_geetest",
                &UseGeetestRequest {
                    app_version: CiweimaoClient::APP_VERSION.to_string(),
                    device_token: CiweimaoClient::DEVICE_TOKEN.to_string(),
                    login_name: username.as_ref().to_string(),
                },
            )
            .await
            .location(here!())?;
        check_response(&response.code, &response.tip).location(here!())?;

        let data = response.data.unwrap();
        if data.need_use_geetest == "0" {
            Ok(VerifyType::None)
        } else if data.need_use_geetest == "1" {
            Ok(VerifyType::Geetest)
        } else if data.need_use_geetest == "2" {
            Ok(VerifyType::VerifyCode)
        } else {
            unreachable!("The value range of need_use_geetest is 1..=2");
        }
    }

    async fn no_verification_login<T, E>(
        &self,
        username: T,
        password: E,
    ) -> Result<(String, String), Error>
    where
        T: AsRef<str> + Send + Sync,
        E: AsRef<str> + Send + Sync,
    {
        let response: LoginResponse = self
            .post(
                "/signup/login",
                &LoginRequest {
                    app_version: CiweimaoClient::APP_VERSION.to_string(),
                    device_token: CiweimaoClient::DEVICE_TOKEN.to_string(),
                    login_name: username.as_ref().to_string(),
                    passwd: password.as_ref().to_string(),
                },
            )
            .await
            .location(here!())?;
        check_response(&response.code, &response.tip).location(here!())?;

        let data = response.data.unwrap();
        Ok((data.reader_info.account, data.login_token))
    }

    async fn geetest_login<T, E>(&self, username: T, password: E) -> Result<(String, String), Error>
    where
        T: AsRef<str> + Send + Sync,
        E: AsRef<str> + Send + Sync,
    {
        let info = self.geetest_info(&username).await.location(here!())?;
        let validate = CiweimaoClient::run_server(&info).await.location(here!())?;

        let response: LoginResponse = self
            .post(
                "/signup/login",
                &LoginCaptchaRequest {
                    app_version: CiweimaoClient::APP_VERSION.to_string(),
                    device_token: CiweimaoClient::DEVICE_TOKEN.to_string(),
                    login_name: username.as_ref().to_string(),
                    passwd: password.as_ref().to_string(),
                    geetest_seccode: validate.to_string() + "|jordan",
                    geetest_validate: validate,
                    geetest_challenge: info.challenge,
                },
            )
            .await
            .location(here!())?;
        check_response(&response.code, &response.tip).location(here!())?;

        let data = response.data.unwrap();
        Ok((data.reader_info.account, data.login_token))
    }

    async fn geetest_info<T>(&self, username: T) -> Result<GeetestInfoResponse, Error>
    where
        T: AsRef<str>,
    {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

        let response = self
            .get_query(
                "/signup/geetest_first_register",
                &GeetestInfoRequest {
                    t: timestamp,
                    user_id: username.as_ref().to_string(),
                },
            )
            .await
            .location(here!())?
            .json::<GeetestInfoResponse>()
            .await
            .location(here!())?;

        if response.success != 1 {
            return Err(Error::NovelApi(
                "`/signup/geetest_first_register` failed".to_string(),
            ));
        }

        Ok(response)
    }

    // TODO 更美观的页面
    async fn run_server(info: &GeetestInfoResponse) -> Result<String, Error> {
        // TODO use std::path::MAIN_SEPARATOR_STR
        // https://doc.rust-lang.org/std/path/constant.MAIN_SEPARATOR_STR.html
        #[cfg(target_os = "windows")]
        macro_rules! PATH_SEPARATOR {
            () => {
                r"\"
            };
        }

        #[cfg(not(target_os = "windows"))]
        macro_rules! PATH_SEPARATOR {
            () => {
                r"/"
            };
        }

        let js = warp::path("geetest.js").map(|| {
            Response::builder()
                .status(200)
                .header("content-type", "text/javascript")
                .body(include_str!(concat!(
                    "assets",
                    PATH_SEPARATOR!(),
                    "geetest.js"
                )))
        });

        let info = info.clone();
        let index = warp::path("captcha").map(move || {
            let html = format!(
                include_str!(concat!("assets", PATH_SEPARATOR!(), "index.html")),
                info.gt,
                info.challenge,
                if info.new_captcha { "true" } else { "false" }
            );

            warp::reply::html(html)
        });

        let (tx, mut rx) = mpsc::channel(1);
        let validate = warp::path!("validate" / String).map(move |validate| {
            tx.try_send(validate).unwrap();
            String::from("Verification is successful, you can close the browser now")
        });

        let (stop_tx, stop_rx) = oneshot::channel();
        let (addr, server) = warp::serve(index.or(js).or(validate)).bind_with_graceful_shutdown(
            // TODO 处理端口被占用的情况
            ([127, 0, 0, 1], 3030),
            async {
                stop_rx.await.ok();
            },
        );
        tokio::task::spawn(server);

        opener::open_browser(format!("http://{}:{}/captcha", addr.ip(), addr.port()))
            .location(here!())?;

        let validate = rx.recv().await.unwrap();
        let _ = stop_tx.send(());

        Ok(validate)
    }

    async fn sms_login<T, E>(&self, username: T, password: E) -> Result<(String, String), Error>
    where
        T: AsRef<str> + Send + Sync,
        E: AsRef<str> + Send + Sync,
    {
        let account = String::default();

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros() as f64
            / 1000000.0;

        let md5 = hash::hash(
            MessageDigest::md5(),
            format!("{account}{timestamp}").as_bytes(),
        )
        .location(here!())?;

        let response: SendVerifyCodeResponse = self
            .post(
                "/signup/send_verify_code",
                &SendVerifyCodeRequest {
                    account,
                    app_version: CiweimaoClient::APP_VERSION.to_string(),
                    device_token: CiweimaoClient::DEVICE_TOKEN.to_string(),
                    hashvalue: hex_simd::encode_to_string(md5, AsciiCase::Lower),
                    login_name: username.as_ref().to_string(),
                    timestamp: timestamp.to_string(),
                    verify_type: String::from("5"),
                },
            )
            .await
            .location(here!())?;
        check_response(&response.code, &response.tip).location(here!())?;

        print!("Please enter SMS verification code: ");
        io::stdout().flush().location(here!())?;

        let mut ver_code = String::new();
        io::stdin().read_line(&mut ver_code).location(here!())?;

        let response: LoginResponse = self
            .post(
                "/signup/login",
                &LoginSMSRequest {
                    app_version: CiweimaoClient::APP_VERSION.to_string(),
                    device_token: CiweimaoClient::DEVICE_TOKEN.to_string(),
                    login_name: username.as_ref().to_string(),
                    passwd: password.as_ref().to_string(),
                    to_code: response.data.unwrap().to_code,
                    ver_code: ver_code.trim().to_string(),
                },
            )
            .await
            .location(here!())?;
        check_response(&response.code, &response.tip).location(here!())?;

        let data = response.data.unwrap();
        Ok((data.reader_info.account, data.login_token))
    }

    // TODO /chapter/get_chapter_cmd_s
    async fn chapter_cmd<T>(&self, identifier: T) -> Result<String, Error>
    where
        T: AsRef<str>,
    {
        let response: ChapterCmdResponse = self
            .post(
                "/chapter/get_chapter_cmd",
                &ChapterCmdRequest {
                    app_version: CiweimaoClient::APP_VERSION.to_string(),
                    device_token: CiweimaoClient::DEVICE_TOKEN.to_string(),
                    account: self.account(),
                    login_token: self.login_token(),
                    chapter_id: identifier.as_ref().to_string(),
                },
            )
            .await
            .location(here!())?;
        check_response(&response.code, &response.tip).location(here!())?;

        Ok(response.data.unwrap().command)
    }

    fn parse_data_time<T>(str: T) -> Option<NaiveDateTime>
    where
        T: AsRef<str>,
    {
        let str = str.as_ref();
        if str.is_empty() {
            return None;
        }

        match NaiveDateTime::from_str(&str.replace(' ', "T")) {
            Ok(data_time) => Some(data_time),
            Err(error) => {
                warn!("data_time parse failed: {error}, content: {str}");

                None
            }
        }
    }

    fn parse_number<T, E>(str: T) -> Option<E>
    where
        T: AsRef<str>,
        E: FromStr,
    {
        let str = str.as_ref();
        if str.is_empty() {
            return None;
        }

        match str.parse::<E>() {
            Ok(word_count) => Some(word_count),
            Err(_) => {
                warn!("number parse failed: conetent: {str}");
                None
            }
        }
    }

    fn parse_bool<T>(str: T) -> Option<bool>
    where
        T: AsRef<str>,
    {
        let str = str.as_ref();
        if str.is_empty() {
            return None;
        }

        if str == "1" {
            Some(true)
        } else {
            Some(false)
        }
    }

    fn parse_url<T>(str: T) -> Option<Url>
    where
        T: AsRef<str>,
    {
        let str = str.as_ref();
        if str.is_empty() {
            return None;
        }

        match Url::parse(str) {
            Ok(url) => Some(url),
            Err(error) => {
                warn!("Url parse failed: {error}: {str}");
                None
            }
        }
    }

    fn parse_tags<T>(str: T) -> Option<Vec<Tag>>
    where
        T: AsRef<str>,
    {
        let str = str.as_ref();
        if str.is_empty() {
            return None;
        }

        let mut tags: Vec<Tag> = vec![];
        for tag in str.split(',') {
            tags.push(Tag {
                id: None,
                name: tag.trim().to_string(),
            });
        }

        if tags.is_empty() {
            None
        } else {
            Some(tags)
        }
    }

    fn parse_genre<T>(str: T) -> Option<String>
    where
        T: AsRef<str>,
    {
        let str = str.as_ref();
        if str.is_empty() {
            return None;
        }

        match str.parse::<u8>() {
            Ok(index) => match CATEGORIES.get(&index) {
                Some(str) => Some(str.to_string()),
                None => {
                    warn!("The category index does not exist: {str}");
                    None
                }
            },
            Err(error) => {
                warn!("`category_index` parse failed: {error}");
                None
            }
        }
    }

    fn parse_introduction<T>(str: T) -> Option<Vec<String>>
    where
        T: AsRef<str>,
    {
        let str = str.as_ref();
        if str.is_empty() {
            return None;
        }

        let introduction = str
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

    fn parse_image_url<T>(str: T) -> Option<Url>
    where
        T: AsRef<str>,
    {
        let str = str.as_ref();
        if str.is_empty() {
            return None;
        }

        let fragment = Html::parse_fragment(str);
        let selector = Selector::parse("img").unwrap();

        let element = fragment.select(&selector).next();
        if element.is_none() {
            warn!("No `img` element exists: {str}");
            return None;
        }
        let element = element.unwrap();

        let url = element.value().attr("src");
        if url.is_none() {
            warn!("No `src` element exists: {str}");
            return None;
        }
        let url = url.unwrap();

        CiweimaoClient::parse_url(url)
    }

    async fn shelf_list(&self) -> Result<u32, Error> {
        let response: ShelfListResponse = self
            .post(
                "/bookshelf/get_shelf_list",
                &ShelfListRequest {
                    app_version: CiweimaoClient::APP_VERSION.to_string(),
                    device_token: CiweimaoClient::DEVICE_TOKEN.to_string(),
                    account: self.account(),
                    login_token: self.login_token(),
                },
            )
            .await
            .location(here!())?;
        check_response(&response.code, &response.tip).location(here!())?;

        Ok(response.data.unwrap().shelf_list[0]
            .shelf_id
            .parse::<u32>()
            .location(here!())?)
    }
}
