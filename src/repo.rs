//! 数据访问层：只操作 task schema。

use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter, QueryOrder,
    Set,
};
use uuid::Uuid;

use club_common::{new_id, AppError};

use crate::entity::{column, comment, project, project_member, subtask, task};

/// 数据库错误 → 统一错误。
pub fn map_db_err(err: DbErr) -> AppError {
    AppError::internal(err)
}

/// 创建项目（owner 为管理员）。
pub async fn create_project(
    db: &DatabaseConnection,
    owner_id: Uuid,
    name: &str,
    description: &str,
    now: DateTime<Utc>,
) -> Result<project::Model, AppError> {
    let model = project::ActiveModel {
        id: Set(new_id()),
        name: Set(name.to_string()),
        description: Set(description.to_string()),
        owner_id: Set(owner_id),
        archived_at: Set(None),
        created_at: Set(now.fixed_offset()),
    }
    .insert(db)
    .await
    .map_err(map_db_err)?;
    project_member::ActiveModel {
        project_id: Set(model.id),
        user_id: Set(owner_id),
        role: Set(project_member::ROLE_ADMIN.to_string()),
        joined_at: Set(now.fixed_offset()),
    }
    .insert(db)
    .await
    .map_err(map_db_err)?;
    // 默认三列：待办 / 进行中 / 已完成
    for (name, position, is_done) in [
        ("待办", 1, false),
        ("进行中", 2, false),
        ("已完成", 3, true),
    ] {
        column::ActiveModel {
            id: Set(new_id()),
            project_id: Set(model.id),
            name: Set(name.to_string()),
            position: Set(position),
            is_done: Set(is_done),
            created_at: Set(now.fixed_offset()),
        }
        .insert(db)
        .await
        .map_err(map_db_err)?;
    }
    Ok(model)
}

/// 用户参与的项目（不含归档）。
pub async fn list_projects(
    db: &DatabaseConnection,
    user_id: Uuid,
) -> Result<Vec<project::Model>, AppError> {
    let members = project_member::Entity::find()
        .filter(project_member::Column::UserId.eq(user_id))
        .all(db)
        .await
        .map_err(map_db_err)?;
    let ids: Vec<Uuid> = members.iter().map(|m| m.project_id).collect();
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    project::Entity::find()
        .filter(project::Column::Id.is_in(ids))
        .filter(project::Column::ArchivedAt.is_null())
        .order_by_desc(project::Column::CreatedAt)
        .all(db)
        .await
        .map_err(map_db_err)
}

/// 校验项目成员。
pub async fn ensure_member(
    db: &DatabaseConnection,
    project_id: Uuid,
    user_id: Uuid,
) -> Result<project_member::Model, AppError> {
    project_member::Entity::find_by_id((project_id, user_id))
        .one(db)
        .await
        .map_err(map_db_err)?
        .ok_or_else(|| AppError::forbidden("TASK_NOT_MEMBER", "无权访问该项目"))
}

/// 校验项目管理员。
pub async fn ensure_admin(
    db: &DatabaseConnection,
    project_id: Uuid,
    user_id: Uuid,
) -> Result<project_member::Model, AppError> {
    let member = ensure_member(db, project_id, user_id).await?;
    if member.role != project_member::ROLE_ADMIN {
        return Err(AppError::forbidden("TASK_FORBIDDEN", "仅项目管理员可操作"));
    }
    Ok(member)
}

/// 添加项目成员（幂等）。
pub async fn add_member(
    db: &DatabaseConnection,
    project_id: Uuid,
    user_id: Uuid,
    role: &str,
    now: DateTime<Utc>,
) -> Result<project_member::Model, AppError> {
    if let Some(existing) = project_member::Entity::find_by_id((project_id, user_id))
        .one(db)
        .await
        .map_err(map_db_err)?
    {
        return Ok(existing);
    }
    project_member::ActiveModel {
        project_id: Set(project_id),
        user_id: Set(user_id),
        role: Set(role.to_string()),
        joined_at: Set(now.fixed_offset()),
    }
    .insert(db)
    .await
    .map_err(map_db_err)
}

/// 新建看板列。
pub async fn create_column(
    db: &DatabaseConnection,
    project_id: Uuid,
    name: &str,
    now: DateTime<Utc>,
) -> Result<column::Model, AppError> {
    let max: Option<i64> = column::Entity::find()
        .filter(column::Column::ProjectId.eq(project_id))
        .order_by_desc(column::Column::Position)
        .one(db)
        .await
        .map_err(map_db_err)?
        .map(|row| row.position);
    column::ActiveModel {
        id: Set(new_id()),
        project_id: Set(project_id),
        name: Set(name.to_string()),
        position: Set(max.unwrap_or(0) + 1),
        is_done: Set(false),
        created_at: Set(now.fixed_offset()),
    }
    .insert(db)
    .await
    .map_err(map_db_err)
}

/// 列出看板列。
pub async fn list_columns(
    db: &DatabaseConnection,
    project_id: Uuid,
) -> Result<Vec<column::Model>, AppError> {
    column::Entity::find()
        .filter(column::Column::ProjectId.eq(project_id))
        .order_by_asc(column::Column::Position)
        .all(db)
        .await
        .map_err(map_db_err)
}

/// 查找列。
pub async fn find_column(
    db: &DatabaseConnection,
    column_id: Uuid,
) -> Result<Option<column::Model>, AppError> {
    column::Entity::find_by_id(column_id)
        .one(db)
        .await
        .map_err(map_db_err)
}

/// 创建任务。
#[allow(clippy::too_many_arguments)]
pub async fn create_task(
    db: &DatabaseConnection,
    project_id: Uuid,
    column_id: Uuid,
    title: &str,
    description_md: &str,
    assignee_id: Option<Uuid>,
    priority: &str,
    due_at: Option<DateTime<Utc>>,
    user_id: Uuid,
    now: DateTime<Utc>,
) -> Result<task::Model, AppError> {
    let max: Option<i64> = task::Entity::find()
        .filter(task::Column::ColumnId.eq(column_id))
        .order_by_desc(task::Column::Position)
        .one(db)
        .await
        .map_err(map_db_err)?
        .map(|row| row.position);
    task::ActiveModel {
        id: Set(new_id()),
        project_id: Set(project_id),
        column_id: Set(column_id),
        title: Set(title.to_string()),
        description_md: Set(description_md.to_string()),
        assignee_id: Set(assignee_id),
        priority: Set(priority.to_string()),
        due_at: Set(due_at.map(|value| value.fixed_offset())),
        position: Set(max.unwrap_or(0) + 1),
        created_by: Set(user_id),
        completed_at: Set(None),
        created_at: Set(now.fixed_offset()),
        updated_at: Set(now.fixed_offset()),
    }
    .insert(db)
    .await
    .map_err(map_db_err)
}

/// 查找任务。
pub async fn find_task(
    db: &DatabaseConnection,
    task_id: Uuid,
) -> Result<Option<task::Model>, AppError> {
    task::Entity::find_by_id(task_id)
        .one(db)
        .await
        .map_err(map_db_err)
}

/// 任务列表（可按列/负责人过滤）。
pub async fn list_tasks(
    db: &DatabaseConnection,
    project_id: Uuid,
    column_id: Option<Uuid>,
    assignee_id: Option<Uuid>,
) -> Result<Vec<task::Model>, AppError> {
    let mut select = task::Entity::find().filter(task::Column::ProjectId.eq(project_id));
    if let Some(column_id) = column_id {
        select = select.filter(task::Column::ColumnId.eq(column_id));
    }
    if let Some(assignee_id) = assignee_id {
        select = select.filter(task::Column::AssigneeId.eq(assignee_id));
    }
    select
        .order_by_asc(task::Column::ColumnId)
        .order_by_asc(task::Column::Position)
        .all(db)
        .await
        .map_err(map_db_err)
}

/// 更新任务字段（None 表示不修改）。
#[allow(clippy::too_many_arguments)]
pub async fn update_task(
    db: &DatabaseConnection,
    task: &task::Model,
    title: Option<String>,
    description_md: Option<String>,
    assignee_id: Option<Option<Uuid>>,
    priority: Option<String>,
    due_at: Option<Option<DateTime<Utc>>>,
    now: DateTime<Utc>,
) -> Result<task::Model, AppError> {
    let mut active: task::ActiveModel = task.clone().into();
    if let Some(title) = title {
        active.title = Set(title);
    }
    if let Some(description_md) = description_md {
        active.description_md = Set(description_md);
    }
    if let Some(assignee_id) = assignee_id {
        active.assignee_id = Set(assignee_id);
    }
    if let Some(priority) = priority {
        active.priority = Set(priority);
    }
    if let Some(due_at) = due_at {
        active.due_at = Set(due_at.map(|value| value.fixed_offset()));
    }
    active.updated_at = Set(now.fixed_offset());
    active.update(db).await.map_err(map_db_err)
}

/// 移动任务到目标列/位置（进入完成列时记录完成时间）。
pub async fn move_task(
    db: &DatabaseConnection,
    task: &task::Model,
    target: &column::Model,
    now: DateTime<Utc>,
) -> Result<task::Model, AppError> {
    let max: Option<i64> = task::Entity::find()
        .filter(task::Column::ColumnId.eq(target.id))
        .order_by_desc(task::Column::Position)
        .one(db)
        .await
        .map_err(map_db_err)?
        .map(|row| row.position);
    let mut active: task::ActiveModel = task.clone().into();
    active.column_id = Set(target.id);
    active.position = Set(max.unwrap_or(0) + 1);
    active.completed_at = Set(if target.is_done {
        Some(now.fixed_offset())
    } else {
        None
    });
    active.updated_at = Set(now.fixed_offset());
    active.update(db).await.map_err(map_db_err)
}

/// 我的任务（跨项目）。
pub async fn my_tasks(
    db: &DatabaseConnection,
    user_id: Uuid,
    due_before: Option<DateTime<Utc>>,
    only_open: bool,
) -> Result<Vec<task::Model>, AppError> {
    let members = project_member::Entity::find()
        .filter(project_member::Column::UserId.eq(user_id))
        .all(db)
        .await
        .map_err(map_db_err)?;
    let ids: Vec<Uuid> = members.iter().map(|m| m.project_id).collect();
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut select = task::Entity::find()
        .filter(task::Column::ProjectId.is_in(ids))
        .filter(task::Column::AssigneeId.eq(user_id));
    if let Some(due_before) = due_before {
        // NULL 截止时间自然不满足 lte
        select = select.filter(task::Column::DueAt.lte(due_before.fixed_offset()));
    }
    if only_open {
        select = select.filter(task::Column::CompletedAt.is_null());
    }
    select
        .order_by_asc(task::Column::DueAt)
        .all(db)
        .await
        .map_err(map_db_err)
}

/// 创建子任务。
pub async fn create_subtask(
    db: &DatabaseConnection,
    task_id: Uuid,
    title: &str,
    now: DateTime<Utc>,
) -> Result<subtask::Model, AppError> {
    let max: Option<i64> = subtask::Entity::find()
        .filter(subtask::Column::TaskId.eq(task_id))
        .order_by_desc(subtask::Column::Position)
        .one(db)
        .await
        .map_err(map_db_err)?
        .map(|row| row.position);
    subtask::ActiveModel {
        id: Set(new_id()),
        task_id: Set(task_id),
        title: Set(title.to_string()),
        done: Set(false),
        position: Set(max.unwrap_or(0) + 1),
        created_at: Set(now.fixed_offset()),
    }
    .insert(db)
    .await
    .map_err(map_db_err)
}

/// 列出子任务。
pub async fn list_subtasks(
    db: &DatabaseConnection,
    task_id: Uuid,
) -> Result<Vec<subtask::Model>, AppError> {
    subtask::Entity::find()
        .filter(subtask::Column::TaskId.eq(task_id))
        .order_by_asc(subtask::Column::Position)
        .all(db)
        .await
        .map_err(map_db_err)
}

/// 切换子任务完成状态。
pub async fn set_subtask_done(
    db: &DatabaseConnection,
    subtask: &subtask::Model,
    done: bool,
) -> Result<subtask::Model, AppError> {
    let mut active: subtask::ActiveModel = subtask.clone().into();
    active.done = Set(done);
    active.update(db).await.map_err(map_db_err)
}

/// 创建评论。
pub async fn create_comment(
    db: &DatabaseConnection,
    task_id: Uuid,
    author_id: Uuid,
    content_md: &str,
    now: DateTime<Utc>,
) -> Result<comment::Model, AppError> {
    comment::ActiveModel {
        id: Set(new_id()),
        task_id: Set(task_id),
        author_id: Set(author_id),
        content_md: Set(content_md.to_string()),
        created_at: Set(now.fixed_offset()),
    }
    .insert(db)
    .await
    .map_err(map_db_err)
}

/// 列出评论。
pub async fn list_comments(
    db: &DatabaseConnection,
    task_id: Uuid,
) -> Result<Vec<comment::Model>, AppError> {
    comment::Entity::find()
        .filter(comment::Column::TaskId.eq(task_id))
        .order_by_asc(comment::Column::CreatedAt)
        .all(db)
        .await
        .map_err(map_db_err)
}
