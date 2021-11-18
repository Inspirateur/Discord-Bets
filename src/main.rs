mod bets;
use serenity::{
    async_trait,
    model::{
        gateway::Ready,
        id::GuildId,
        interactions::{
            application_command::{
                ApplicationCommandInteractionDataOptionValue, ApplicationCommandOptionType,
            },
            Interaction, InteractionResponseType,
        },
    },
    prelude::*,
};
use shellwords::split;
use std::env;

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::ApplicationCommand(command) = interaction {
            let content = match command.data.name.as_str() {
                "make_account" => {
                    format!("We're in {}", command.channel_id)
                }
                "bet" => {
                    let desc = if let ApplicationCommandInteractionDataOptionValue::String(value) =
                        command
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
                    let outcomes = match split(
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
                    ) {
                        Ok(outcomes) => outcomes,
                        Err(why) => panic!("{}", why),
                    };
                    format!("{}\n{}", desc, outcomes.join("\n"))
                }
                "leaderboard" => "uuuh".to_string(),
                _ => "not implemented :(".to_string(),
            };

            if let Err(why) = command
                .create_interaction_response(&ctx.http, |response| {
                    response
                        .kind(InteractionResponseType::ChannelMessageWithSource)
                        .interaction_response_data(|message| message.content(content))
                })
                .await
            {
                println!("Cannot respond to slash command: {}", why);
            }
        }
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);

        let guild_id = GuildId(
            env::var("GUILD_ID")
                .expect("Expected GUILD_ID in environment")
                .parse()
                .expect("GUILD_ID must be an integer"),
        );

        let commands = GuildId::set_application_commands(&guild_id, &ctx.http, |commands| {
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
        .await;

        println!(
            "I now have the following guild slash commands: {:#?}",
            commands
        );
    }
}

#[tokio::main]
async fn main() {
    // Configure the client with your Discord bot token in the environment.
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");

    // The Application Id is usually the Bot User Id.
    let application_id: u64 = env::var("APPLICATION_ID")
        .expect("Expected an application id in the environment")
        .parse()
        .expect("application id is not a valid id");

    // Build our client.
    let mut client = Client::builder(token)
        .event_handler(Handler)
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
