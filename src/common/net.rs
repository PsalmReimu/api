use std::{
    io::BufWriter,
    ops::Deref,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use reqwest::{
    header::{HeaderMap, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, CONNECTION},
    Certificate, Client, Proxy,
};
use reqwest_cookie_store::{CookieStore, CookieStoreMutex};
use tokio::fs;
use tracing::info;
use url::Url;

use crate::{config_dir_path, Error};

const COOKIE_FILE_NAME: &str = "cookie.json";

#[must_use]
pub(crate) struct HTTPClientBuilder {
    app_name: &'static str,
    accept: HeaderValue,
    accept_language: HeaderValue,
    user_agent: String,
    cookie: bool,
    proxy: Option<Url>,
    no_proxy: bool,
    cert_path: Option<PathBuf>,
}

impl HTTPClientBuilder {
    pub(crate) fn new(app_name: &'static str) -> Self {
        Self {
            app_name,
            accept: HeaderValue::from_static(
                "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.9",
            ),
            accept_language: HeaderValue::from_static("zh-CN,zh;q=0.9"),
            user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/109.0.0.0 Safari/537.36".to_string(),
            cookie: false,
            proxy: None,
            no_proxy: false,
            cert_path: None,
        }
    }

    pub(crate) fn accept(self, accept: &'static str) -> Self {
        Self {
            accept: HeaderValue::from_static(accept),
            ..self
        }
    }

    pub(crate) fn accept_language(self, accept_language: &'static str) -> Self {
        Self {
            accept_language: HeaderValue::from_static(accept_language),
            ..self
        }
    }

    pub(crate) fn user_agent<T>(self, user_agent: T) -> Self
    where
        T: AsRef<str>,
    {
        Self {
            user_agent: user_agent.as_ref().to_string(),
            ..self
        }
    }

    pub(crate) fn cookie(self, flag: bool) -> Self {
        Self {
            cookie: flag,
            ..self
        }
    }

    pub(crate) fn proxy(self, proxy: Option<Url>) -> Self {
        Self { proxy, ..self }
    }

    pub(crate) fn no_proxy(self, flag: bool) -> Self {
        Self {
            no_proxy: flag,
            ..self
        }
    }

    pub(crate) fn cert<T>(self, cert_path: Option<T>) -> Self
    where
        T: AsRef<Path>,
    {
        Self {
            cert_path: cert_path.map(|path| path.as_ref().to_path_buf()),
            ..self
        }
    }

    pub(crate) async fn build(self) -> Result<HTTPClient, Error> {
        let mut cookie_store = None;
        if self.cookie {
            cookie_store = Some(Arc::new(self.create_cookie_store().await?));
        }

        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, self.accept);
        headers.insert(ACCEPT_LANGUAGE, self.accept_language);
        headers.insert(CONNECTION, HeaderValue::from_static("keep-alive"));

        let mut client_builder = Client::builder()
            .default_headers(headers)
            .http2_keep_alive_interval(Duration::from_secs(10))
            .http2_keep_alive_timeout(Duration::from_secs(60))
            .user_agent(self.user_agent);

        if self.cookie {
            client_builder =
                client_builder.cookie_provider(Arc::clone(cookie_store.as_ref().unwrap()));
        }

        if let Some(proxy) = self.proxy {
            client_builder = client_builder.proxy(Proxy::all(proxy)?);
        }

        if self.no_proxy {
            client_builder = client_builder.no_proxy();
        }

        if let Some(cert_path) = self.cert_path {
            let cert = Certificate::from_pem(&fs::read(cert_path).await?)?;
            client_builder = client_builder.add_root_certificate(cert);
        }

        Ok(HTTPClient {
            app_name: self.app_name,
            cookie_store,
            client: client_builder.build()?,
        })
    }

    async fn create_cookie_store(&self) -> Result<CookieStoreMutex, Error> {
        let mut config_dir = config_dir_path(self.app_name)?;
        fs::create_dir_all(&config_dir).await?;

        config_dir.push(COOKIE_FILE_NAME);

        let cookie_store = if !config_dir.exists() {
            info!(
                "The cookie file will be created at: `{}`",
                config_dir.display()
            );

            CookieStore::default()
        } else {
            info!("The cookie file is located at: `{}`", config_dir.display());

            let json = fs::read(&config_dir).await?;
            CookieStore::load_json(json.as_slice())?
        };

        Ok(CookieStoreMutex::new(cookie_store))
    }
}

#[must_use]
pub(crate) struct HTTPClient {
    app_name: &'static str,
    cookie_store: Option<Arc<CookieStoreMutex>>,
    client: Client,
}

impl HTTPClient {
    pub(crate) fn builder(app_name: &'static str) -> HTTPClientBuilder {
        HTTPClientBuilder::new(app_name)
    }

    pub(crate) fn add_cookie(&self, cookie_str: &str, url: &Url) -> Result<(), Error> {
        self.cookie_store
            .as_ref()
            .expect("Cookies not turned on")
            .lock()
            .unwrap()
            .parse(cookie_str, url)?;

        Ok(())
    }
}

impl Deref for HTTPClient {
    type Target = Client;

    fn deref(&self) -> &Self::Target {
        &self.client
    }
}

impl Drop for HTTPClient {
    fn drop(&mut self) {
        if let Some(ref cookie_store) = self.cookie_store {
            let mut config_path = config_dir_path(self.app_name)
                .expect("Failed to get the path to the project's config directory");
            config_path.push(COOKIE_FILE_NAME);

            info!("Save the cookie file at: `{}`", config_path.display());
            let file = std::fs::File::create(config_path).expect("Failed to open cookie file");

            let mut writer = BufWriter::new(file);
            let store = cookie_store.lock().unwrap();
            store
                .save_json(&mut writer)
                .expect("Failed to save cookie file");
        }
    }
}
