//! task schema v2：任务开始时间、状态（active/done/terminated）、多负责人表与回填。

use sea_orm_migration::prelude::*;

/// v2 迁移。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();
        conn.execute_unprepared("ALTER TABLE tasks ADD COLUMN start_at timestamptz")
            .await?;
        conn.execute_unprepared("ALTER TABLE tasks ADD COLUMN status text NOT NULL DEFAULT 'active'")
            .await?;
        conn.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS task_assignees (\
                task_id uuid NOT NULL, user_id uuid NOT NULL, assigned_at timestamptz NOT NULL, \
                PRIMARY KEY (task_id, user_id))",
        )
        .await?;
        conn.execute_unprepared(
            "INSERT INTO task_assignees (task_id, user_id, assigned_at) \
             SELECT id, assignee_id, created_at FROM tasks WHERE assignee_id IS NOT NULL \
             ON CONFLICT DO NOTHING",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
