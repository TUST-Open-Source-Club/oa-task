//! task schema 迁移（含 outbox 表）。

use sea_orm_migration::prelude::*;

/// 初始化迁移。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("CREATE SCHEMA IF NOT EXISTS task")
            .await?;
        manager
            .get_connection()
            .execute_unprepared(club_bus::outbox::DDL)
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Projects::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Projects::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(Projects::Name).string_len(64).not_null())
                    .col(
                        ColumnDef::new(Projects::Description)
                            .text()
                            .not_null()
                            .default(""),
                    )
                    .col(ColumnDef::new(Projects::OwnerId).uuid().not_null())
                    .col(ColumnDef::new(Projects::ArchivedAt).timestamp_with_time_zone())
                    .col(
                        ColumnDef::new(Projects::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(ProjectMembers::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(ProjectMembers::ProjectId).uuid().not_null())
                    .col(ColumnDef::new(ProjectMembers::UserId).uuid().not_null())
                    .col(
                        ColumnDef::new(ProjectMembers::Role)
                            .string_len(16)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ProjectMembers::JoinedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .primary_key(
                        Index::create()
                            .col(ProjectMembers::ProjectId)
                            .col(ProjectMembers::UserId),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(BoardColumns::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(BoardColumns::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(BoardColumns::ProjectId).uuid().not_null())
                    .col(ColumnDef::new(BoardColumns::Name).string_len(64).not_null())
                    .col(
                        ColumnDef::new(BoardColumns::Position)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(BoardColumns::IsDone)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(BoardColumns::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Tasks::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Tasks::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(Tasks::ProjectId).uuid().not_null())
                    .col(ColumnDef::new(Tasks::ColumnId).uuid().not_null())
                    .col(ColumnDef::new(Tasks::Title).string_len(255).not_null())
                    .col(
                        ColumnDef::new(Tasks::DescriptionMd)
                            .text()
                            .not_null()
                            .default(""),
                    )
                    .col(ColumnDef::new(Tasks::AssigneeId).uuid())
                    .col(
                        ColumnDef::new(Tasks::Priority)
                            .string_len(16)
                            .not_null()
                            .default("normal"),
                    )
                    .col(ColumnDef::new(Tasks::DueAt).timestamp_with_time_zone())
                    .col(
                        ColumnDef::new(Tasks::Position)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(ColumnDef::new(Tasks::CreatedBy).uuid().not_null())
                    .col(ColumnDef::new(Tasks::CompletedAt).timestamp_with_time_zone())
                    .col(
                        ColumnDef::new(Tasks::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Tasks::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("ix_tasks_project_column")
                    .table(Tasks::Table)
                    .col(Tasks::ProjectId)
                    .col(Tasks::ColumnId)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("ix_tasks_assignee")
                    .table(Tasks::Table)
                    .col(Tasks::AssigneeId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Subtasks::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Subtasks::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(Subtasks::TaskId).uuid().not_null())
                    .col(ColumnDef::new(Subtasks::Title).string_len(255).not_null())
                    .col(
                        ColumnDef::new(Subtasks::Done)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(Subtasks::Position)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(Subtasks::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(TaskComments::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(TaskComments::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(TaskComments::TaskId).uuid().not_null())
                    .col(ColumnDef::new(TaskComments::AuthorId).uuid().not_null())
                    .col(ColumnDef::new(TaskComments::ContentMd).text().not_null())
                    .col(
                        ColumnDef::new(TaskComments::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for table in [
            "task_comments",
            "subtasks",
            "tasks",
            "board_columns",
            "project_members",
            "projects",
        ] {
            manager
                .get_connection()
                .execute_unprepared(&format!("DROP TABLE IF EXISTS {table}"))
                .await?;
        }
        Ok(())
    }
}

/// 迁移入口。
pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(Migration)]
    }
}

/// projects 表标识符。
#[derive(DeriveIden)]
pub enum Projects {
    /// 表。
    Table,
    /// id。
    Id,
    /// name。
    Name,
    /// description。
    Description,
    /// owner_id。
    OwnerId,
    /// archived_at。
    ArchivedAt,
    /// created_at。
    CreatedAt,
}

/// project_members 表标识符。
#[derive(DeriveIden)]
pub enum ProjectMembers {
    /// 表。
    Table,
    /// project_id。
    ProjectId,
    /// user_id。
    UserId,
    /// role。
    Role,
    /// joined_at。
    JoinedAt,
}

/// board_columns 表标识符。
#[derive(DeriveIden)]
pub enum BoardColumns {
    /// 表。
    Table,
    /// id。
    Id,
    /// project_id。
    ProjectId,
    /// name。
    Name,
    /// position。
    Position,
    /// is_done。
    IsDone,
    /// created_at。
    CreatedAt,
}

/// tasks 表标识符。
#[derive(DeriveIden)]
pub enum Tasks {
    /// 表。
    Table,
    /// id。
    Id,
    /// project_id。
    ProjectId,
    /// column_id。
    ColumnId,
    /// title。
    Title,
    /// description_md。
    DescriptionMd,
    /// assignee_id。
    AssigneeId,
    /// priority。
    Priority,
    /// due_at。
    DueAt,
    /// position。
    Position,
    /// created_by。
    CreatedBy,
    /// completed_at。
    CompletedAt,
    /// created_at。
    CreatedAt,
    /// updated_at。
    UpdatedAt,
}

/// subtasks 表标识符。
#[derive(DeriveIden)]
pub enum Subtasks {
    /// 表。
    Table,
    /// id。
    Id,
    /// task_id。
    TaskId,
    /// title。
    Title,
    /// done。
    Done,
    /// position。
    Position,
    /// created_at。
    CreatedAt,
}

/// task_comments 表标识符。
#[derive(DeriveIden)]
pub enum TaskComments {
    /// 表。
    Table,
    /// id。
    Id,
    /// task_id。
    TaskId,
    /// author_id。
    AuthorId,
    /// content_md。
    ContentMd,
    /// created_at。
    CreatedAt,
}
