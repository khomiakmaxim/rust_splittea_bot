use crate::{cli::CLI, controller::Controller, db::Database, entity::group};
use async_once::AsyncOnce;
use rust_decimal::Decimal;
use std::{cmp::Ordering, collections::HashMap};
use teloxide::{
    dispatching::dialogue::{self, InMemStorage},
    prelude::*,
    utils::command::BotCommands,
};
use tracing::info;

type MyDialogue = Dialogue<ChatState, InMemStorage<ChatState>>;
type HandlerResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

#[derive(BotCommands, Clone, Debug)]
#[command(
    rename_rule = "lowercase",
    description = "Splittea supports the following commands:"
)]
enum Command {
    #[command(description = "display this text")]
    Help,
    #[command(description = "create new group and put yourself as it's first member")]
    CreateGroup,
    #[command(description = "add member to a group")]
    AddMemberToGroup,
    #[command(description = "add an expense")]
    AddExpense,
    #[command(description = "list all expenses in a group")]
    ListExpensesInGroup,
    #[command(description = "list all your groups")]
    ListMyGroups,
    #[command(description = "cancel whatever you do")]
    Cancel,
}

#[derive(Clone, Default)]
enum ChatState {
    #[default]
    Start,
    // ----- Add new expense
    ReceiveGroupIdForExpense,
    RecieveAmountSpent {
        group_id: i64,
    },
    ReceiveNote {
        group_id: i64,
        amount: Decimal,
    },
    // ----- Add new group
    ReceiveGroupName,
    // ----- Add memeber to a group
    ReceiveGroupIdForAddMember,
    ReceiveUsername {
        group_id: i64,
    },
    // ----- List expenses in group
    ReceiveGroupIdForExpensesList,
}

lazy_static::lazy_static! {
    static ref DATABASE: AsyncOnce<Database> = AsyncOnce::new(async {
        Database::new(&CLI.database)
            .await
            .unwrap_or_else(|err| panic!("Failed to connect to database {:?}: {}", CLI.database, err))
    });
}

pub async fn run() -> anyhow::Result<()> {
    info!("Starting running splittea...");
    DATABASE
        .get()
        .await
        .apply_migrations()
        .await
        .expect("Failed to apply database migrations");

    let bot = Bot::new(&CLI.token);
    bot.set_my_commands(Command::bot_commands()).await?;

    use dptree::case;
    let command_handler = teloxide::filter_command::<Command, _>()
        .branch(
            case![ChatState::Start]
                .branch(case![Command::Help].endpoint(help))
                .branch(case![Command::ListMyGroups].endpoint(list_my_groups))
                .branch(case![Command::CreateGroup].endpoint(create_group))
                .branch(case![Command::AddMemberToGroup].endpoint(add_member_to_group))
                .branch(case![Command::AddExpense].endpoint(add_expense))
                .branch(case![Command::ListExpensesInGroup].endpoint(list_expenses_in_group))
                .branch(case![Command::Cancel].endpoint(cancel)),
        )
        .branch(case![Command::Cancel].endpoint(cancel));

    let message_handler = Update::filter_message()
        // ----- Create group
        .branch(case![ChatState::ReceiveGroupName].endpoint(receive_group_name))
        // ----- Add member to a group
        .branch(
            case![ChatState::ReceiveGroupIdForAddMember].endpoint(receive_group_id_for_add_member),
        )
        .branch(case![ChatState::ReceiveUsername { group_id }].endpoint(receive_user_name))
        // ----- Add expense
        .branch(case![ChatState::ReceiveGroupIdForExpense].endpoint(receive_group_id_for_expense))
        .branch(case![ChatState::RecieveAmountSpent { group_id }].endpoint(receive_amount_spent))
        .branch(case![ChatState::ReceiveNote { group_id, amount }].endpoint(receive_note))
        // ----- List expenses in group
        .branch(
            case![ChatState::ReceiveGroupIdForExpensesList]
                .endpoint(receive_group_id_for_expenses_list),
        );

    let composed_handler = Update::filter_message()
        .branch(command_handler)
        .branch(message_handler)
        .branch(dptree::endpoint(invalid_state));

    let handler =
        dialogue::enter::<Update, InMemStorage<ChatState>, ChatState, _>().branch(composed_handler);

    info!("Ready for listening commands hand messages...");
    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![InMemStorage::<ChatState>::new()])
        .error_handler(LoggingErrorHandler::with_custom_text(
            "An error has occurred in the dispatcher",
        ))
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    Ok(())
}

async fn help(bot: Bot, msg: Message) -> HandlerResult {
    bot.send_message(msg.chat.id, Command::descriptions().to_string())
        .await?;
    Ok(())
}

fn groups_to_pretty(groups: Vec<group::Model>) -> String {
    groups
        .iter()
        .map(|model| format!("{} — `{}`\n", model.id, model.name))
        .collect::<Vec<String>>()
        .join(", ")
}

async fn list_my_groups(bot: Bot, dialogue: MyDialogue, msg: Message) -> HandlerResult {
    let username = get_author_username(&msg).await?;
    let ctl = Controller::from_msg(&bot, &msg).await?;

    let groups = ctl.get_user_groups(&username).await?;
    if groups.is_empty() {
        bot.send_message(msg.chat.id, "You don't belong to any group yet")
            .await?;
        dialogue.update(ChatState::Start).await?;
    } else {
        let groups = groups_to_pretty(groups);
        let text = format!("Here are your groups:\n {}", groups);

        bot.send_message(msg.chat.id, text).await?;
    }

    Ok(())
}

async fn create_group(bot: Bot, dialogue: MyDialogue, msg: Message) -> HandlerResult {
    let text = "Pick a name for your group";
    bot.send_message(msg.chat.id, text).await?;
    dialogue.update(ChatState::ReceiveGroupName).await?;
    Ok(())
}

async fn receive_group_name(bot: Bot, dialogue: MyDialogue, msg: Message) -> HandlerResult {
    if let Some(group_name) = msg.text() {
        let username = get_author_username(&msg).await?;

        let ctl = Controller::from_msg(&bot, &msg).await?;
        let cretaed_group = ctl.create_group(group_name).await?;
        ctl.add_user_to_a_group(&username, cretaed_group.id).await?;

        let text = format!(
            "Group `{}` was successfully created and you've been added to it",
            group_name
        );
        bot.send_message(dialogue.chat_id(), text).await?;
        dialogue.update(ChatState::Start).await?;
    }

    Ok(())
}

async fn add_member_to_group(bot: Bot, msg: Message, dialogue: MyDialogue) -> HandlerResult {
    let username = get_author_username(&msg).await?;
    let ctl = Controller::from_msg(&bot, &msg).await?;

    let groups = ctl.get_user_groups(&username).await?;
    if groups.is_empty() {
        bot.send_message(msg.chat.id, "You don't belong to any group yet")
            .await?;

        dialogue.update(ChatState::Start).await?;
    } else {
        let groups = groups_to_pretty(groups);
        let text = format!(
            "Choose id of the group where you want to add a member:\n {}",
            groups
        );

        bot.send_message(msg.chat.id, text).await?;

        dialogue
            .update(ChatState::ReceiveGroupIdForAddMember)
            .await?;
    }

    Ok(())
}

async fn add_expense(bot: Bot, msg: Message, dialogue: MyDialogue) -> HandlerResult {
    let username = get_author_username(&msg).await?;
    let ctl = Controller::from_msg(&bot, &msg).await?;

    let groups = ctl.get_user_groups(&username).await?;
    if groups.is_empty() {
        bot.send_message(msg.chat.id, "You don't belong to any group yet")
            .await?;
        dialogue.update(ChatState::Start).await?;
    } else {
        let groups = groups_to_pretty(groups);
        let text = format!(
            "Choose id of the group you'd like to add the expense:\n {}",
            groups
        );
        bot.send_message(msg.chat.id, text).await?;
        dialogue.update(ChatState::ReceiveGroupIdForExpense).await?;
    }

    Ok(())
}

async fn receive_group_id_for_expense(
    bot: Bot,
    msg: Message,
    dialogue: MyDialogue,
) -> HandlerResult {
    if let Some(group_id) = msg.text() {
        if let Ok(group_id) = group_id.parse::<i64>() {
            let ctl = Controller::from_msg(&bot, &msg).await?;

            let group_name = ctl
                .get_group_by_id(group_id)
                .await?
                .ok_or(anyhow::anyhow!("Inexistent group id"))?
                .name;

            let msg = format!(
                "You want to add an expense to a group `{}`.\n Now, type the amount you spent:",
                group_name
            );

            bot.send_message(dialogue.chat_id(), msg).await?;

            dialogue
                .update(ChatState::RecieveAmountSpent { group_id })
                .await?;
        } else {
            bot.send_message(msg.chat.id, "Please, send a decimal value")
                .await?;
        }
    }

    Ok(())
}

#[derive(Debug)]
struct UserDebt {
    username: String,
    debt: Decimal,
}

async fn receive_group_id_for_expenses_list(
    bot: Bot,
    dialogue: MyDialogue,
    msg: Message,
) -> HandlerResult {
    if let Some(group_id) = msg.text() {
        if let Ok(group_id) = group_id.parse::<i64>() {
            let ctl = Controller::from_msg(&bot, &msg).await?;
            let expenses_in_group = ctl.get_expenses_in_group(group_id).await?;

            if !expenses_in_group.is_empty() {
                let mut user_spent: HashMap<String, Decimal> = HashMap::new();

                for exp in expenses_in_group.iter() {
                    *user_spent.entry(exp.username.clone()).or_default() += exp.amount;
                }

                // How much many was spent overall
                let sum: Decimal = user_spent.iter().map(|x| *x.1).sum();
                assert!(!user_spent.is_empty());

                // Mean spent per user in a group
                let mean = sum / Decimal::from(user_spent.len());

                // How much everybody owes to the group. Negative value means that this person is owed by the group
                let user_debt: Vec<(String, Decimal)> =
                    user_spent.into_iter().map(|x| (x.0, x.1 - mean)).collect();

                // Debt must sum up to zero
                debug_assert!(user_debt.iter().map(|x| x.1).sum::<Decimal>() == Decimal::from(0));

                let mut creditors: Vec<UserDebt> = Vec::new();
                let mut debitors: Vec<UserDebt> = Vec::new();
                let mut transactions: Vec<(String, String, Decimal)> = Vec::new();

                // Separate users into creditors and debitors
                for (username, debt) in user_debt {
                    match debt.cmp(&Decimal::ZERO) {
                        Ordering::Greater => {
                            creditors.push(UserDebt {
                                username: username.clone(),
                                debt: debt.abs(),
                            });
                        }
                        Ordering::Less => {
                            debitors.push(UserDebt {
                                username: username.clone(),
                                debt: debt.abs(),
                            });
                        }
                        _ => (),
                    }
                }

                // Sort creditors and debtors by debt amount
                creditors.sort_by(|a, b| b.debt.cmp(&a.debt));
                debitors.sort_by(|a, b| b.debt.cmp(&a.debt));

                // Match debtors and creditors
                let mut debitor_index = 0;
                let mut creditor_index = 0;

                while debitor_index < debitors.len() && creditor_index < creditors.len() {
                    let debtor = &debitors[debitor_index];
                    let creditor = &creditors[creditor_index];

                    // Calculate the amount to transfer
                    let transfer_amount = debtor.debt.min(creditor.debt);

                    // Record the transaction
                    transactions.push((
                        debtor.username.clone(),
                        creditor.username.clone(),
                        transfer_amount,
                    ));

                    // Adjust the debt values
                    debitors[debitor_index].debt -= transfer_amount;
                    creditors[creditor_index].debt -= transfer_amount;

                    // If a debtor's debt is fully matched, move to the next debitor
                    if debitors[debitor_index].debt == Decimal::ZERO {
                        debitor_index += 1;
                    }

                    // If a creditor's debt is fully matched, move to the next creditor
                    if creditors[creditor_index].debt == Decimal::ZERO {
                        creditor_index += 1;
                    }
                }

                let mut text = String::from("Group debt state:\n");

                if transactions.is_empty() {
                    text.push_str("😊No debt in this group😊");
                }

                for (x, y, amount) in transactions {
                    let formatted_string = format!("😑{} owes {} to {}😑\n", x, amount, y);
                    text.push_str(&formatted_string);
                }

                text.push_str("\n --- \n Overall expenses in group:\n");

                for exp in expenses_in_group {
                    let formatted_string = format!(
                        "{} spent {} with note: {}\n",
                        exp.username, exp.amount, exp.note
                    );
                    text.push_str(&formatted_string);
                }

                bot.send_message(msg.chat.id, text).await?;
            } else {
                bot.send_message(msg.chat.id, "There are no expenses yet in this group")
                    .await?;
            }

            dialogue.update(ChatState::Start).await?;
        }
    }

    Ok(())
}

async fn receive_note(
    bot: Bot,
    dialogue: MyDialogue,
    msg: Message,
    data: (i64, rust_decimal::Decimal),
) -> HandlerResult {
    let (group_id, amount) = data;
    if let Some(note) = msg.text() {
        let username = get_author_username(&msg).await?;
        let ctl = Controller::from_msg(&bot, &msg).await?;

        ctl.add_expense(&username, amount, group_id, note).await?;
        bot.send_message(msg.chat.id, "The expense has been added")
            .await?;

        dialogue.update(ChatState::Start).await?;
    }

    Ok(())
}

async fn receive_amount_spent(
    bot: Bot,
    dialogue: MyDialogue,
    msg: Message,
    group_id: i64,
) -> HandlerResult {
    if let Some(amount) = msg.text() {
        if let Ok(amount) = amount.parse::<Decimal>() {
            if amount > 0.into() {
                bot.send_message(msg.chat.id, "Provide some note:").await?;

                dialogue
                    .update(ChatState::ReceiveNote { group_id, amount })
                    .await?;
            } else {
                bot.send_message(msg.chat.id, "Please, provide some positive amount:")
                    .await?;
            }
        } else {
            bot.send_message(msg.chat.id, "Please, provide some decimal value:")
                .await?;
        }
    }

    Ok(())
}

async fn receive_user_name(
    bot: Bot,
    dialogue: MyDialogue,
    msg: Message,
    group_id: i64,
) -> HandlerResult {
    if let Some(nickname) = msg.text() {
        if nickname.is_empty() {
            bot.send_message(
                msg.chat.id,
                "Please, provide a username, starting from `@`:",
            )
            .await?;
        } else if nickname.starts_with('@') {
            let ctl = Controller::from_msg(&bot, &msg).await?;
            ctl.add_user_to_a_group(nickname, group_id).await?;

            let text = format!("User {} has been successfully added to a group", nickname);
            bot.send_message(msg.chat.id, text).await?;

            dialogue.update(ChatState::Start).await?;
        } else {
            bot.send_message(
                msg.chat.id,
                "Please, provide a username, starting from `@`:",
            )
            .await?;
        }
    }

    Ok(())
}

async fn receive_group_id_for_add_member(
    bot: Bot,
    dialogue: MyDialogue,
    msg: Message,
) -> HandlerResult {
    if let Some(group_id) = msg.text() {
        if let Ok(group_id) = group_id.parse::<i64>() {
            let username = get_author_username(&msg).await?;
            let ctl = Controller::from_msg(&bot, &msg).await?;

            if ctl.user_is_in_group(&username, group_id).await? {
                bot.send_message(msg.chat.id, "Provide @username of that user: ")
                    .await?;
                dialogue
                    .update(ChatState::ReceiveUsername { group_id })
                    .await?;
            } else {
                bot.send_message(msg.chat.id, "Please, provide id from the list: ")
                    .await?;
            }
        } else {
            bot.send_message(msg.chat.id, "Please, send an integer value: ")
                .await?;
        }
    }

    Ok(())
}

async fn get_author_username(
    msg: &Message,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    if let Some(user) = msg.from() {
        if let Some(ref username) = user.username {
            Ok(format!("@{}", username.to_owned()))
        } else {
            Err("😔Sorry, I can't detect your username😔".into())
        }
    } else {
        Err("😔Sorry, I can't get info about you😔".into())
    }
}

async fn cancel(bot: Bot, msg: Message, dialogue: MyDialogue) -> HandlerResult {
    bot.send_message(msg.chat.id, "Canceled whatever you did")
        .await?;
    dialogue.exit().await?;
    Ok(())
}

async fn list_expenses_in_group(bot: Bot, msg: Message, dialogue: MyDialogue) -> HandlerResult {
    let username = get_author_username(&msg).await?;
    let ctl = Controller::from_msg(&bot, &msg).await?;

    let groups = ctl.get_user_groups(&username).await?;
    if groups.is_empty() {
        bot.send_message(
            msg.chat.id,
            "You are not a member of any group, yet. You can create one with /creategroup",
        )
        .await?;
    } else {
        let groups = groups_to_pretty(groups);
        let text = format!("Good, choose id of one of your groups:\n {}", groups);
        bot.send_message(msg.chat.id, text).await?;
    }

    dialogue
        .update(ChatState::ReceiveGroupIdForExpensesList)
        .await?;

    Ok(())
}

impl<'a> Controller<'a> {
    pub async fn new(
        bot: &'a Bot,
        chat_id: ChatId,
        user_id: UserId,
    ) -> anyhow::Result<Controller<'a>> {
        Ok(Self {
            db: DATABASE.get().await,
            bot,
            chat_id,
            user_id,
        })
    }

    pub async fn from_msg(bot: &'a Bot, msg: &Message) -> anyhow::Result<Controller<'a>> {
        Self::new(
            bot,
            msg.chat.id,
            msg.from().ok_or(anyhow::anyhow!("User not found"))?.id,
        )
        .await
    }
}

async fn invalid_state(bot: Bot, msg: Message) -> HandlerResult {
    bot.send_message(
        msg.chat.id,
        "Unable to handle the message. Type /help to see the usage.",
    )
    .await?;
    Ok(())
}
