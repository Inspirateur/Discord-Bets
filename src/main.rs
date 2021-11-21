mod bets;
mod front;
mod handler;
mod utils;
use crate::front::is_readable;
use handler::{response, Handler};
use serenity::{
    async_trait,
    model::{
        gateway::Ready,
        id::GuildId,
        interactions::{application_command::ApplicationCommandOptionType, Interaction},
    },
    prelude::*,
};
use std::env;

#[async_trait]
impl EventHandler for Handler {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        // only answer if the bot has access to the channel
        match interaction {
            Interaction::ApplicationCommand(command) => {
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
            Interaction::MessageComponent(command) => self.button_clicked(ctx, command).await,
            _ => {}
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
