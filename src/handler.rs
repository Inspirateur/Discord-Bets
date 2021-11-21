use crate::bets::{BetError, Bets};
use crate::front::{bet_stub, options_display, Front, FrontError};
use serenity::{
    http::Http,
    model::interactions::{
        application_command::{
            ApplicationCommandInteraction, ApplicationCommandInteractionDataOptionValue,
        },
        message_component::{ButtonStyle, MessageComponentInteraction},
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
                            message.content(&desc).components(|components| {
                                components.create_action_row(|action_row| {
                                    action_row
                                        .create_button(|button| {
                                            button
                                                .custom_id(0)
                                                .style(ButtonStyle::Primary)
                                                .label("Lock")
                                        })
                                        .create_button(|button| {
                                            button
                                                .custom_id(1)
                                                .style(ButtonStyle::Danger)
                                                .label("Abort")
                                        })
                                })
                            })
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
                                        components.create_action_row(|action_row| {
                                            action_row
                                                .create_button(|button| {
                                                    button
                                                        .custom_id(0)
                                                        .style(ButtonStyle::Secondary)
                                                        .label("10 %")
                                                })
                                                .create_button(|button| {
                                                    button
                                                        .custom_id(1)
                                                        .style(ButtonStyle::Secondary)
                                                        .label("50 %")
                                                })
                                                .create_button(|button| {
                                                    button
                                                        .custom_id(2)
                                                        .style(ButtonStyle::Secondary)
                                                        .label("All In")
                                                })
                                        })
                                    })
                                })
                                .await
                            {
                                outcomes_msg.push(outcome_msg);
                            };
                        }
                        if outcomes_msg.len() == outcomes.len() {}
                    }
                }
                Err(why) => println!("{}", why),
            }
        }
    }

    pub async fn leadeboard(&self, ctx: Context, command: ApplicationCommandInteraction) {
        response(&ctx.http, &command, "nomegalul").await;
    }

    pub async fn button_clicked(&self, ctx: Context, command: MessageComponentInteraction) {
        todo!()
    }
}
