use crate::{
    db,
    entity::{expense, group},
};
use rust_decimal::Decimal;
use teloxide::{
    types::{ChatId, UserId},
    Bot,
};

pub struct Controller<'a> {
    pub bot: &'a Bot,
    pub db: &'a db::Database,
    pub user_id: UserId,
    pub chat_id: ChatId,
}

impl<'a> Controller<'a> {
    pub async fn get_expenses_in_group(
        &self,
        group_id: i64,
    ) -> anyhow::Result<Vec<expense::Model>> {
        self.db
            .get_expenses_in_group(group_id)
            .await
            .map_err(|err| anyhow::anyhow!("Retrieving expenses failed. Err: {err}"))
    }

    pub async fn user_is_in_group(&self, username: &str, group_id: i64) -> anyhow::Result<bool> {
        let users_in_group = self
            .db
            .get_users_in_group(group_id)
            .await
            .map_err(|err| anyhow::anyhow!("Retrieving users in group failed. Err: {err}"))?;

        Ok(users_in_group.iter().any(|uig| uig.username == username))
    }

    pub async fn add_expense(
        &self,
        username: &str,
        amount: Decimal,
        group_id: i64,
        note: &str,
    ) -> anyhow::Result<expense::Model> {
        self.db
            .insert_expense(username, amount, group_id, note)
            .await
            .map_err(|err| anyhow::anyhow!("Expense insertion failed. Err: {err}"))
    }

    pub async fn create_group(&self, group_name: &str) -> anyhow::Result<group::Model> {
        self.db
            .insert_group(group_name)
            .await
            .map_err(|err| anyhow::anyhow!("Group creation failed. Err: {err}"))
    }

    pub async fn add_user_to_a_group(&self, username: &str, group_id: i64) -> anyhow::Result<()> {
        self.db
            .add_user_to_group(group_id, username)
            .await
            .map_err(|err| anyhow::anyhow!("Adding user to group failed. Err: {err}"))
    }

    pub async fn get_user_groups(&self, username: &str) -> anyhow::Result<Vec<group::Model>> {
        self.db
            .get_user_groups(username)
            .await
            .map_err(|err| anyhow::anyhow!("Retrieving user groups failed. Err: {err}"))
    }

    pub async fn get_group_by_id(&self, group_id: i64) -> anyhow::Result<Option<group::Model>> {
        self.db
            .get_group_by_id(group_id)
            .await
            .map_err(|err| anyhow::anyhow!("Retrieving group by id failed. Err: {err}"))
    }
}
