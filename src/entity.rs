//! task schema 实体。

/// 项目。
pub mod project {
    use sea_orm::entity::prelude::*;

    /// 项目模型。
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "projects")]
    pub struct Model {
        /// ID。
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: Uuid,
        /// 名称。
        pub name: String,
        /// 描述。
        #[sea_orm(column_type = "Text")]
        pub description: String,
        /// 拥有者。
        pub owner_id: Uuid,
        /// 归档时间。
        #[sea_orm(nullable)]
        pub archived_at: Option<DateTimeWithTimeZone>,
        /// 创建时间。
        pub created_at: DateTimeWithTimeZone,
    }

    /// 关系。
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// 项目成员。
pub mod project_member {
    use sea_orm::entity::prelude::*;

    /// 管理员。
    pub const ROLE_ADMIN: &str = "admin";
    /// 成员。
    pub const ROLE_MEMBER: &str = "member";

    /// 成员模型。
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "project_members")]
    pub struct Model {
        /// 项目。
        #[sea_orm(primary_key, auto_increment = false)]
        pub project_id: Uuid,
        /// 用户。
        #[sea_orm(primary_key, auto_increment = false)]
        pub user_id: Uuid,
        /// 角色。
        pub role: String,
        /// 加入时间。
        pub joined_at: DateTimeWithTimeZone,
    }

    /// 关系。
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// 看板列。
pub mod column {
    use sea_orm::entity::prelude::*;

    /// 看板列模型。
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "board_columns")]
    pub struct Model {
        /// ID。
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: Uuid,
        /// 项目。
        pub project_id: Uuid,
        /// 名称。
        pub name: String,
        /// 排序。
        pub position: i64,
        /// 是否为完成列。
        pub is_done: bool,
        /// 创建时间。
        pub created_at: DateTimeWithTimeZone,
    }

    /// 关系。
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// 任务。
pub mod task {
    use sea_orm::entity::prelude::*;

    /// 任务模型。
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "tasks")]
    pub struct Model {
        /// ID。
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: Uuid,
        /// 项目。
        pub project_id: Uuid,
        /// 所在列。
        pub column_id: Uuid,
        /// 标题。
        pub title: String,
        /// 描述（Markdown）。
        #[sea_orm(column_type = "Text")]
        pub description_md: String,
        /// 负责人（多负责人取首个，保持兼容）。
        #[sea_orm(nullable)]
        pub assignee_id: Option<Uuid>,
        /// 开始时间。
        #[sea_orm(nullable)]
        pub start_at: Option<DateTimeWithTimeZone>,
        /// 优先级：low/normal/high/urgent。
        pub priority: String,
        /// 截止时间。
        #[sea_orm(nullable)]
        pub due_at: Option<DateTimeWithTimeZone>,
        /// 列内排序。
        pub position: i64,
        /// 创建者。
        pub created_by: Uuid,
        /// 状态：active/done/terminated。
        pub status: String,
        /// 完成时间。
        #[sea_orm(nullable)]
        pub completed_at: Option<DateTimeWithTimeZone>,
        /// 创建时间。
        pub created_at: DateTimeWithTimeZone,
        /// 更新时间。
        pub updated_at: DateTimeWithTimeZone,
    }

    /// 关系。
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// 任务负责人（多对多）。
pub mod task_assignee {
    use sea_orm::entity::prelude::*;

    /// 负责人模型。
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "task_assignees")]
    pub struct Model {
        /// 任务。
        #[sea_orm(primary_key, auto_increment = false)]
        pub task_id: Uuid,
        /// 用户。
        #[sea_orm(primary_key, auto_increment = false)]
        pub user_id: Uuid,
        /// 指派时间。
        pub assigned_at: DateTimeWithTimeZone,
    }

    /// 关系。
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// 子任务。
pub mod subtask {
    use sea_orm::entity::prelude::*;

    /// 子任务模型。
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "subtasks")]
    pub struct Model {
        /// ID。
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: Uuid,
        /// 任务。
        pub task_id: Uuid,
        /// 标题。
        pub title: String,
        /// 是否完成。
        pub done: bool,
        /// 排序。
        pub position: i64,
        /// 创建时间。
        pub created_at: DateTimeWithTimeZone,
    }

    /// 关系。
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// 任务评论。
pub mod comment {
    use sea_orm::entity::prelude::*;

    /// 评论模型。
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "task_comments")]
    pub struct Model {
        /// ID。
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: Uuid,
        /// 任务。
        pub task_id: Uuid,
        /// 作者。
        pub author_id: Uuid,
        /// 内容（Markdown）。
        #[sea_orm(column_type = "Text")]
        pub content_md: String,
        /// 创建时间。
        pub created_at: DateTimeWithTimeZone,
    }

    /// 关系。
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}
