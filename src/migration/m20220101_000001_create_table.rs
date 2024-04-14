use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(User::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(User::Username)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Group::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Group::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Group::Name).string().not_null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(UserGroup::Table)
                    .col(ColumnDef::new(UserGroup::Username).string().not_null())
                    .col(ColumnDef::new(UserGroup::GroupId).integer().not_null())
                    .primary_key(
                        Index::create()
                            .name("pk-user_group")
                            .col(UserGroup::Username)
                            .col(UserGroup::GroupId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-user_group-username")
                            .from(UserGroup::Table, UserGroup::Username)
                            .to(User::Table, User::Username)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-user_group-group_id")
                            .from(UserGroup::Table, UserGroup::GroupId)
                            .to(Group::Table, Group::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Expense::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Expense::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Expense::Note).string())
                    .col(ColumnDef::new(Expense::Amount).decimal().not_null())
                    .col(ColumnDef::new(Expense::Username).integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .from(Expense::Table, Expense::Username)
                            .to(User::Table, User::Username)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .col(ColumnDef::new(Expense::GroupId).integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .from(Expense::Table, Expense::GroupId)
                            .to(Group::Table, Group::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Expense::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(User::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Group::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(UserGroup::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
enum User {
    Table,
    Username,
}

#[derive(DeriveIden)]
enum Group {
    Table,
    Id,
    Name,
}

#[derive(DeriveIden)]
enum UserGroup {
    Table,
    Username,
    GroupId,
}

#[derive(DeriveIden)]
enum Expense {
    Table,
    Id,
    Amount,
    Username,
    GroupId,
    Note,
}
