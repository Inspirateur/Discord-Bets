mod bets;
mod front;
use bets::{BetError, Bets};
use front::is_readable;
use serenity::{
    async_trait,
    http::Http,
    model::{
        gateway::Ready,
        id::GuildId,
        interactions::{
            application_command::{
                ApplicationCommandInteraction, ApplicationCommandInteractionDataOptionValue,
                ApplicationCommandOptionType,
            },
            Interaction, InteractionResponseType,
        },
    },
    prelude::*,
};
use shellwords::{split, MismatchedQuotes};
use std::env;

use crate::front::{Front, FrontError};

struct Handler {
    bets: Bets,
    front: Front,
}

async fn response<D>(http: &Http, command: &ApplicationCommandInteraction, msg: D)
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
            response(
                &ctx.http,
                &command,
                format!("{}\n{}", desc, outcomes.join("\n")),
            )
            .await;
        }
    }

    pub async fn leadeboard(&self, ctx: Context, command: ApplicationCommandInteraction) {
        response(&ctx.http, &command, "nomegalul").await;
    }
}

#[async_trait]
impl EventHandler for Handler {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        // only answer if the bot has access to the channel
        if let Interaction::ApplicationCommand(command) = interaction {
            if is_readable(&ctx, command.channel_id).await {
                match command.data.name.as_str() {
                    "make_account" => self.make_account(ctx, command).await,
                    "bet" => self.bet(ctx, command).await,
                    "leaderboard" => self.leadeboard(ctx, command).await,
                    _ => {}
                };
            } else {
                response(
                    &ctx.http,
                    &command,
                    "Sorry, I only answer to commands in the channels that I can read.",
                )
                .await;
            }
        }
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);

        for guild in ready.guilds {
            println!("Registering slash commands for Guild {}", guild.id());
            if let Err(why) =
                GuildId::set_application_commands(&guild.id(), &ctx.http, |commands| {
                    commands
                        .create_application_command(|command| {
                            command.name("make_account").description(
                                "Create an account and displays it as a thread under this channel.",
                            )
                        })
                        .create_application_command(|command| {
                            command
                                .name("bet")
                                .description("Create a bet.")
                                .create_option(|option| {
                                    option
                                        .name("desc")
                                        .description("The description of the bet")
                                        .kind(ApplicationCommandOptionType::String)
                                        .required(true)
                                })
                                .create_option(|option| {
                                    option
                                        .name("options")
                                        .description("The possible outcomes of the bet")
                                        .kind(ApplicationCommandOptionType::String)
                                        .required(true)
                                })
                        })
                        .create_application_command(|command| {
                            command
                                .name("leaderboard")
                                .description("Displays the leadeboard.")
                                .create_option(|option| {
                                    option
                                        .name("permanent")
                                        .description("To make a ever updating leaderboard")
                                        .kind(ApplicationCommandOptionType::Boolean)
                                        .required(false)
                                })
                        })
                })
                .await
            {
                println!("{}", why);
            };
        }
    }
}

#[tokio::main]
async fn main() {
    // Configure the client with your Discord bot token in the environment.
    let token = env::var("GOTOH_TOKEN").expect("Expected a token in the environment");

    // The Application Id is usually the Bot User Id.
    let application_id: u64 = env::var("GOTOH_ID")
        .expect("Expected an application id in the environment")
        .parse()
        .expect("application id is not a valid id");

    // Build our client.
    let mut client = Client::builder(token)
        .event_handler(Handler::new())
        .application_id(application_id)
        .await
        .expect("Error creating client");

    // Finally, start a single shard, and start listening to events.
    //
    // Shards will automatically attempt to reconnect, and will perform
    // exponential backoff until it reconnects.
    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }
}
