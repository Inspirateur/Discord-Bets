use crate::bets::{BetError, Bets, AccountUpdate};
use crate::front::{bet_stub, options_display, update_options, Front, FrontError};
use crate::{CURRENCY, STARTING_COINS, INCOME, handler_utils::*};
use std::sync::{atomic::AtomicBool, Arc};
use itertools::Itertools;
use serenity::http::CacheHttp;
use serenity::model::channel::GuildChannel;
use serenity::model::id::{MessageId, UserId};
use serenity::model::interactions::message_component::ButtonStyle;
use serenity::{
    http::Http,
    model::channel::Channel,
    model::channel::Message,
    model::id::GuildId,
    model::interactions::{
        application_command::{
            ApplicationCommandInteraction, ApplicationCommandInteractionDataOptionValue,
        },
        message_component::MessageComponentInteraction,
        InteractionResponseType,
    },
    prelude::*,
};
use shellwords::{split, MismatchedQuotes};

pub struct Handler {
    pub bets: Bets,
    pub front: Front,
    pub is_loop_running: AtomicBool,
}

pub async fn passive_income(ctx: Arc<Context>, bets: Arc<Bets>, front: Arc<Front>) {
    // give INCOME to every one that has an account
    match bets.income(INCOME) {
        Ok(acc_updates) => {
            if let Err(why) = front.update_account_threads(
                &ctx.http, acc_updates, format!("Passive income: **+{{diff}}** {}", CURRENCY)
            ).await {
                println!("Couldn't update account threads: {:?}", why);
            }
        },
        Err(why) => println!("Couldn't distribute income: {:?}", why)
    }
}

pub async fn response<D>(http: &Http, command: &ApplicationCommandInteraction, msg: D)
where
    D: ToString,
{
    if let Err(why) = command
        .create_interaction_response(http, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|message| message.content(msg))
        })
        .await
    {
        println!("{}", why);
    };
}

pub async fn follow_up<D>(http: &Http, command: &ApplicationCommandInteraction, msg: D)
where
    D: ToString,
{
    if let Err(why) = command
        .create_followup_message(http, |response| response.content(msg))
        .await
    {
        println!("{}", why);
    };
}

impl Handler {
    pub fn new() -> Self {
        Handler {
            bets: Bets::new("bets.db").unwrap(),
            front: Front::new("front.db").unwrap(),
            is_loop_running: AtomicBool::new(false),
        }
    }

    pub async fn make_account(&self, ctx: Context, command: ApplicationCommandInteraction) {
        // we only do something if the command was used in a server
        if let Some(guild_id) = command.guild_id {
            let guild = format!("{}", guild_id);
            let user = format!("{}", command.user.id);
            let mut new_acc = false;
            // try to create the account
            match self.bets.create_account(&guild, &user, STARTING_COINS) {
                Err(BetError::AlreadyExists) => {
                    response(
                        &ctx.http,
                        &command,
                        "You already have an account.".to_string(),
                    )
                    .await;
                }
                Err(BetError::InternalError(why)) => {
                    response(
                        &ctx.http,
                        &command,
                        format!("Internal Error while creating the Account ```{}```", why),
                    )
                    .await;
                    return;
                }
                Err(_) => {}
                Ok(_) => {
                    new_acc = true;
                    response(
                        &ctx.http,
                        &command,
                        "Your account was successfully created.".to_string(),
                    )
                    .await;
                }
            }
            // try to create the account thread
            if let Ok(balance) = self.bets.balance(&guild, &user) {
                match self
                    .front
                    .create_account_thread(&ctx, guild_id, command.channel_id, command.user.id)
                    .await
                {
                    Ok(()) => {
                        follow_up(
                            &ctx.http,
                            &command,
                            "Your account thread was successfully created.".to_string(),
                        )
                        .await;
                        let msg = if new_acc {
                            format!(
                                "Your account has been created with a starting balance of {}",
                                balance
                            )
                        } else {
                            String::from("It seems your previous Account Thread is gone, this is the new one.")
                        };
                        match self
                            .front
                            .update_account_thread(
                                &ctx.http,
                                AccountUpdate { 
                                    server: guild_id.0.to_string(),
                                    user: format!("{}", command.user.id), 
                                    diff: balance as i32, 
                                    balance: balance,
                                },
                                msg,
                            )
                            .await
                        {
                            Err(FrontError::LackPermission(perms)) => {
                                follow_up(
                                    &ctx.http,
                                    &command,
                                    format!("Cannot update the Account Thread because I am lacking the permissions: {}", perms),
                                ).await;
                            }
                            Err(FrontError::InternalError(why)) => {
                                follow_up(
                                    &ctx.http,
                                    &command,
                                    format!(
                                        "Internal error while updating Account Thread ```{}```",
                                        why
                                    ),
                                )
                                .await
                            }
                            _ => {}
                        }
                    }
                    Err(FrontError::LackPermission(perms)) => 
                    follow_up(
                        &ctx.http,
                        &command,
                        format!(
                            "Cannot create the Account Thread because I am lacking the permissions: {}",
                            perms
                        ),
                    )
                    .await,
                    Err(FrontError::AlreadyExists) => 
                        follow_up(
                            &ctx.http,
                            &command,
                            "You already have an account thread.".to_string(),
                        )
                        .await,
                    Err(FrontError::InternalError(why)) => 
                    follow_up(
                        &ctx.http,
                        &command,
                        format!(
                            "Internal error while creating the Account Thread ```{}```",
                            why
                        ),
                    )
                    .await,
                    _ => {}
                }
            }
        }
    }

    fn bet_parse(
        command: &ApplicationCommandInteraction,
    ) -> Result<(String, Vec<String>), MismatchedQuotes> {
        let desc = if let ApplicationCommandInteractionDataOptionValue::String(value) = command
            .data
            .options
            .get(0)
            .expect("Expected a description of the bet")
            .resolved
            .as_ref()
            .expect("Expected a string")
        {
            value.clone()
        } else {
            String::new()
        };
        let outcomes = split(
            if let ApplicationCommandInteractionDataOptionValue::String(value) = command
                .data
                .options
                .get(1)
                .expect("Expected outcomes for the bet")
                .resolved
                .as_ref()
                .expect("Expected a string")
            {
                value
            } else {
                ""
            },
        )?;
        Ok((desc, outcomes))
    }

    pub async fn bet(&self, ctx: Context, command: ApplicationCommandInteraction) {
        // TODO: silence notifications in bet message
        if let Ok((desc, outcomes)) = Handler::bet_parse(&command) {
            if outcomes.len() < 2 {
                response(
                    &ctx.http,
                    &command,
                    "You must define 2 outcomes or more to create a bet.",
                )
                .await;
                return;
            }
            match command
                .create_interaction_response(&ctx.http, |response| {
                    response
                        .kind(InteractionResponseType::ChannelMessageWithSource)
                        .interaction_response_data(|message| {
                            message
                                .content(&desc)
                                .components(|components| bet_components(components, OPEN))
                        })
                })
                .await
            {
                Ok(_) => {
                    if let Ok(bet_msg) = command.get_interaction_response(&ctx.http).await {
                        let mut outcomes_msg = Vec::new();
                        for outcome in options_display(&bet_stub(&outcomes)).iter() {
                            if let Ok(outcome_msg) = command
                                .channel_id
                                .send_message(&ctx.http, |messsage| {
                                    messsage.content(outcome).components(|components| {
                                        option_components(components, OPEN)
                                    })
                                })
                                .await
                            {
                                outcomes_msg.push(outcome_msg);
                            };
                        }
                        if outcomes_msg.len() == outcomes.len() {
                            // Everything is in order, we can create the bet
                            match self.bets.create_bet(
                                &format!("{}", command.guild_id.unwrap()),
                                &format!("{}", bet_msg.id),
                                &bet_msg.content,
                                &outcomes_msg
                                    .iter()
                                    .map(|msg| format!("{}", msg.id))
                                    .collect_vec(),
                                &outcomes,
                            ) {
                                Err(why) => {
                                    println!("Error while creating bet: {:?}", why)
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Err(why) => println!("{}", why),
            }
        }
    }

    pub async fn leadeboard(&self, ctx: Context, command: ApplicationCommandInteraction) {
        if let Some(guild_id) = command.guild_id {
            match self.bets.accounts(&guild_id.0.to_string()) {
                Ok(mut accounts) => {
                    accounts.sort_by_key(|acc| acc.balance+acc.in_bet);
                    accounts.reverse();
                    let msg = format!("{}  ({} in bet)   user\n", CURRENCY, CURRENCY) 
                    + &accounts.into_iter().take(10).map(|acc| 
                        format!("{}  ({})   <@{}>", acc.balance, acc.in_bet, acc.user)
                    ).join("\n") + "\n...";
                    if let Err(why) = command
                        .create_interaction_response(&ctx.http, |response| {
                            response
                                .kind(InteractionResponseType::ChannelMessageWithSource)
                                .interaction_response_data(|message| message.content(msg).allowed_mentions(|mentions| mentions.empty_users()))
                        })
                        .await
                    {
                        println!("{}", why);
                    };
                },
                Err(why) => println!("Couldn't retrieve accounts: {:?}", why)
            }
        }
    }

    pub async fn reset(&self, ctx: Context, command: ApplicationCommandInteraction) {
        if let Some(guild_id) = command.guild_id {
            if let Some(guild) = guild_id.to_guild_cached(&ctx).await {
                if let Ok(perms) = guild.member_permissions(&ctx.http, command.user.id).await {
                    if perms.administrator() {
                        if let Err(why) = command
                            .create_interaction_response(ctx.http, |response| {
                                response
                                    .kind(InteractionResponseType::ChannelMessageWithSource)
                                    .interaction_response_data(|message| 
                                        message.content("⚠️ RESETTING WILL:\n1/ ABORT EVERY ACTIVE BET\n2/ RESET EVERY ACCOUNT TO THE STARTING SUM\n(administrator only)")
                                        .components(|components| components.create_action_row(|action_row| 
                                            action_row.create_button(|button| 
                                                button.custom_id("cancel").label("Cancel").style(ButtonStyle::Secondary)
                                            ).create_button(|button|
                                                button.custom_id("reset").label("RESET").style(ButtonStyle::Danger)
                                            )
                                        ))
                                    )
                            })
                            .await
                        {
                            println!("Couldn't send reset message: {}", why);
                        };
                    } else {
                        if let Err(why) = command
                            .create_interaction_response(ctx.http, |response| {
                                response
                                    .kind(InteractionResponseType::ChannelMessageWithSource)
                                    .interaction_response_data(|message| 
                                        message.content("Resetting requires administrator permissions.")
                                    )
                            })
                            .await
                        {
                            println!("Couldn't send lack of perm message for reset: {}", why);
                        };
                    }
                }
            }
        }
    }

    async fn bet_msg_from_opt(
        &self,
        http: &Http,
        server: GuildId,
        channel: &GuildChannel,
        message_id: MessageId,
    ) -> Option<Message> {
        if let Ok(bet_message_id) = self
            .bets
            .bet_of_option(&format!("{}", server), &format!("{}", message_id))
        {
            if let Ok(bet_message) = channel
                .message(http, bet_message_id.parse::<u64>().unwrap())
                .await
            {
                return Some(bet_message);
            }
        }
        None
    }

    async fn update_bet(
        &self,
        http: &Http,
        server: GuildId,
        channel: GuildChannel,
        message: &mut Message,
        status: &str,
    ) {
        if let Ok(options) = self
            .bets
            .options_of_bet(&format!("{}", server), &format!("{}", message.id))
        {
            let content = message.content.clone();
            if let Err(why) = message
                .edit(http, |msg| {
                    msg.components(|components| bet_components(components, status));
                    if status == ABORT {
                        msg.content(format!("**ABORTED**\n{}", content));
                    }
                    msg
                })
                .await
            {
                println!("Failed to edit bet to {}: {}", status, why);
            }
            for option in options {
                if let Ok(mut option_msg) =
                    channel.message(http, option.parse::<u64>().unwrap()).await
                {
                    if let Err(why) = option_msg
                        .edit(http, |msg| {
                            msg.components(|components| option_components(components, status))
                        })
                        .await
                    {
                        println!("Failed to edit option to {}: {}", status, why);
                    }
                }
            }
        } else {
            println!("Failed to get options of the bet");
        }
    }

    async fn bet_clicked(&self, http: &Http, server: GuildId, channel: GuildChannel, message: &mut Message, user_id: UserId, percent: u32) {
        match self.bets.bet_on(
            &format!("{}", server),
            &format!("{}", message.id),
            &format!("{}", user_id),
            percent as f32 / 100.0,
        ) {
            Ok((acc, bet_status)) => {
                if let Err(why) =
                    update_options(&http, &channel, &bet_status).await
                {
                    println!("Error in updating options: {}", why);
                }
                if let Err(why) = self
                    .front
                    .update_account_thread(
                        &http,
                        acc.clone(),
                        format!(
                            "You bet **{}** {} on:\n{}",
                            -acc.diff, CURRENCY, message.content
                        ),
                    )
                    .await
                {
                    println!("Error in account thread update: {:?}", why);
                };
            }
            Err(why) => {
                println!("Error while betting: {:?}", why)
            }
        }
    }

    pub async fn button_clicked(&self, ctx: Context, command: MessageComponentInteraction) {
        if let Some(server) = command.guild_id {
            if let Ok(Channel::Guild(channel)) = command.channel_id.to_channel(&ctx.http).await {
                let user = command.user;
                if let Some(mut message) = command.message.regular() {
                    let button = command.data.custom_id.as_str();
                    if let Ok(i) = button.parse::<usize>() {
                        let percent = BET_OPTS[i];
                        self.bet_clicked(&ctx.http, server, channel, &mut message, user.id, percent).await;
                    } else if button == CANCEL || button == RESET {
                        // check for admin perm
                        if let Some(guild) = server.to_guild_cached(&ctx).await {
                            if let Ok(perms) = guild.member_permissions(&ctx.http, user.id).await {
                                if perms.administrator() {
                                    if button == CANCEL {
                                        if let Err(why) = message.edit(&ctx.http, |msg| msg.components(|components| components).content("RESET CANCELLED")).await {
                                            println!("Couldn't cancel the reset: {}", why);
                                        }
                                    } else if button == RESET {
                                        // reset every account on the server
                                        if let Err(why) = self.bets.reset(&server.0.to_string(), STARTING_COINS) {
                                            println!("Couldn't reset accounts in db: {:?}", why);
                                        } else if let Err(why) = message.edit(&ctx.http, |msg| msg.components(|components| components).content("ALL ACCOUNTS ARE RESET")).await {
                                            println!("Couldn't edit the reset message: {}", why);
                                        } else if let Err(why) = self.front.update_account_thread_reset(&ctx.http, &server.0.to_string()).await {
                                            println!("Couldn't display reset in account threads: {:?}", why);
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        let mut bet_msg = message.clone();
                        if button == WIN {
                            match self
                                .bet_msg_from_opt(&ctx.http, server, &channel, message.id)
                                .await 
                            { 
                                Some(bet_msg_) => bet_msg = bet_msg_,
                                None => println!("Couldn't retrieve bet message from win option")
                            }
                        }
                        if let Some(interaction) = &bet_msg.interaction {
                            if interaction.user.id == user.id {
                                if button == LOCK {
                                    if let Ok(()) = self
                                        .bets
                                        .lock_bet(&format!("{}", server), &format!("{}", message.id))
                                    {
                                        self.update_bet(&ctx.http, server, channel, &mut message, LOCK)
                                            .await;
                                    }
                                } else if button == ABORT {
                                    if let Ok(account_updates) = self.bets.abort_bet(
                                        &format!("{}", server),
                                        &format!("{}", message.id),
                                    ) {
                                        // Announce the aborting
                                        if let Err(why) = message.reply(
                                            &ctx.http, 
                                            "The bet has been aborted ! Wagers are being refunded.", 
                                        ).await {
                                            println!("Couldn't reply to announce the aborting: {}", why);
                                        }
                                        // pass bet in "ABORT" state in front end
                                        self.update_bet(
                                            &ctx.http(),
                                            server,
                                            channel,
                                            &mut message,
                                            ABORT,
                                        )
                                        .await;
                                        // update the accounts
                                        if let Err(why) = self
                                            .front
                                            .update_account_threads(
                                                &ctx.http,
                                                account_updates,
                                                format!(
                                                    "You got back **{{diff}}** {} because the bet was aborted",
                                                    CURRENCY
                                                ),
                                            )
                                            .await
                                        {
                                            println!(
                                                "Couldn't update account thread: {:?}",
                                                why
                                            );
                                        }
                                    }
                                } else if button == WIN {
                                     if let Ok(account_updates) = self.bets.close_bet(
                                        &format!("{}", server),
                                        &format!("{}", message.id),
                                    ) {
                                        // Announce the win
                                        let content = message.content.clone();
                                        if let Err(why) = bet_msg.reply(
                                            &ctx.http, 
                                            format!(
                                                "{}\nhas won ! **{}** {} is shared between the winners.", 
                                                content,
                                                account_updates.iter().fold(0, |i, acc| i + acc.diff), 
                                                CURRENCY
                                            )
                                        ).await {
                                            println!("Couldn't reply to announce the winner: {}", why);
                                        }
                                        // pass bet in "WIN" state in front end
                                        self.update_bet(
                                            &ctx.http(),
                                            server,
                                            channel,
                                            &mut bet_msg,
                                            WIN,
                                        )
                                        .await;
                                        // update winning option
                                        if let Err(why) = message
                                        .edit(&ctx.http, |msg| {
                                            msg.content(format!("**WINNER**\n{}", content))
                                        })
                                        .await 
                                        {
                                            println!("Couldn't edit winning option: {}", why);
                                        }
                                        // update the accounts
                                        if let Err(why) = self
                                            .front
                                            .update_account_threads(
                                                &ctx.http,
                                                account_updates,
                                                format!(
                                                    "You won **{{diff}}** {} by betting on:\n{}",
                                                    CURRENCY, content
                                                ),
                                            )
                                            .await
                                        {
                                            println!(
                                                "Couldn't update account thread: {:?}",
                                                why
                                            );
                                        }
                                    }
                                }
                            } else {
                                println!("user can't administrate the bet");
                            }
                        }
                    }
                }
            }
        }
    }
}
