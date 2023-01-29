use http::StatusCode;
use thiserror::Error;

/// novel-api error
#[must_use]
#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    StdIo(#[from] std::io::Error),
    #[error(transparent)]
    StdSystemTime(#[from] std::time::SystemTimeError),
    #[error(transparent)]
    StdParseInt(#[from] std::num::ParseIntError),
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    Boring(#[from] boring::error::ErrorStack),
    #[error(transparent)]
    Base64Simd(#[from] base64_simd::Error),
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
    #[error(transparent)]
    Opener(#[from] opener::OpenError),
    #[error(transparent)]
    Semver(#[from] semver::Error),
    #[error(transparent)]
    Confy(#[from] confy::ConfyError),
    #[error(transparent)]
    Simdutf8(#[from] simdutf8::basic::Utf8Error),
    #[error(transparent)]
    SeaOrm(#[from] sea_orm::DbErr),
    #[error(transparent)]
    Chrono(#[from] chrono::ParseError),
    #[error(transparent)]
    Image(#[from] image::ImageError),
    #[error(transparent)]
    Keyring(#[from] keyring::Error),
    #[error(transparent)]
    Url(#[from] url::ParseError),
    #[error(transparent)]
    Cookie(#[from] cookie_store::CookieError),
    #[error(transparent)]
    CookieStore(#[from] cookie_store::Error),
    #[error(transparent)]
    StatusCode(#[from] http::status::InvalidStatusCode),
    #[error("{0}")]
    NovelApi(String),
    #[error("The HTTP request was unsuccessful, status code: `{code}`, message: `{msg}`")]
    Http { code: StatusCode, msg: String },
}

/// Source code location
#[must_use]
pub struct Location {
    pub file: &'static str,
    pub function_name: &'static str,
    pub line: u32,
    pub column: u32,
}
