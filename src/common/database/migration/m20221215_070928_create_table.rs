use async_trait::async_trait;
use sea_orm_migration::prelude::*;

#[must_use]
#[derive(Iden)]
enum Text {
    Table,
    Identifier,
    DateTime,
    Text,
}

#[must_use]
#[derive(Iden)]
enum Image {
    Table,
    Url,
    Image,
}

#[must_use]
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Text::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Text::Identifier)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Text::DateTime).date_time())
                    .col(ColumnDef::new(Text::Text).binary().not_null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Image::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Image::Url).string().not_null().primary_key())
                    .col(ColumnDef::new(Image::Image).binary().not_null())
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Text::Table).if_exists().to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(Image::Table).if_exists().to_owned())
            .await?;

        Ok(())
    }
}
