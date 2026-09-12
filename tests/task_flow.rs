//! task 集成测试：项目/列/任务/移动/子任务/评论/权限。

mod common;

use axum::http::StatusCode;
use common::*;
use serde_json::json;
use uuid::Uuid;

async fn create_project(app: &TestApp, token: &str) -> String {
    let response = request(
        &app.app,
        "POST",
        "/api/v1/task/projects",
        Some(token),
        Some(&json!({ "name": "迎新活动", "description": "物料与场地" })),
    )
    .await;
    response.expect(StatusCode::CREATED)["id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn project_column_task_flow() {
    let app = spawn().await;
    let owner = Uuid::now_v7();
    let worker = Uuid::now_v7();
    let token = issue_token(&app, owner);
    let worker_token = issue_token(&app, worker);
    let project_id = create_project(&app, &token).await;

    // 默认三列
    let columns = request(
        &app.app,
        "GET",
        &format!("/api/v1/task/projects/{project_id}/columns"),
        Some(&token),
        None,
    )
    .await;
    let columns = columns.expect(StatusCode::OK);
    assert_eq!(columns.as_array().unwrap().len(), 3);
    let todo_id = columns[0]["id"].as_str().unwrap().to_string();
    let done_id = columns[2]["id"].as_str().unwrap().to_string();
    assert_eq!(columns[2]["isDone"], true);

    // 拉入成员并创建任务（指派给 worker）
    request(
        &app.app,
        "POST",
        &format!("/api/v1/task/projects/{project_id}/members"),
        Some(&token),
        Some(&json!({ "userIds": [worker] })),
    )
    .await
    .expect(StatusCode::OK);
    let created = request(
        &app.app,
        "POST",
        &format!("/api/v1/task/projects/{project_id}/tasks"),
        Some(&worker_token),
        Some(&json!({
            "columnId": todo_id,
            "title": "采购横幅",
            "descriptionMd": "尺寸 3x1",
            "assigneeId": worker.to_string(),
            "priority": "high"
        })),
    )
    .await;
    let created = created.expect(StatusCode::CREATED);
    let task_id = created["id"].as_str().unwrap().to_string();
    assert_eq!(created["priority"], "high");

    // 更新
    let updated = request(
        &app.app,
        "PATCH",
        &format!("/api/v1/task/tasks/{task_id}"),
        Some(&token),
        Some(&json!({ "title": "采购主视觉横幅", "priority": "urgent" })),
    )
    .await;
    assert_eq!(updated.expect(StatusCode::OK)["title"], "采购主视觉横幅");

    // 子任务
    let subtask = request(
        &app.app,
        "POST",
        &format!("/api/v1/task/tasks/{task_id}/subtasks"),
        Some(&token),
        Some(&json!({ "title": "联系供应商" })),
    )
    .await;
    let subtask = subtask.expect(StatusCode::CREATED);
    let subtask_id = subtask["id"].as_str().unwrap().to_string();
    let toggled = request(
        &app.app,
        "PATCH",
        &format!("/api/v1/task/subtasks/{subtask_id}"),
        Some(&token),
        Some(&json!({ "done": true })),
    )
    .await;
    assert_eq!(toggled.expect(StatusCode::OK)["done"], true);

    // 评论
    let comment = request(
        &app.app,
        "POST",
        &format!("/api/v1/task/tasks/{task_id}/comments"),
        Some(&token),
        Some(&json!({ "contentMd": "已联系，明天到货" })),
    )
    .await;
    comment.expect(StatusCode::CREATED);
    let comments = request(
        &app.app,
        "GET",
        &format!("/api/v1/task/tasks/{task_id}/comments"),
        Some(&worker_token),
        None,
    )
    .await;
    assert_eq!(comments.expect(StatusCode::OK).as_array().unwrap().len(), 1);

    // 移动到完成列 → completedAt 记录
    let moved = request(
        &app.app,
        "POST",
        &format!("/api/v1/task/tasks/{task_id}/move"),
        Some(&worker_token),
        Some(&json!({ "columnId": done_id })),
    )
    .await;
    let moved = moved.expect(StatusCode::OK);
    assert!(!moved["completedAt"].is_null());

    // 我的任务（仅未完成 → 空）
    let my = request(
        &app.app,
        "GET",
        "/api/v1/task/my/tasks",
        Some(&worker_token),
        None,
    )
    .await;
    assert_eq!(my.expect(StatusCode::OK).as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn permissions_and_validation() {
    let app = spawn().await;
    let owner = Uuid::now_v7();
    let outsider = Uuid::now_v7();
    let token = issue_token(&app, owner);
    let outsider_token = issue_token(&app, outsider);
    let project_id = create_project(&app, &token).await;

    // 非成员访问 → 403
    let denied = request(
        &app.app,
        "GET",
        &format!("/api/v1/task/projects/{project_id}/tasks"),
        Some(&outsider_token),
        None,
    )
    .await;
    denied.expect(StatusCode::FORBIDDEN);

    // 列列表取第一列
    let columns = request(
        &app.app,
        "GET",
        &format!("/api/v1/task/projects/{project_id}/columns"),
        Some(&token),
        None,
    )
    .await;
    let column_id = columns.expect(StatusCode::OK)[0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // 空标题 → 422
    let bad_title = request(
        &app.app,
        "POST",
        &format!("/api/v1/task/projects/{project_id}/tasks"),
        Some(&token),
        Some(&json!({ "columnId": column_id, "title": "   " })),
    )
    .await;
    bad_title.expect(StatusCode::UNPROCESSABLE_ENTITY);

    // 非法优先级 → 422
    let bad_priority = request(
        &app.app,
        "POST",
        &format!("/api/v1/task/projects/{project_id}/tasks"),
        Some(&token),
        Some(&json!({ "columnId": column_id, "title": "x", "priority": "super" })),
    )
    .await;
    bad_priority.expect(StatusCode::UNPROCESSABLE_ENTITY);

    // 不存在的任务 → 404
    let missing = request(
        &app.app,
        "GET",
        &format!("/api/v1/task/tasks/{}", Uuid::now_v7()),
        Some(&token),
        None,
    )
    .await;
    missing.expect(StatusCode::NOT_FOUND);

    // 未登录 → 401；健康检查
    request(&app.app, "GET", "/api/v1/task/projects", None, None)
        .await
        .expect(StatusCode::UNAUTHORIZED);
    let health = request(&app.app, "GET", "/healthz", None, None).await;
    assert_eq!(health.expect(StatusCode::OK)["status"], "ok");
    let ready = request(&app.app, "GET", "/readyz", None, None).await;
    assert_eq!(ready.expect(StatusCode::OK)["database"], "ok");
}

#[tokio::test]
async fn column_admin_and_cross_project_move() {
    let app = spawn().await;
    let owner = Uuid::now_v7();
    let member = Uuid::now_v7();
    let token = issue_token(&app, owner);
    let member_token = issue_token(&app, member);
    let project_id = create_project(&app, &token).await;

    // 普通成员建列 → 403
    let denied = request(
        &app.app,
        "POST",
        &format!("/api/v1/task/projects/{project_id}/columns"),
        Some(&member_token),
        Some(&json!({ "name": "评审中" })),
    )
    .await;
    denied.expect(StatusCode::FORBIDDEN);

    // 管理员建列 → 201
    let created = request(
        &app.app,
        "POST",
        &format!("/api/v1/task/projects/{project_id}/columns"),
        Some(&token),
        Some(&json!({ "name": "评审中" })),
    )
    .await;
    created.expect(StatusCode::CREATED);

    // 跨项目移动 → 404
    let other_project = create_project(&app, &token).await;
    let other_columns = request(
        &app.app,
        "GET",
        &format!("/api/v1/task/projects/{other_project}/columns"),
        Some(&token),
        None,
    )
    .await;
    let other_column = other_columns.expect(StatusCode::OK)[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let columns = request(
        &app.app,
        "GET",
        &format!("/api/v1/task/projects/{project_id}/columns"),
        Some(&token),
        None,
    )
    .await;
    let column_id = columns.expect(StatusCode::OK)[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let task = request(
        &app.app,
        "POST",
        &format!("/api/v1/task/projects/{project_id}/tasks"),
        Some(&token),
        Some(&json!({ "columnId": column_id, "title": "跨项目移动测试" })),
    )
    .await;
    let task_id = task.expect(StatusCode::CREATED)["id"]
        .as_str()
        .unwrap()
        .to_string();
    let move_to_other = request(
        &app.app,
        "POST",
        &format!("/api/v1/task/tasks/{task_id}/move"),
        Some(&token),
        Some(&json!({ "columnId": other_column })),
    )
    .await;
    move_to_other.expect(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn schema_validation_paths() {
    let err = task_service::db::connect_with_schema(&test_database_url(), "Bad-Name").await;
    assert!(err.is_err());
    assert!(task_service::config::Config::from_map(Default::default()).is_err());
    std::env::set_var("DATABASE_URL", test_database_url());
    let config = task_service::config::Config::from_env().expect("env config");
    assert_eq!(config.bind_addr, "0.0.0.0:8083");
    std::env::remove_var("DATABASE_URL");
}

#[tokio::test]
async fn due_filters_assignment_and_error_branches() {
    let app = spawn().await;
    let owner = Uuid::now_v7();
    let token = issue_token(&app, owner);
    let project_id = create_project(&app, &token).await;
    let columns = request(
        &app.app,
        "GET",
        &format!("/api/v1/task/projects/{project_id}/columns"),
        Some(&token),
        None,
    )
    .await;
    let column_id = columns.expect(StatusCode::OK)[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let now = chrono::Utc::now();

    // 三个到期任务：昨天 / 今天+1h / 5 天后
    for (title, due) in [
        ("逾期任务", now - chrono::Duration::days(1)),
        ("今日任务", now + chrono::Duration::hours(1)),
        ("本周任务", now + chrono::Duration::days(5)),
    ] {
        request(
            &app.app,
            "POST",
            &format!("/api/v1/task/projects/{project_id}/tasks"),
            Some(&token),
            Some(&json!({
                "columnId": column_id,
                "title": title,
                "assigneeId": owner.to_string(),
                "dueAt": due.to_rfc3339()
            })),
        )
        .await
        .expect(StatusCode::CREATED);
    }

    let my_all = request(&app.app, "GET", "/api/v1/task/my/tasks", Some(&token), None).await;
    assert_eq!(my_all.expect(StatusCode::OK).as_array().unwrap().len(), 3);
    let overdue = request(
        &app.app,
        "GET",
        "/api/v1/task/my/tasks?due=overdue",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(overdue.expect(StatusCode::OK).as_array().unwrap().len(), 1);
    let today = request(
        &app.app,
        "GET",
        "/api/v1/task/my/tasks?due=today",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(today.expect(StatusCode::OK).as_array().unwrap().len(), 2);
    let week = request(
        &app.app,
        "GET",
        "/api/v1/task/my/tasks?due=week",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(week.expect(StatusCode::OK).as_array().unwrap().len(), 3);

    // 按列与负责人过滤
    let by_column = request(
        &app.app,
        "GET",
        &format!(
            "/api/v1/task/projects/{project_id}/tasks?columnId={column_id}&assigneeId={owner}"
        ),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(
        by_column.expect(StatusCode::OK).as_array().unwrap().len(),
        3
    );

    // 更新任务：清空负责人 + 修改描述/截止时间
    let tasks = request(
        &app.app,
        "GET",
        &format!("/api/v1/task/projects/{project_id}/tasks"),
        Some(&token),
        None,
    )
    .await;
    let task_id = tasks.expect(StatusCode::OK)[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let cleared = request(
        &app.app,
        "PATCH",
        &format!("/api/v1/task/tasks/{task_id}"),
        Some(&token),
        Some(&json!({ "assigneeId": null, "descriptionMd": "补充说明", "dueAt": null })),
    )
    .await;
    let cleared = cleared.expect(StatusCode::OK);
    assert!(cleared["assigneeId"].is_null());
    assert_eq!(cleared["descriptionMd"], "补充说明");

    // 错误分支
    let bad_column = request(
        &app.app,
        "POST",
        &format!("/api/v1/task/projects/{project_id}/tasks"),
        Some(&token),
        Some(&json!({ "columnId": Uuid::now_v7(), "title": "x" })),
    )
    .await;
    bad_column.expect(StatusCode::NOT_FOUND);

    let empty_subtask = request(
        &app.app,
        "POST",
        &format!("/api/v1/task/tasks/{task_id}/subtasks"),
        Some(&token),
        Some(&json!({ "title": "" })),
    )
    .await;
    empty_subtask.expect(StatusCode::UNPROCESSABLE_ENTITY);

    let long_comment = request(
        &app.app,
        "POST",
        &format!("/api/v1/task/tasks/{task_id}/comments"),
        Some(&token),
        Some(&json!({ "contentMd": "x".repeat(5000) })),
    )
    .await;
    long_comment.expect(StatusCode::UNPROCESSABLE_ENTITY);

    let missing_subtask = request(
        &app.app,
        "PATCH",
        &format!("/api/v1/task/subtasks/{}", Uuid::now_v7()),
        Some(&token),
        Some(&json!({ "done": true })),
    )
    .await;
    missing_subtask.expect(StatusCode::NOT_FOUND);
}
