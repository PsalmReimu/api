mod structure;
mod utils;

use std::{
    io::{self, Cursor, Write},
    path::{Path, PathBuf},
    str::FromStr,
    time::{SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use boring::{
    hash::{self, MessageDigest},
    sha,
};
use chrono::NaiveDateTime;
use hex_simd::AsciiCase;
use image::{io::Reader, DynamicImage};
use parking_lot::RwLock;
use scraper::{Html, Selector};
use tokio::sync::{mpsc, oneshot, OnceCell};
use tracing::{info, warn};
use url::Url;
use warp::{http::Response, Filter};

use crate::{
    Category, ChapterInfo, Client, ContentInfo, ContentInfos, Error, FindImageResult,
    FindTextResult, HTTPClient, Identifier, NovelDB, NovelInfo, Tag, UserInfo, VolumeInfo,
    VolumeInfos,
};
use structure::*;

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
        Ok(self.client().await?.add_cookie(cookie_str, url)?)
    }

    fn shutdown(&self) -> Result<(), Error> {
        self.shutdown()
    }

    async fn login<T, E>(&self, username: T, password: E) -> Result<(), Error>
    where
        T: AsRef<str> + Send + Sync,
        E: AsRef<str> + Send + Sync,
    {
        let (account, login_token);

        match self.verify_type(&username).await? {
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

        self.save_token(account, login_token);

        Ok(())
    }

    async fn user_info(&self) -> Result<Option<UserInfo>, Error> {
        if !self.has_token() {
            return Ok(None);
        }

        let response: UserInfoResponse = self
            .post(
                "/reader/get_my_info",
                &UserInfoRequest {
                    app_version: CiweimaoClient::APP_VERSION,
                    device_token: CiweimaoClient::DEVICE_TOKEN,
                    account: self.account(),
                    login_token: self.login_token(),
                },
            )
            .await?;
        if response.code == CiweimaoClient::LOGIN_EXPIRED {
            return Ok(None);
        }
        check_response(response.code, response.tip)?;

        let user_info = UserInfo {
            nickname: response.data.unwrap().reader_info.reader_name,
        };

        Ok(Some(user_info))
    }

    async fn novel_info(&self, id: u32) -> Result<Option<NovelInfo>, Error> {
        let response: NovelInfoResponse = self
            .post(
                "/book/get_info_by_id",
                &NovelInfoRequest {
                    app_version: CiweimaoClient::APP_VERSION,
                    device_token: CiweimaoClient::DEVICE_TOKEN,
                    account: self.account(),
                    login_token: self.login_token(),
                    book_id: id,
                },
            )
            .await?;
        if response.code == CiweimaoClient::NOT_FOUND {
            return Ok(None);
        }
        check_response(response.code, response.tip)?;

        let data = response.data.unwrap().book_info;
        let novel_info = NovelInfo {
            id,
            name: data.book_name,
            author_name: data.author_name,
            cover_url: CiweimaoClient::parse_url(data.cover),
            introduction: CiweimaoClient::parse_introduction(data.description),
            word_count: CiweimaoClient::parse_number(data.total_word_count),
            finished: CiweimaoClient::parse_bool(data.up_status),
            create_time: CiweimaoClient::parse_data_time(data.newtime),
            update_time: CiweimaoClient::parse_data_time(data.uptime),
            category: self.parse_category(data.category_index).await?,
            tags: CiweimaoClient::parse_tags(data.tag),
        };

        Ok(Some(novel_info))
    }

    async fn volume_infos(&self, id: u32) -> Result<VolumeInfos, Error> {
        let response: VolumesResponse = self
            .post(
                "/chapter/get_updated_chapter_by_division_new",
                &VolumesRequest {
                    app_version: CiweimaoClient::APP_VERSION,
                    device_token: CiweimaoClient::DEVICE_TOKEN,
                    account: self.account(),
                    login_token: self.login_token(),
                    book_id: id,
                },
            )
            .await?;
        check_response(response.code, response.tip)?;

        let mut volume_infos = VolumeInfos::new();
        for item in response.data.unwrap().chapter_list {
            let mut volume_info = VolumeInfo {
                title: item.division_name,
                chapter_infos: Vec::new(),
            };

            for chapter in item.chapter_list {
                let chapter_info = ChapterInfo {
                    identifier: Identifier::Id(chapter.chapter_id.parse::<u32>()?),
                    title: chapter.chapter_title,
                    word_count: CiweimaoClient::parse_number(chapter.word_count),
                    update_time: CiweimaoClient::parse_data_time(chapter.mtime),
                    is_vip: None,
                    accessible: CiweimaoClient::parse_bool(chapter.auth_access),
                    is_valid: CiweimaoClient::parse_bool(chapter.is_valid),
                };

                volume_info.chapter_infos.push(chapter_info);
            }

            volume_infos.push(volume_info);
        }

        Ok(volume_infos)
    }

    async fn content_infos(&self, info: &ChapterInfo) -> Result<ContentInfos, Error> {
        let content;

        match self.db().await?.find_text(info).await? {
            FindTextResult::Ok(str) => {
                content = str;
            }
            other => {
                let identifier = info.identifier.to_string();

                let cmd = self.chapter_cmd(&identifier).await?;
                let aes_key = sha::sha256(cmd.as_bytes());

                let response: ChapsResponse = self
                    .post(
                        "/chapter/get_cpt_ifm",
                        &ChapsRequest {
                            app_version: CiweimaoClient::APP_VERSION,
                            device_token: CiweimaoClient::DEVICE_TOKEN,
                            account: self.account(),
                            login_token: self.login_token(),
                            chapter_id: identifier,
                            chapter_command: cmd,
                        },
                    )
                    .await?;
                check_response(response.code, response.tip)?;

                let conetent = CiweimaoClient::aes_256_cbc_base64_decrypt(
                    aes_key,
                    response.data.unwrap().chapter_info.txt_content,
                )?;
                content = simdutf8::basic::from_utf8(&conetent)?.to_string();

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
            if line.starts_with("<img") {
                if let Some(url) = CiweimaoClient::parse_image_url(line) {
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
        let response: SearchResponse = self
            .post(
                "/bookcity/get_filter_search_book_list",
                &SearchRequest {
                    app_version: CiweimaoClient::APP_VERSION,
                    device_token: CiweimaoClient::DEVICE_TOKEN,
                    account: self.account(),
                    login_token: self.login_token(),
                    key: text.as_ref().to_string(),
                    count: size,
                    page,
                },
            )
            .await?;
        check_response(response.code, response.tip)?;

        let mut result = Vec::new();
        if response.data.is_some() {
            for novel_info in response.data.unwrap().book_list {
                result.push(novel_info.book_id.parse::<u32>()?);
            }
        }

        Ok(result)
    }

    async fn favorite_infos(&self) -> Result<Vec<u32>, Error> {
        let shelf_id = self.shelf_list().await?;

        let response: FavoritesResponse = self
            .post(
                "/bookshelf/get_shelf_book_list_new",
                &FavoritesRequest {
                    app_version: CiweimaoClient::APP_VERSION,
                    device_token: CiweimaoClient::DEVICE_TOKEN,
                    account: self.account(),
                    login_token: self.login_token(),
                    shelf_id,
                },
            )
            .await?;
        check_response(response.code, response.tip)?;

        let mut result = Vec::new();
        if response.data.is_some() {
            for novel_info in response.data.unwrap().book_list {
                result.push(novel_info.book_info.book_id.parse::<u32>()?);
            }
        }

        Ok(result)
    }

    async fn category_info(&self) -> Result<&Vec<Category>, Error> {
        static CATEGORIES: OnceCell<Vec<Category>> = OnceCell::const_new();

        CATEGORIES
            .get_or_try_init(|| async {
                let response: CategoryResponse = self
                    .post(
                        "/meta/get_meta_data",
                        &CategoryRequest {
                            app_version: CiweimaoClient::APP_VERSION,
                            device_token: CiweimaoClient::DEVICE_TOKEN,
                            account: self.account(),
                            login_token: self.login_token(),
                        },
                    )
                    .await?;
                check_response(response.code, response.tip)?;

                let mut result = Vec::new();
                for category in response.data.unwrap().category_list {
                    for category_detail in category.category_detail {
                        result.push(Category {
                            id: CiweimaoClient::parse_number(category_detail.category_index),
                            name: category_detail.category_name,
                        });
                    }
                }

                Ok(result)
            })
            .await
    }

    async fn tag_infos(&self) -> Result<&Vec<Tag>, Error> {
        static TAGS: OnceCell<Vec<Tag>> = OnceCell::const_new();

        TAGS.get_or_try_init(|| async {
            let response: TagResponse = self
                .post(
                    "/book/get_official_tag_list",
                    &TagRequest {
                        app_version: CiweimaoClient::APP_VERSION,
                        device_token: CiweimaoClient::DEVICE_TOKEN,
                        account: self.account(),
                        login_token: self.login_token(),
                    },
                )
                .await?;
            check_response(response.code, response.tip)?;

            let mut result = Vec::new();
            for tag in response.data.unwrap().official_tag_list {
                result.push(Tag {
                    id: None,
                    name: tag.tag_name,
                });
            }

            Ok(result)
        })
        .await
    }
}

#[must_use]
enum VerifyType {
    None,
    Geetest,
    VerifyCode,
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
                    app_version: CiweimaoClient::APP_VERSION,
                    device_token: CiweimaoClient::DEVICE_TOKEN,
                    login_name: username.as_ref().to_string(),
                },
            )
            .await?;
        check_response(response.code, response.tip)?;

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
                    app_version: CiweimaoClient::APP_VERSION,
                    device_token: CiweimaoClient::DEVICE_TOKEN,
                    login_name: username.as_ref().to_string(),
                    passwd: password.as_ref().to_string(),
                },
            )
            .await?;
        check_response(response.code, response.tip)?;

        let data = response.data.unwrap();
        Ok((data.reader_info.account, data.login_token))
    }

    async fn geetest_login<T, E>(&self, username: T, password: E) -> Result<(String, String), Error>
    where
        T: AsRef<str> + Send + Sync,
        E: AsRef<str> + Send + Sync,
    {
        let info = self.geetest_info(&username).await?;
        let geetest_challenge = info.challenge.clone();

        let validate = CiweimaoClient::run_server(info).await?;

        let response: LoginResponse = self
            .post(
                "/signup/login",
                &LoginCaptchaRequest {
                    app_version: CiweimaoClient::APP_VERSION,
                    device_token: CiweimaoClient::DEVICE_TOKEN,
                    login_name: username.as_ref().to_string(),
                    passwd: password.as_ref().to_string(),
                    geetest_seccode: validate.to_string() + "|jordan",
                    geetest_validate: validate,
                    geetest_challenge,
                },
            )
            .await?;
        check_response(response.code, response.tip)?;

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
            .await?
            .json::<GeetestInfoResponse>()
            .await?;

        if response.success != 1 {
            return Err(Error::NovelApi(
                "`/signup/geetest_first_register` failed".to_string(),
            ));
        }

        Ok(response)
    }

    // TODO 更美观的页面
    async fn run_server(info: GeetestInfoResponse) -> Result<String, Error> {
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
            (
                [127, 0, 0, 1],
                portpicker::pick_unused_port().expect("No ports free"),
            ),
            async {
                stop_rx.await.ok();
            },
        );
        tokio::task::spawn(server);

        opener::open_browser(format!("http://{}:{}/captcha", addr.ip(), addr.port()))?;

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
        )?;

        let response: SendVerifyCodeResponse = self
            .post(
                "/signup/send_verify_code",
                &SendVerifyCodeRequest {
                    account,
                    app_version: CiweimaoClient::APP_VERSION,
                    device_token: CiweimaoClient::DEVICE_TOKEN,
                    hashvalue: hex_simd::encode_to_string(md5, AsciiCase::Lower),
                    login_name: username.as_ref().to_string(),
                    timestamp: timestamp.to_string(),
                    verify_type: String::from("5"),
                },
            )
            .await?;
        check_response(response.code, response.tip)?;

        print!("Please enter SMS verification code: ");
        io::stdout().flush()?;

        let mut ver_code = String::new();
        io::stdin().read_line(&mut ver_code)?;

        let response: LoginResponse = self
            .post(
                "/signup/login",
                &LoginSMSRequest {
                    app_version: CiweimaoClient::APP_VERSION,
                    device_token: CiweimaoClient::DEVICE_TOKEN,
                    login_name: username.as_ref().to_string(),
                    passwd: password.as_ref().to_string(),
                    to_code: response.data.unwrap().to_code,
                    ver_code: ver_code.trim().to_string(),
                },
            )
            .await?;
        check_response(response.code, response.tip)?;

        let data = response.data.unwrap();
        Ok((data.reader_info.account, data.login_token))
    }

    // TODO use /chapter/get_chapter_cmd_s
    async fn chapter_cmd<T>(&self, identifier: T) -> Result<String, Error>
    where
        T: AsRef<str>,
    {
        let response: ChapterCmdResponse = self
            .post(
                "/chapter/get_chapter_cmd",
                &ChapterCmdRequest {
                    app_version: CiweimaoClient::APP_VERSION,
                    device_token: CiweimaoClient::DEVICE_TOKEN,
                    account: self.account(),
                    login_token: self.login_token(),
                    chapter_id: identifier.as_ref().to_string(),
                },
            )
            .await?;
        check_response(response.code, response.tip)?;

        Ok(response.data.unwrap().command)
    }

    async fn shelf_list(&self) -> Result<u32, Error> {
        let response: ShelfListResponse = self
            .post(
                "/bookshelf/get_shelf_list",
                &ShelfListRequest {
                    app_version: CiweimaoClient::APP_VERSION,
                    device_token: CiweimaoClient::DEVICE_TOKEN,
                    account: self.account(),
                    login_token: self.login_token(),
                },
            )
            .await?;
        check_response(response.code, response.tip)?;

        Ok(response.data.unwrap().shelf_list[0]
            .shelf_id
            .parse::<u32>()?)
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
                warn!("`NaiveDateTime` parse failed: {error}, content: {str}");

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
                warn!("`Number` parse failed: conetent: {str}");
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
                warn!("`Url` parse failed: {error}, content: {str}");
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

    async fn parse_category<T>(&self, str: T) -> Result<Option<Category>, Error>
    where
        T: AsRef<str>,
    {
        let str = str.as_ref();
        if str.is_empty() {
            return Ok(None);
        }

        let categories = self.category_info().await?;

        match str.parse::<u16>() {
            Ok(index) => match categories.iter().find(|item| item.id == Some(index)) {
                Some(category) => Ok(Some(Category {
                    id: category.id,
                    name: category.name.clone(),
                })),
                None => {
                    warn!("The category index does not exist: {str}");
                    Ok(None)
                }
            },
            Err(error) => {
                warn!("`category_index` parse failed: {error}");
                Ok(None)
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
            warn!("No `src` attribute exists: {str}");
            return None;
        }
        let url = url.unwrap();

        CiweimaoClient::parse_url(url)
    }
}
