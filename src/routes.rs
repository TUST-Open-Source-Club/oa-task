//! HTTP 路由。

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use chrono::{DateTime, Duration, FixedOffset, Utc};
use sea_orm::EntityTrait;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use club_auth_sdk::AuthUser;
use club_common::{AppError, FieldError};

use crate::entity::{column, comment, subtask, task};
use crate::repo;
use crate::state::SharedState;

/// 存活检查。
pub async fn healthz() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

/// 就绪检查。
pub async fn readyz(State(state): State<SharedState>) -> Json<Value> {
    match state.db.ping().await {
        Ok(_) => Json(json!({ "status": "ready", "database": "ok" })),
        Err(err) => {
            tracing::error!(error = %err, "数据库就绪检查失败");
            Json(json!({ "status": "degraded", "database": "error" }))
        }
    }
}

/// 用户 ID。
fn user_id_of(auth: &AuthUser) -> Result<Uuid, AppError> {
    auth.claims()
        .sub
        .parse()
        .map_err(|_| AppError::unauthorized("AUTH_INVALID_TOKEN", "访问令牌无效"))
}

/// 写入 outbox 事件（未配置总线时跳过）。
#[allow(clippy::too_many_arguments)]
async fn enqueue_event(
    state: &SharedState,
    event_type: &str,
    actor: Uuid,
    targets: Vec<Uuid>,
    task_id: Uuid,
    title: &str,
    body: &str,
    priority: &str,
) {
    if state.bus.is_none() || targets.is_empty() {
        return;
    }
    let payload = json!({
        "id": Uuid::now_v7(),
        "type": event_type,
        "actorId": actor,
        "targetUsers": targets.iter().map(|id| id.to_string()).collect::<Vec<_>>(),
        "resource": { "type": "task", "id": task_id, "url": format!("/tasks/{task_id}") },
        "title": title,
        "body": body,
        "priority": priority
    });
    if let Err(err) = club_bus::outbox::enqueue(&state.db, event_type, &payload, Utc::now()).await {
        tracing::warn!(error = %err, "任务事件写入 outbox 失败");
    }
}

/// 看板列 DTO。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnDto {
    /// ID。
    pub id: String,
    /// 名称。
    pub name: String,
    /// 排序。
    pub position: i64,
    /// 是否完成列。
    pub is_done: bool,
}

impl From<&column::Model> for ColumnDto {
    fn from(model: &column::Model) -> Self {
        Self {
            id: model.id.to_string(),
            name: model.name.clone(),
            position: model.position,
            is_done: model.is_done,
        }
    }
}

/// 任务 DTO。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskDto {
    /// ID。
    pub id: String,
    /// 项目。
    pub project_id: String,
    /// 列。
    pub column_id: String,
    /// 标题。
    pub title: String,
    /// 描述。
    pub description_md: String,
    /// 负责人（多负责人中的首个，兼容字段）。
    pub assignee_id: Option<String>,
    /// 负责人列表。
    pub assignee_ids: Vec<String>,
    /// 开始时间。
    pub start_at: Option<DateTime<FixedOffset>>,
    /// 状态：active/done/terminated。
    pub status: String,
    /// 优先级。
    pub priority: String,
    /// 截止时间。
    pub due_at: Option<DateTime<FixedOffset>>,
    /// 排序。
    pub position: i64,
    /// 完成时间。
    pub completed_at: Option<DateTime<FixedOffset>>,
    /// 更新时间。
    pub updated_at: DateTime<FixedOffset>,
}

impl From<&task::Model> for TaskDto {
    fn from(model: &task::Model) -> Self {
        Self {
            id: model.id.to_string(),
            project_id: model.project_id.to_string(),
            column_id: model.column_id.to_string(),
            title: model.title.clone(),
            description_md: model.description_md.clone(),
            assignee_id: model.assignee_id.map(|id| id.to_string()),
            assignee_ids: model.assignee_id.map(|id| id.to_string()).into_iter().collect(),
            start_at: model.start_at,
            status: model.status.clone(),
            priority: model.priority.clone(),
            due_at: model.due_at,
            position: model.position,
            completed_at: model.completed_at,
            updated_at: model.updated_at,
        }
    }
}

/// 创建项目请求。
#[derive(Debug, Deserialize)]
pub struct CreateProjectRequest {
    /// 名称。
    pub name: String,
    /// 描述。
    pub description: Option<String>,
}

/// `POST /projects`。
pub async fn create_project(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(input): Json<CreateProjectRequest>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let user_id = user_id_of(&auth)?;
    let name = input.name.trim();
    if name.is_empty() || name.chars().count() > 64 {
        return Err(AppError::unprocessable(
            "TASK_VALIDATION",
            "项目名称需为 1 ~ 64 字符",
            vec![FieldError::new("name", "非法")],
        ));
    }
    let model = repo::create_project(
        &state.db,
        user_id,
        name,
        input.description.as_deref().unwrap_or(""),
        state.now(),
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "id": model.id, "name": model.name })),
    ))
}

/// `GET /projects`。
pub async fn list_projects(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let user_id = user_id_of(&auth)?;
    let projects = repo::list_projects(&state.db, user_id).await?;
    Ok(Json(json!(projects
        .iter()
        .map(|project| json!({ "id": project.id, "name": project.name, "description": project.description }))
        .collect::<Vec<_>>())))
}

/// 添加成员请求。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddMembersRequest {
    /// 用户列表。
    pub user_ids: Vec<Uuid>,
}

/// `POST /projects/{id}/members`（管理员）。
pub async fn add_members(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(project_id): Path<Uuid>,
    Json(input): Json<AddMembersRequest>,
) -> Result<Json<Value>, AppError> {
    let user_id = user_id_of(&auth)?;
    repo::ensure_admin(&state.db, project_id, user_id).await?;
    let mut added = 0;
    for member_id in &input.user_ids {
        repo::add_member(
            &state.db,
            project_id,
            *member_id,
            crate::entity::project_member::ROLE_MEMBER,
            state.now(),
        )
        .await?;
        added += 1;
    }
    Ok(Json(json!({ "added": added })))
}

/// `GET /projects/{id}/columns`。
pub async fn list_columns(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Vec<ColumnDto>>, AppError> {
    let user_id = user_id_of(&auth)?;
    repo::ensure_member(&state.db, project_id, user_id).await?;
    let columns = repo::list_columns(&state.db, project_id).await?;
    Ok(Json(columns.iter().map(ColumnDto::from).collect()))
}

/// 创建列请求。
#[derive(Debug, Deserialize)]
pub struct CreateColumnRequest {
    /// 名称。
    pub name: String,
}

/// `POST /projects/{id}/columns`。
pub async fn create_column(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(project_id): Path<Uuid>,
    Json(input): Json<CreateColumnRequest>,
) -> Result<(StatusCode, Json<ColumnDto>), AppError> {
    let user_id = user_id_of(&auth)?;
    repo::ensure_admin(&state.db, project_id, user_id).await?;
    let name = input.name.trim();
    if name.is_empty() || name.chars().count() > 64 {
        return Err(AppError::unprocessable(
            "TASK_VALIDATION",
            "列名称不合法",
            vec![FieldError::new("name", "非法")],
        ));
    }
    let model = repo::create_column(&state.db, project_id, name, state.now()).await?;
    Ok((StatusCode::CREATED, Json(ColumnDto::from(&model))))
}

/// 任务查询。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskQuery {
    /// 按列过滤。
    pub column_id: Option<Uuid>,
    /// 按负责人过滤。
    pub assignee_id: Option<Uuid>,
}

/// `GET /projects/{id}/tasks`。
pub async fn list_tasks(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(project_id): Path<Uuid>,
    Query(query): Query<TaskQuery>,
) -> Result<Json<Vec<TaskDto>>, AppError> {
    let user_id = user_id_of(&auth)?;
    repo::ensure_member(&state.db, project_id, user_id).await?;
    let tasks = repo::list_tasks(&state.db, project_id, query.column_id, query.assignee_id).await?;
    let mut items = Vec::with_capacity(tasks.len());
    for model in &tasks {
        items.push(task_dto(&state, model).await?);
    }
    Ok(Json(items))
}

/// 加载负责人列表并构造 DTO。
async fn task_dto(state: &SharedState, model: &task::Model) -> Result<TaskDto, AppError> {
    let mut dto = TaskDto::from(model);
    let assignees = repo::list_task_assignees(&state.db, model.id).await?;
    if !assignees.is_empty() {
        dto.assignee_ids = assignees.iter().map(|id| id.to_string()).collect();
        dto.assignee_id = assignees.first().map(|id| id.to_string());
    }
    Ok(dto)
}

/// 创建任务请求。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskRequest {
    /// 列。
    pub column_id: Uuid,
    /// 标题。
    pub title: String,
    /// 描述。
    pub description_md: Option<String>,
    /// 负责人（单人选，兼容）。
    pub assignee_id: Option<Uuid>,
    /// 负责人列表（多人）。
    pub assignee_ids: Option<Vec<Uuid>>,
    /// 优先级。
    pub priority: Option<String>,
    /// 截止时间（RFC3339）。
    pub due_at: Option<DateTime<Utc>>,
    /// 开始时间（RFC3339）。
    pub start_at: Option<DateTime<Utc>>,
}

/// `POST /projects/{id}/tasks`。
pub async fn create_task(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(project_id): Path<Uuid>,
    Json(input): Json<CreateTaskRequest>,
) -> Result<(StatusCode, Json<TaskDto>), AppError> {
    let user_id = user_id_of(&auth)?;
    repo::ensure_member(&state.db, project_id, user_id).await?;
    let title = input.title.trim();
    if title.is_empty() || title.chars().count() > 255 {
        return Err(AppError::unprocessable(
            "TASK_VALIDATION",
            "标题需为 1 ~ 255 字符",
            vec![FieldError::new("title", "非法")],
        ));
    }
    let column = repo::find_column(&state.db, input.column_id)
        .await?
        .filter(|column| column.project_id == project_id)
        .ok_or_else(|| AppError::not_found("TASK_COLUMN_NOT_FOUND", "看板列不存在"))?;
    let priority = input.priority.unwrap_or_else(|| "normal".to_string());
    if !["low", "normal", "high", "urgent"].contains(&priority.as_str()) {
        return Err(AppError::unprocessable(
            "TASK_VALIDATION",
            "优先级不合法",
            vec![FieldError::new("priority", "仅支持 low/normal/high/urgent")],
        ));
    }
    let mut assignees = input.assignee_ids.unwrap_or_default();
    if assignees.is_empty() {
        assignees.extend(input.assignee_id);
    }
    assignees.dedup();
    let model = repo::create_task(
        &state.db,
        project_id,
        column.id,
        title,
        input.description_md.as_deref().unwrap_or(""),
        assignees.first().copied(),
        &priority,
        input.due_at,
        input.start_at,
        user_id,
        state.now(),
    )
    .await?;
    if !assignees.is_empty() {
        repo::set_task_assignees(&state.db, model.id, &assignees, state.now()).await?;
    }
    for assignee in &assignees {
        if *assignee != user_id {
            enqueue_event(
                &state,
                "task.assigned",
                user_id,
                vec![*assignee],
                model.id,
                "有新任务指派给你",
                &model.title,
                "high",
            )
            .await;
        }
    }
    Ok((StatusCode::CREATED, Json(task_dto(&state, &model).await?)))
}

/// 加载任务并校验项目成员。
async fn load_task_for_member(
    state: &SharedState,
    user_id: Uuid,
    task_id: Uuid,
) -> Result<task::Model, AppError> {
    let model = repo::find_task(&state.db, task_id)
        .await?
        .ok_or_else(|| AppError::not_found("TASK_NOT_FOUND", "任务不存在"))?;
    repo::ensure_member(&state.db, model.project_id, user_id).await?;
    Ok(model)
}

/// `GET /tasks/{id}`。
pub async fn get_task(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(task_id): Path<Uuid>,
) -> Result<Json<TaskDto>, AppError> {
    let user_id = user_id_of(&auth)?;
    let model = load_task_for_member(&state, user_id, task_id).await?;
    Ok(Json(TaskDto::from(&model)))
}

/// 三态 JSON：字段缺省 = None，显式 null = Some(None)，有值 = Some(Some(v))。
fn double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// 更新任务请求。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTaskRequest {
    /// 标题。
    pub title: Option<String>,
    /// 描述。
    pub description_md: Option<String>,
    /// 负责人（null 表示清空）。
    #[serde(default, deserialize_with = "double_option")]
    pub assignee_id: Option<Option<Uuid>>,
    /// 负责人列表（覆盖式；空数组表示清空）。
    pub assignee_ids: Option<Vec<Uuid>>,
    /// 优先级。
    pub priority: Option<String>,
    /// 截止时间（null 表示清空）。
    #[serde(default, deserialize_with = "double_option")]
    pub due_at: Option<Option<DateTime<Utc>>>,
    /// 开始时间（null 表示清空）。
    #[serde(default, deserialize_with = "double_option")]
    pub start_at: Option<Option<DateTime<Utc>>>,
}

/// `PATCH /tasks/{id}`。
pub async fn update_task(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(task_id): Path<Uuid>,
    Json(input): Json<UpdateTaskRequest>,
) -> Result<Json<TaskDto>, AppError> {
    let user_id = user_id_of(&auth)?;
    let model = load_task_for_member(&state, user_id, task_id).await?;
    if let Some(priority) = input.priority.as_deref() {
        if !["low", "normal", "high", "urgent"].contains(&priority) {
            return Err(AppError::unprocessable(
                "TASK_VALIDATION",
                "优先级不合法",
                vec![FieldError::new("priority", "非法")],
            ));
        }
    }
    let old_assignees = repo::list_task_assignees(&state.db, model.id).await?;
    let legacy_old: Vec<Uuid> = if old_assignees.is_empty() {
        model.assignee_id.into_iter().collect()
    } else {
        old_assignees
    };
    let mut replaced: Option<Vec<Uuid>> = None;
    let assignee_field = if let Some(mut list) = input.assignee_ids {
        list.dedup();
        let first = list.first().copied();
        replaced = Some(list);
        Some(first)
    } else {
        input.assignee_id
    };
    let updated = repo::update_task(
        &state.db,
        &model,
        input.title,
        input.description_md,
        assignee_field,
        input.priority,
        input.due_at,
        input.start_at,
        state.now(),
    )
    .await?;
    if let Some(list) = replaced {
        repo::set_task_assignees(&state.db, updated.id, &list, state.now()).await?;
        for assignee in &list {
            if *assignee != user_id && !legacy_old.contains(assignee) {
                enqueue_event(
                    &state,
                    "task.assigned",
                    user_id,
                    vec![*assignee],
                    updated.id,
                    "有新任务指派给你",
                    &updated.title,
                    "high",
                )
                .await;
            }
        }
    } else if let Some(Some(assignee)) = input.assignee_id {
        repo::set_task_assignees(&state.db, updated.id, &[assignee], state.now()).await?;
        if assignee != user_id && !legacy_old.contains(&assignee) {
            enqueue_event(
                &state,
                "task.assigned",
                user_id,
                vec![assignee],
                updated.id,
                "有新任务指派给你",
                &updated.title,
                "high",
            )
            .await;
        }
    } else if matches!(input.assignee_id, Some(None)) {
        repo::set_task_assignees(&state.db, updated.id, &[], state.now()).await?;
    }
    Ok(Json(task_dto(&state, &updated).await?))
}

/// 更新任务状态请求。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTaskStatusRequest {
    /// active / done / terminated。
    pub status: String,
}

/// `POST /tasks/{id}/status`。
pub async fn update_task_status(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(task_id): Path<Uuid>,
    Json(input): Json<UpdateTaskStatusRequest>,
) -> Result<Json<TaskDto>, AppError> {
    let user_id = user_id_of(&auth)?;
    if !["active", "done", "terminated"].contains(&input.status.as_str()) {
        return Err(AppError::unprocessable(
            "TASK_VALIDATION",
            "状态不合法",
            vec![FieldError::new("status", "仅支持 active/done/terminated")],
        ));
    }
    let model = load_task_for_member(&state, user_id, task_id).await?;
    let updated = repo::update_task_status(&state.db, &model, &input.status, state.now()).await?;
    if matches!(input.status.as_str(), "done" | "terminated") {
        let mut targets = repo::list_task_assignees(&state.db, updated.id).await?;
        if targets.is_empty() {
            targets.extend(updated.assignee_id);
        }
        targets.retain(|id| *id != user_id);
        if !targets.is_empty() {
            let event_type = if input.status == "done" {
                "task.completed"
            } else {
                "task.terminated"
            };
            enqueue_event(
                &state,
                event_type,
                user_id,
                targets,
                updated.id,
                "任务状态更新",
                &updated.title,
                "normal",
            )
            .await;
        }
    }
    Ok(Json(task_dto(&state, &updated).await?))
}

/// `POST /tasks/{id}/claim`：认领任务（把自己加入负责人）。
pub async fn claim_task(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(task_id): Path<Uuid>,
) -> Result<Json<TaskDto>, AppError> {
    let user_id = user_id_of(&auth)?;
    let model = load_task_for_member(&state, user_id, task_id).await?;
    let mut assignees = repo::list_task_assignees(&state.db, model.id).await?;
    if assignees.is_empty() {
        assignees.extend(model.assignee_id);
    }
    if !assignees.contains(&user_id) {
        assignees.push(user_id);
    }
    repo::set_task_assignees(&state.db, model.id, &assignees, state.now()).await?;
    let first = assignees.first().copied();
    let updated = repo::update_task(
        &state.db,
        &model,
        None,
        None,
        Some(first),
        None,
        None,
        None,
        state.now(),
    )
    .await?;
    Ok(Json(task_dto(&state, &updated).await?))
}

/// `DELETE /tasks/{id}`。
pub async fn delete_task(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(task_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let user_id = user_id_of(&auth)?;
    let model = load_task_for_member(&state, user_id, task_id).await?;
    repo::delete_task(&state.db, model.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 移动任务请求。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveTaskRequest {
    /// 目标列。
    pub column_id: Uuid,
}

/// `POST /tasks/{id}/move`。
pub async fn move_task(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(task_id): Path<Uuid>,
    Json(input): Json<MoveTaskRequest>,
) -> Result<Json<TaskDto>, AppError> {
    let user_id = user_id_of(&auth)?;
    let model = load_task_for_member(&state, user_id, task_id).await?;
    let target = repo::find_column(&state.db, input.column_id)
        .await?
        .filter(|column| column.project_id == model.project_id)
        .ok_or_else(|| AppError::not_found("TASK_COLUMN_NOT_FOUND", "看板列不存在"))?;
    let moved = repo::move_task(&state.db, &model, &target, state.now()).await?;
    Ok(Json(TaskDto::from(&moved)))
}

/// 子任务 DTO。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtaskDto {
    /// ID。
    pub id: String,
    /// 标题。
    pub title: String,
    /// 是否完成。
    pub done: bool,
}

impl From<&subtask::Model> for SubtaskDto {
    fn from(model: &subtask::Model) -> Self {
        Self {
            id: model.id.to_string(),
            title: model.title.clone(),
            done: model.done,
        }
    }
}

/// 创建子任务请求。
#[derive(Debug, Deserialize)]
pub struct CreateSubtaskRequest {
    /// 标题。
    pub title: String,
}

/// `GET /tasks/{id}/subtasks`。
pub async fn list_subtasks(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(task_id): Path<Uuid>,
) -> Result<Json<Vec<SubtaskDto>>, AppError> {
    let user_id = user_id_of(&auth)?;
    load_task_for_member(&state, user_id, task_id).await?;
    let items = repo::list_subtasks(&state.db, task_id).await?;
    Ok(Json(items.iter().map(SubtaskDto::from).collect()))
}

/// `POST /tasks/{id}/subtasks`。
pub async fn create_subtask(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(task_id): Path<Uuid>,
    Json(input): Json<CreateSubtaskRequest>,
) -> Result<(StatusCode, Json<SubtaskDto>), AppError> {
    let user_id = user_id_of(&auth)?;
    load_task_for_member(&state, user_id, task_id).await?;
    let title = input.title.trim();
    if title.is_empty() {
        return Err(AppError::unprocessable(
            "TASK_VALIDATION",
            "子任务标题不能为空",
            vec![FieldError::new("title", "不能为空")],
        ));
    }
    let model = repo::create_subtask(&state.db, task_id, title, state.now()).await?;
    Ok((StatusCode::CREATED, Json(SubtaskDto::from(&model))))
}

/// 切换子任务请求。
#[derive(Debug, Deserialize)]
pub struct ToggleSubtaskRequest {
    /// 是否完成。
    pub done: bool,
}

/// `PATCH /subtasks/{id}`。
pub async fn toggle_subtask(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(subtask_id): Path<Uuid>,
    Json(input): Json<ToggleSubtaskRequest>,
) -> Result<Json<SubtaskDto>, AppError> {
    let user_id = user_id_of(&auth)?;
    let model = subtask::Entity::find_by_id(subtask_id)
        .one(&state.db)
        .await
        .map_err(repo::map_db_err)?
        .ok_or_else(|| AppError::not_found("TASK_SUBTASK_NOT_FOUND", "子任务不存在"))?;
    load_task_for_member(&state, user_id, model.task_id).await?;
    let updated = repo::set_subtask_done(&state.db, &model, input.done).await?;
    Ok(Json(SubtaskDto::from(&updated)))
}

/// 评论 DTO。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentDto {
    /// ID。
    pub id: String,
    /// 作者。
    pub author_id: String,
    /// 内容。
    pub content_md: String,
    /// 时间。
    pub created_at: DateTime<FixedOffset>,
}

impl From<&comment::Model> for CommentDto {
    fn from(model: &comment::Model) -> Self {
        Self {
            id: model.id.to_string(),
            author_id: model.author_id.to_string(),
            content_md: model.content_md.clone(),
            created_at: model.created_at,
        }
    }
}

/// 创建评论请求。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCommentRequest {
    /// 内容。
    pub content_md: String,
}

/// `GET /tasks/{id}/comments`。
pub async fn list_comments(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(task_id): Path<Uuid>,
) -> Result<Json<Vec<CommentDto>>, AppError> {
    let user_id = user_id_of(&auth)?;
    load_task_for_member(&state, user_id, task_id).await?;
    let items = repo::list_comments(&state.db, task_id).await?;
    Ok(Json(items.iter().map(CommentDto::from).collect()))
}

/// `POST /tasks/{id}/comments`。
pub async fn create_comment(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(task_id): Path<Uuid>,
    Json(input): Json<CreateCommentRequest>,
) -> Result<(StatusCode, Json<CommentDto>), AppError> {
    let user_id = user_id_of(&auth)?;
    let model = load_task_for_member(&state, user_id, task_id).await?;
    let content = input.content_md.trim();
    if content.is_empty() || content.chars().count() > 4096 {
        return Err(AppError::unprocessable(
            "TASK_VALIDATION",
            "评论内容需为 1 ~ 4096 字符",
            vec![FieldError::new("contentMd", "非法")],
        ));
    }
    let comment = repo::create_comment(&state.db, task_id, user_id, content, state.now()).await?;
    if let Some(assignee) = model.assignee_id {
        if assignee != user_id {
            enqueue_event(
                &state,
                "task.comment.added",
                user_id,
                vec![assignee],
                task_id,
                "任务有新评论",
                &model.title,
                "normal",
            )
            .await;
        }
    }
    Ok((StatusCode::CREATED, Json(CommentDto::from(&comment))))
}

/// 我的任务查询。
#[derive(Debug, Deserialize)]
pub struct MyTaskQuery {
    /// overdue / today / week（空 = 全部未完成）。
    pub due: Option<String>,
}

/// `GET /my/tasks`。
pub async fn my_tasks(
    State(state): State<SharedState>,
    auth: AuthUser,
    Query(query): Query<MyTaskQuery>,
) -> Result<Json<Vec<TaskDto>>, AppError> {
    let user_id = user_id_of(&auth)?;
    let now = state.now();
    let (due_before, only_open) = match query.due.as_deref() {
        Some("overdue") => (Some(now), true),
        Some("today") => (Some(now + Duration::days(1)), true),
        Some("week") => (Some(now + Duration::days(7)), true),
        _ => (None, true),
    };
    let items = repo::my_tasks(&state.db, user_id, due_before, only_open).await?;
    let mut dtos = Vec::with_capacity(items.len());
    for model in &items {
        dtos.push(task_dto(&state, model).await?);
    }
    Ok(Json(dtos))
}

/// `/api/v1/task` 路由。
pub fn router() -> Router<SharedState> {
    Router::new()
        .route("/projects", get(list_projects).post(create_project))
        .route("/projects/{id}/members", post(add_members))
        .route(
            "/projects/{id}/columns",
            get(list_columns).post(create_column),
        )
        .route("/projects/{id}/tasks", get(list_tasks).post(create_task))
        .route(
            "/tasks/{id}",
            get(get_task).patch(update_task).delete(delete_task),
        )
        .route("/tasks/{id}/status", post(update_task_status))
        .route("/tasks/{id}/claim", post(claim_task))
        .route("/tasks/{id}/move", post(move_task))
        .route(
            "/tasks/{id}/subtasks",
            get(list_subtasks).post(create_subtask),
        )
        .route("/subtasks/{id}", patch(toggle_subtask))
        .route(
            "/tasks/{id}/comments",
            get(list_comments).post(create_comment),
        )
        .route("/my/tasks", get(my_tasks))
}
