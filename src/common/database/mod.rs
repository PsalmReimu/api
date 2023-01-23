mod entity;
mod migration;

use std::io::Cursor;

use async_compression::tokio::{bufread::ZstdDecoder, write::ZstdEncoder};
use image::{io::Reader, DynamicImage};
use sea_orm::{ActiveModelTrait, Database, DatabaseConnection, EntityTrait};
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncWriteExt, BufReader},
};
use tracing::info;
use url::Url;

use crate::{here, ChapterInfo, Error, ErrorLocation, Location, Timing};
use entity::{Image, Text};
use migration::{Migrator, MigratorTrait};

#[must_use]
pub(crate) struct NovelDB {
    db: DatabaseConnection,
}

#[must_use]
#[derive(Debug, PartialEq)]
pub(crate) enum FindTextResult {
    Ok(String),
    None,
    Outdate,
}

#[must_use]
#[derive(Debug, PartialEq)]
pub(crate) enum FindImageResult {
    Ok(DynamicImage),
    None,
}

impl NovelDB {
    pub(crate) async fn new(app_name: &str) -> Result<Self, Error> {
        let mut timing = Timing::new();

        let mut db_path = crate::data_dir_path(app_name).location(here!())?;
        fs::create_dir_all(&db_path).await.location(here!())?;

        db_path.push("novel.db");

        if db_path.exists() {
            info!("The database file is located at `{}`", db_path.display());
        } else {
            info!(
                "The database file will be created at `{}`",
                db_path.display()
            );
        }

        let db_url = format!("sqlite:{}?mode=rwc", db_path.display());
        let db = Database::connect(db_url).await.location(here!())?;
        Migrator::up(&db, None).await.location(here!())?;

        info!("Database creation takes `{}`", timing.elapsed()?);

        Ok(Self { db })
    }

    #[cfg(test)]
    pub(crate) async fn drop(&self) -> Result<(), Error> {
        Ok(Migrator::down(&self.db, None).await.location(here!())?)
    }

    pub(crate) async fn find_text(&self, info: &ChapterInfo) -> Result<FindTextResult, Error> {
        let identifier = info.identifier.to_string();
        let time = info.update_time;

        let model = Text::find_by_id(identifier)
            .one(&self.db)
            .await
            .location(here!())?;

        match model {
            Some(model) => {
                let saved_data_time = model.date_time;

                if time.is_some()
                    && saved_data_time.is_some()
                    && saved_data_time.unwrap() < time.unwrap()
                {
                    Ok(FindTextResult::Outdate)
                } else {
                    Ok(FindTextResult::Ok(unsafe {
                        String::from_utf8_unchecked(
                            zstd_decompress(&model.text).await.location(here!())?,
                        )
                    }))
                }
            }

            None => Ok(FindTextResult::None),
        }
    }

    pub(crate) async fn insert_text<T>(&self, info: &ChapterInfo, text: T) -> Result<(), Error>
    where
        T: AsRef<str>,
    {
        let model = entity::text::ActiveModel {
            identifier: sea_orm::Set(info.identifier.to_string()),
            date_time: sea_orm::Set(info.update_time),
            text: sea_orm::Set(
                zstd_compress(text.as_ref().as_bytes())
                    .await
                    .location(here!())?,
            ),
        };
        model.insert(&self.db).await.location(here!())?;

        Ok(())
    }

    pub(crate) async fn update_text<T>(&self, info: &ChapterInfo, text: T) -> Result<(), Error>
    where
        T: AsRef<str>,
    {
        let model = entity::text::ActiveModel {
            identifier: sea_orm::Set(info.identifier.to_string()),
            date_time: sea_orm::Set(info.update_time),
            text: sea_orm::Set(
                zstd_compress(text.as_ref().as_bytes())
                    .await
                    .location(here!())?,
            ),
        };
        model.update(&self.db).await.location(here!())?;

        Ok(())
    }

    pub(crate) async fn find_image(&self, url: &Url) -> Result<FindImageResult, Error> {
        let model = Image::find_by_id(url.to_string())
            .one(&self.db)
            .await
            .location(here!())?;

        match model {
            Some(model) => {
                let bytes = zstd_decompress(&model.image).await.location(here!())?;
                let image = Reader::new(Cursor::new(bytes))
                    .with_guessed_format()?
                    .decode()
                    .location(here!())?;

                Ok(FindImageResult::Ok(image))
            }
            None => Ok(FindImageResult::None),
        }
    }

    pub(crate) async fn insert_image<T>(&self, url: &Url, bytes: T) -> Result<(), Error>
    where
        T: AsRef<[u8]>,
    {
        let model = entity::image::ActiveModel {
            url: sea_orm::Set(url.to_string()),
            image: sea_orm::Set(zstd_compress(bytes).await.location(here!())?),
        };
        model.insert(&self.db).await.location(here!())?;

        Ok(())
    }
}

async fn zstd_decompress<T>(data: T) -> Result<Vec<u8>, Error>
where
    T: AsRef<[u8]>,
{
    let mut reader = ZstdDecoder::new(BufReader::new(data.as_ref()));
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf).await.location(here!())?;
    Ok(buf)
}

async fn zstd_compress<T>(data: T) -> Result<Vec<u8>, Error>
where
    T: AsRef<[u8]>,
{
    let mut writer = ZstdEncoder::new(Vec::new());
    writer.write_all(data.as_ref()).await.location(here!())?;
    writer.shutdown().await.location(here!())?;

    let mut res = writer.into_inner();
    res.flush().await.location(here!())?;

    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::str::FromStr;

    use chrono::NaiveDateTime;
    use pretty_assertions::assert_eq;

    use crate::Identifier;

    #[tokio::test]
    async fn zstd() -> Result<(), Error> {
        let data = "test-data";

        let compressed_data = zstd_compress(data).await.location(here!())?;
        let decompressed_data = zstd_decompress(compressed_data).await.location(here!())?;

        assert_eq!(data.as_bytes(), decompressed_data.as_slice());

        Ok(())
    }

    impl Default for ChapterInfo {
        fn default() -> Self {
            Self {
                identifier: Identifier::Id(0),
                title: Default::default(),
                is_vip: Default::default(),
                accessible: Default::default(),
                is_valid: Default::default(),
                word_count: Default::default(),
                update_time: Default::default(),
            }
        }
    }

    #[tokio::test]
    async fn db() -> Result<(), Error> {
        let app_name = "test-app";
        let contents = "test-contents";

        let db = NovelDB::new(app_name).await?;

        let chapter_info_old = ChapterInfo {
            identifier: Identifier::Id(0),
            update_time: Some(NaiveDateTime::from_str("2020-07-08T15:25:15")?),
            ..Default::default()
        };

        let chapter_info_new = ChapterInfo {
            identifier: Identifier::Id(0),
            update_time: Some(NaiveDateTime::from_str("2020-07-08T15:25:17")?),
            ..Default::default()
        };

        assert_eq!(db.find_text(&chapter_info_new).await?, FindTextResult::None);

        db.insert_text(&chapter_info_old, contents).await?;
        assert_eq!(
            db.find_text(&chapter_info_new).await?,
            FindTextResult::Outdate
        );

        db.update_text(&chapter_info_new, contents).await?;

        if let FindTextResult::Ok(result) = db.find_text(&chapter_info_new).await? {
            assert_eq!(result, contents);
        } else {
            panic!("Incorrect database query result");
        }

        db.drop().await?;

        Ok(())
    }
}
