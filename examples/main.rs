use anyhow::Result;

use novel_api::{Client, Options, SfacgClient, WordCountRange};
use tokio::fs;

#[tokio::main]
async fn main() -> Result<()> {
    let novel_id = 263060;

    let client = SfacgClient::new().await?;

    let novel_info = client.novel_info(novel_id).await?;
    println!("{novel_info:#?}");

    let volume_infos = client.volume_infos(novel_id).await?;
    println!("{volume_infos:#?}");

    let content_infos = client
        .content_infos(&volume_infos[volume_infos.len() - 1].chapter_infos[volume_infos.len() - 1])
        .await?;
    println!("{content_infos:#?}");

    let image_file_name = "sfacg-test.webp";
    let image_info = client
        .image(&novel_info.unwrap().cover_url.unwrap())
        .await?;
    image_info.save(image_file_name)?;
    fs::remove_file(image_file_name).await?;

    let search_infos = client.search_infos("测试", 0, 12).await?;
    println!("{search_infos:?}");

    let category_infos = client.categories().await?;
    println!("{category_infos:#?}");

    let tag_infos = client.tags().await?;
    println!("{tag_infos:#?}");

    let options = Options {
        tags: Some(vec![tag_infos[0].clone()]),
        word_count: Some(WordCountRange::RangeFrom(90_0000..)),
        ..Default::default()
    };
    let novels = client.novels(&options, 0, 12).await?;
    println!("{novels:#?}");

    Ok(())
}
