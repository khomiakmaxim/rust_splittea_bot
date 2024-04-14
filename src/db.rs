use rust_decimal::Decimal;
use sea_orm::{
    ActiveModelTrait, ActiveValue::NotSet, ColumnTrait, Database as SeaOrmDatabase,
    DatabaseConnection, DbErr, EntityTrait, QueryFilter, Set,
};
use sea_orm_migration::MigratorTrait;
use std::{fs::OpenOptions, path::PathBuf};

use crate::{
    entity::{expense, group, user, user_group},
    migration::Migrator,
};

#[derive(Debug)]
pub enum Error {
    Database(DbErr),
    File(std::io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::Database(ref err) => {
                write!(f, "Database error: {}", err)
            }
            Self::File(ref err) => write!(f, "File error: {}", err),
        }
    }
}

impl From<DbErr> for Error {
    fn from(err: DbErr) -> Self {
        Self::Database(err)
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Self::File(err)
    }
}

async fn get_db_pool(db_path: &PathBuf) -> Result<DatabaseConnection, Error> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(db_path)?;
    let db_str = format!("sqlite:{}", db_path.display());
    let pool = SeaOrmDatabase::connect(&db_str).await?;
    Ok(pool)
}

#[derive(Clone)]
pub struct Database {
    pool: DatabaseConnection,
}

impl Database {
    pub async fn new(db_path: &PathBuf) -> Result<Self, Error> {
        get_db_pool(db_path).await.map(|pool| Self { pool })
    }

    pub async fn apply_migrations(&self) -> Result<(), Error> {
        Ok(Migrator::up(&self.pool, None).await?)
    }

    pub async fn get_users_in_group(&self, group_id: i64) -> Result<Vec<user::Model>, Error> {
        // TODO: All `join-like` selects are better be done like here: https://www.sea-ql.org/SeaORM/docs/basic-crud/select/#many-to-many
        // Yet, it suddenly stopped working and I can't seem to fix it. Hence, I used the approach below
        let user_groups = user_group::Entity::find()
            .filter(user_group::Column::GroupId.eq(group_id))
            .all(&self.pool)
            .await?;
        let usernames: Vec<String> = user_groups.into_iter().map(|x| x.username).collect();
        let users = user::Entity::find()
            .filter(user::Column::Username.is_in(usernames))
            .all(&self.pool)
            .await?;

        Ok(users)
    }

    pub async fn insert_expense(
        &self,
        username: &str,
        amount: Decimal,
        group_id: i64,
        note: &str,
    ) -> Result<expense::Model, Error> {
        let expense = expense::ActiveModel {
            id: NotSet,
            username: Set(username.to_owned()),
            group_id: Set(group_id),
            amount: Set(amount),
            note: Set(note.to_owned()),
        };

        Ok(expense.insert(&self.pool).await?)
    }

    pub async fn get_expenses_in_group(&self, group_id: i64) -> Result<Vec<expense::Model>, Error> {
        Ok(expense::Entity::find()
            .filter(expense::Column::GroupId.eq(group_id))
            .all(&self.pool)
            .await?)
    }

    #[allow(unused)]
    pub async fn remove_migrations(&self) -> Result<(), Error> {
        Ok(Migrator::down(&self.pool, None).await?)
    }

    pub async fn insert_group(&self, group: &str) -> Result<group::Model, Error> {
        let group = group::ActiveModel {
            id: NotSet,
            name: Set(group.to_string()),
        };
        Ok(group.insert(&self.pool).await?)
    }

    pub async fn get_group_by_id(&self, group_id: i64) -> Result<Option<group::Model>, Error> {
        Ok(group::Entity::find()
            .filter(group::Column::Id.eq(group_id))
            .one(&self.pool)
            .await?)
    }

    pub async fn add_user_to_group(&self, group_id: i64, username: &str) -> Result<(), Error> {
        let user = user::ActiveModel {
            username: Set(username.to_string()),
        };

        let user_group = user_group::ActiveModel {
            username: Set(username.to_string()),
            group_id: Set(group_id),
        };

        // TODO: This is a bit awkward and better be rewritten with `save()` commands
        if let Err(err) = user.insert(&self.pool).await {
            tracing::error!(?err, "Error occurred during `user` insertion");
        }

        if let Err(err) = user_group.insert(&self.pool).await {
            tracing::error!(?err, "Error occurred during `user_group` insertion");
        }

        Ok(())
    }

    pub async fn get_user_groups(
        &self,
        username: &str,
    ) -> Result<std::vec::Vec<group::Model>, Error> {
        let user_groups_ids: Vec<i64> = user_group::Entity::find()
            .filter(user_group::Column::Username.eq(username))
            .all(&self.pool)
            .await?
            .into_iter()
            .map(|x| x.group_id)
            .collect();

        let groups = group::Entity::find()
            .filter(group::Column::Id.is_in(user_groups_ids))
            .all(&self.pool)
            .await?;

        Ok(groups)
    }
}
