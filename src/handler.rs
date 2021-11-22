use crate::bets::{BetError, Bets};
use crate::front::{bet_stub, options_display, update_options, Front, FrontError};
use crate::handler_utils::*;
use itertools::Itertools;
use serenity::http::CacheHttp;
use serenity::model::channel::GuildChannel;
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
    bets: Bets,
    front: Front,
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

impl Handler {
    pub fn new() -> Self {
        Handler {
            bets: Bets::new("bets.db").unwrap(),
            front: Front::new("front.db").unwrap(),
        }
    }

    pub async fn make_account(&self, ctx: Context, command: ApplicationCommandInteraction) {
        // we only do something if the command was used in a server
        if let Some(guild_id) = command.guild_id {
            let guild = format!("{}", guild_id);
            let user = format!("{}", command.user.id);
            let mut new_acc = false;
            let mut resp: Vec<String> = Vec::new();
            // try to create the account
            match self.bets.create_account(&guild, &user, 10) {
                Err(BetError::AlreadyExists) => {
                    resp.push("You already have an account.".to_string())
                }
                Err(BetError::InternalError(why)) => {
                    resp.push(format!(
                        "Internal Error while creating the Account ```{}```",
                        why
                    ));
                    return;
                }
                Err(_) => {}
                Ok(_) => {
                    new_acc = true;
                    resp.push("Your account was successfully created.".to_string());
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
                        resp.push("Your account thread was successfully created.".to_string());
                        let msg = if new_acc {
                            format!(
                                "Your account has been created with a starting balance of {}",
                                balance
                            )
                        } else {
                            String::from("It seems your previous Account Thread is gone, this is the new one.")
                        };
                        match self.front.update_account_thread(
                            &ctx.http,
                            guild_id,
                            command.user.id,
                            balance,
                            msg,
                        )
                        .await
                        {
                            Err(FrontError::LackPermission(perms)) => {
                                resp.push(format!("Cannot update the Account Thread because I am lacking the permissions: {}", perms))
                            }
                            Err(FrontError::InternalError(why)) => {
                                resp.push(format!("Internal error while updating Account Thread ```{}```", why))
                            }
                            _ => {}
                        }
                    }
                    Err(FrontError::LackPermission(perms)) => resp.push(format!(
                        "Cannot create the Account Thread because I am lacking the permissions: {}",
                        perms
                    )),
                    Err(FrontError::AlreadyExists) => {
                        resp.push("You already have an account thread.".to_string())
                    }
                    Err(FrontError::InternalError(why)) => resp.push(format!(
                        "Internal error while creating the Account Thread ```{}```",
                        why
                    )),
                    _ => {}
                }
            }
            response(&ctx.http, &command, resp.join("\n")).await;
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
        response(&ctx.http, &command, "nomegalul").await;
    }

    async fn update_bet_components(
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
            if let Err(why) = message
                .edit(http, |msg| {
                    msg.components(|components| bet_components(components, status))
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
        }
    }

    pub async fn button_clicked(&self, ctx: Context, command: MessageComponentInteraction) {
        if let Some(server) = command.guild_id {
            if let Ok(Channel::Guild(channel)) = command.channel_id.to_channel(&ctx.http).await {
                let user = command.user;
                let message_id = command.message.id();
                if let Ok(mut message) = channel.message(&ctx.http, message_id).await {
                    match command.data.custom_id.as_str() {
                        LOCK => {
                            let mut can_lock = false;
                            if let Some(interaction) = &message.interaction {
                                if interaction.user.id == user.id {
                                    can_lock = true;
                                }
                                if let Ok(perms) = channel.permissions_for_user(&ctx, user.id).await
                                {
                                    can_lock = perms.manage_channels();
                                }
                            }

                            if can_lock {
                                if let Ok(()) = self
                                    .bets
                                    .lock_bet(&format!("{}", server), &format!("{}", message_id))
                                {
                                    self.update_bet_components(
                                        &ctx.http,
                                        server,
                                        channel,
                                        &mut message,
                                        LOCK,
                                    )
                                    .await;
                                }
                            } else {
                                println!("user can't lock");
                            }
                        }
                        ABORT => {
                            println!("abort");
                        }
                        WIN => {
                            let mut can_close = false;
                            match channel.permissions_for_user(&ctx.cache, user.id).await {
                                Ok(perms) => {
                                    println!("nani the fuck {}", perms);
                                    can_close = perms.manage_channels();
                                }
                                Err(why) => println!("Couldn't get perms of user: {}", why),
                            }
                            if can_close {
                                if let Ok(account_updates) = self
                                    .bets
                                    .close_bet(&format!("{}", server), &format!("{}", message_id))
                                {
                                    self.update_bet_components(
                                        &ctx.http(),
                                        server,
                                        channel,
                                        &mut message,
                                        WIN,
                                    )
                                    .await;
                                    for account_update in account_updates {
                                        if let Err(why) = self
                                            .front
                                            .update_account_thread(
                                                &ctx.http,
                                                server,
                                                account_update.user.parse::<u64>().unwrap().into(),
                                                account_update.balance,
                                                format!("You won {}", account_update.diff),
                                            )
                                            .await
                                        {
                                            println!("Couldn't update account thread: {:?}", why);
                                        }
                                    }
                                }
                            } else {
                                println!("user can't close");
                            }
                        }
                        i => {
                            let percent = BET_OPTS[i.parse::<usize>().unwrap()];
                            match self.bets.bet_on(
                                &format!("{}", server),
                                &format!("{}", message_id),
                                &format!("{}", user.id),
                                percent as f32 / 100.0,
                            ) {
                                Ok((acc, bet_status)) => {
                                    if let Err(why) =
                                        update_options(&ctx.http, &channel, &bet_status).await
                                    {
                                        println!("Error in updating options: {}", why);
                                    }
                                    if let Err(why) = self
                                        .front
                                        .update_account_thread(
                                            &ctx.http,
                                            server,
                                            user.id,
                                            acc.balance,
                                            format!("You bet {} on {}", -acc.diff, ""),
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
                    }
                }
            }
        }
    }
}
