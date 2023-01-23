use anyhow::Result;

use novel_api::{CiweimaoClient, Client};
use tokio::fs;

#[tokio::test]
async fn ciweimao() -> Result<()> {
    let novel_id = 100194379;

    let client = CiweimaoClient::new().await?;

    let novel_info = client.novel_info(novel_id).await?;
    println!("{:#?}", novel_info);

    let volume_infos = client.volume_infos(novel_id).await?;
    println!("{:#?}", volume_infos);

    let content_infos = client
        .content_infos(&volume_infos[0].chapter_infos[0])
        .await?;
    println!("{:#?}", content_infos);

    let image_file_name = "ciweimao-test.webp";
    let image_info = client.image_info(&novel_info.cover_url.unwrap()).await?;
    image_info.save(image_file_name)?;
    fs::remove_file(image_file_name).await?;

    let search_infos = client.search_infos("测试", 0, 12).await?;
    println!("{:?}", search_infos);

    Ok(())
}
