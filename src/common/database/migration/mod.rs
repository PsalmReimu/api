mod m20221215_070928_create_table;

use async_trait::async_trait;
pub use sea_orm_migration::prelude::*;

#[must_use]
pub struct Migrator;

#[async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(m20221215_070928_create_table::Migration)]
    }
}
