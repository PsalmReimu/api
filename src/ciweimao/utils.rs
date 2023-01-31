use boring::{
    sha,
    symm::{self, Cipher},
};
use once_cell::sync::OnceCell as SyncOnceCell;
use parking_lot::RwLock;
use reqwest::Response;
use semver::{Version, VersionReq};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::{fs, sync::OnceCell};
use tracing::{info, warn};
use url::Url;

use crate::{CiweimaoClient, Error, HTTPClient, NovelDB};

#[must_use]
#[derive(Serialize, Deserialize)]
struct Config {
    version: Version,
    account: String,
    login_token: String,
}

impl CiweimaoClient {
    const APP_NAME: &str = "ciweimao";

    pub(crate) const OK: &str = "100000";
    pub(crate) const LOGIN_EXPIRED: &str = "200100";
    pub(crate) const NOT_FOUND: &str = "320001";

    pub(crate) const APP_VERSION: &str = "2.9.293";
    pub(crate) const DEVICE_TOKEN: &str = "ciweimao_";

    const HOST: &str = "https://app.hbooker.com";

    const CONFIG_FILE_NAME: &str = "config.toml";
    const CONFIG_VERSION: &str = "0.1.0";

    // TODO use iOS side
    const USER_AGENT: &str =
        "Android  com.kuangxiangciweimao.novel  2.9.293,OnePlus, ONEPLUS A3010, 25, 7.1.1";
    const USER_AGENT_RSS: &str =
        "Dalvik/2.1.0 (Linux; U; Android 7.1.1; ONEPLUS A3010 Build/NMF26F)";

    const AES_KEY: &str = "zG2nSeEfSHfvTCHy5LCcqtBbQehKNLXn";

    /// Create a ciweimao client
    pub async fn new() -> Result<Self, Error> {
        let (account, login_token) = CiweimaoClient::load_config_file().await?;

        Ok(Self {
            proxy: None,
            no_proxy: false,
            cert_path: None,
            client: OnceCell::new(),
            client_rss: OnceCell::new(),
            db: OnceCell::new(),
            account: RwLock::new(account),
            login_token: RwLock::new(login_token),
        })
    }

    async fn load_config_file() -> Result<(Option<String>, Option<String>), Error> {
        let mut config_file_path = crate::config_dir_path(CiweimaoClient::APP_NAME)?;
        config_file_path.push(CiweimaoClient::CONFIG_FILE_NAME);

        if config_file_path.exists() {
            info!(
                "The config file is located at: `{}`",
                config_file_path.display()
            );

            let config = fs::read_to_string(config_file_path).await?;
            let config: Config = toml::from_str(&config)?;

            let req = VersionReq::parse(&format!("^{}", CiweimaoClient::CONFIG_VERSION))?;
            if !req.matches(&config.version) {
                warn!("Ignoring the configuration file because the configuration file version is incompatible");
                Ok((None, None))
            } else {
                Ok((Some(config.account), Some(config.login_token)))
            }
        } else {
            info!(
                "The config file will be created at: `{}`",
                config_file_path.display()
            );

            Ok((None, None))
        }
    }

    #[must_use]
    #[inline]
    pub(crate) fn account(&self) -> String {
        self.account.read().as_ref().unwrap().to_string()
    }

    #[must_use]
    #[inline]
    pub(crate) fn login_token(&self) -> String {
        self.login_token.read().as_ref().unwrap().to_string()
    }

    #[must_use]
    pub(crate) fn has_token(&self) -> bool {
        self.account.read().is_some() && self.login_token.read().is_some()
    }

    pub(crate) fn save_token(&self, account: String, login_token: String) {
        *self.account.write() = Some(account);
        *self.login_token.write() = Some(login_token);
    }

    #[inline]
    pub(crate) async fn client(&self) -> Result<&HTTPClient, Error> {
        self.client
            .get_or_try_init(|| async {
                HTTPClient::builder(CiweimaoClient::APP_NAME)
                    .accept("*/*")
                    .accept_language("zh-Hans-CN;q=1")
                    .user_agent(CiweimaoClient::USER_AGENT)
                    .allow_compress(false)
                    .proxy(self.proxy.clone())
                    .no_proxy(self.no_proxy)
                    .cert(self.cert_path.clone())
                    .build()
                    .await
            })
            .await
    }

    #[inline]
    pub(crate) async fn client_rss(&self) -> Result<&HTTPClient, Error> {
        self.client_rss
            .get_or_try_init(|| async {
                HTTPClient::builder(CiweimaoClient::APP_NAME)
                    .accept("image/*,*/*;q=0.8")
                    .accept_language("zh-CN,zh-Hans;q=0.9")
                    .user_agent(CiweimaoClient::USER_AGENT_RSS)
                    .proxy(self.proxy.clone())
                    .no_proxy(self.no_proxy)
                    .cert(self.cert_path.clone())
                    .build()
                    .await
            })
            .await
    }

    #[inline]
    pub(crate) async fn db(&self) -> Result<&NovelDB, Error> {
        self.db
            .get_or_try_init(|| async { NovelDB::new(CiweimaoClient::APP_NAME).await })
            .await
    }

    #[inline]
    pub(crate) async fn get_query<T, E>(&self, url: T, query: &E) -> Result<Response, Error>
    where
        T: AsRef<str>,
        E: Serialize,
    {
        let response = self
            .client()
            .await?
            .get(CiweimaoClient::HOST.to_string() + url.as_ref())
            .query(query)
            .send()
            .await?;
        crate::check_status(
            response.status(),
            format!("HTTP request failed: `{}`", url.as_ref()),
        )?;

        Ok(response)
    }

    #[inline]
    pub(crate) async fn get_rss(&self, url: &Url) -> Result<Response, Error> {
        let response = self.client_rss().await?.get(url.clone()).send().await?;
        crate::check_status(response.status(), format!("HTTP request failed: `{url}`"))?;

        Ok(response)
    }

    #[inline]
    pub(crate) async fn post<T, E, R>(&self, url: T, form: &E) -> Result<R, Error>
    where
        T: AsRef<str>,
        E: Serialize,
        R: DeserializeOwned,
    {
        let response = self
            .client()
            .await?
            .post(CiweimaoClient::HOST.to_string() + url.as_ref())
            .form(form)
            .send()
            .await?;
        crate::check_status(
            response.status(),
            format!("HTTP request failed: `{}`", url.as_ref()),
        )?;

        let bytes = response.bytes().await?;
        let bytes =
            CiweimaoClient::aes_256_cbc_base64_decrypt(CiweimaoClient::get_default_key(), &bytes)?;

        Ok(serde_json::from_str(simdutf8::basic::from_utf8(&bytes)?)?)
    }

    #[must_use]
    #[inline]
    fn get_default_key() -> &'static [u8; 32] {
        static AES_KEY: SyncOnceCell<[u8; 32]> = SyncOnceCell::new();
        AES_KEY.get_or_init(|| sha::sha256(CiweimaoClient::AES_KEY.as_bytes()))
    }

    #[inline]
    pub(crate) fn aes_256_cbc_base64_decrypt<T, E>(key: T, data: E) -> Result<Vec<u8>, Error>
    where
        T: AsRef<[u8]>,
        E: AsRef<[u8]>,
    {
        let base64 = base64_simd::STANDARD;
        let decoded = base64.decode_to_vec(data.as_ref())?;

        let cipher = Cipher::aes_256_cbc();
        let result = symm::decrypt(cipher, key.as_ref(), Some(&[0; 16]), &decoded)?;

        Ok(result)
    }
}

impl Drop for CiweimaoClient {
    fn drop(&mut self) {
        if self.has_token() {
            let config = Config {
                version: Version::parse(CiweimaoClient::CONFIG_VERSION).unwrap(),
                account: self.account(),
                login_token: self.login_token(),
            };

            let mut config_file_path = crate::config_dir_path(CiweimaoClient::APP_NAME)
                .expect("Failed to obtain configuration file path");
            config_file_path.push(CiweimaoClient::CONFIG_FILE_NAME);

            std::fs::write(&config_file_path, toml::to_string(&config).unwrap())
                .expect("Configuration file save failed");

            info!("Save the config file at: `{}`", config_file_path.display());
        } else {
            info!("No data can be saved to the configuration file");
        }
    }
}
