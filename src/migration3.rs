//! task schema v3：任务附件表。

use sea_orm_migration::prelude::*;

/// v3 迁移。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE TABLE IF NOT EXISTS task_attachments (\
                    id uuid PRIMARY KEY, task_id uuid NOT NULL, name text NOT NULL, \
                    size bigint NOT NULL, mime text NOT NULL, storage_key text NOT NULL, \
                    uploader_id uuid NOT NULL, created_at timestamptz NOT NULL)",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
