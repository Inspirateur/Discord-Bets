mod bets;
mod front;
mod handler;
mod handler_utils;
mod utils;
use front::is_readable;
use handler::{passive_income, response, Handler};
use serenity::{
    async_trait,
    http::Http,
    model::{
        gateway::Ready,
        guild::Guild,
        id::GuildId,
        gateway::GatewayIntents,
        application::interaction::Interaction,
    },
    prelude::*,
};
use std::{
    env,
    sync::{atomic::Ordering, Arc},
    time::Duration,
};
pub const CURRENCY: &str = "💵";
pub const STARTING_COINS: u32 = 300;
pub const INCOME: u32 = 5;
pub const INTERVAL: u64 = 3;

#[async_trait]
impl EventHandler for Handler {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        match interaction {
            Interaction::ApplicationCommand(command) => {
                // only answer if the bot has access to the channel
                if is_readable(&ctx, command.channel_id).await {
                    match command.data.name.as_str() {
                        "make_account" => self.make_account(ctx, command).await,
                        "bet" => self.bet(ctx, command).await,
                        "leaderboard" => self.leadeboard(ctx, command).await,
                        "reset" => self.reset(ctx, command).await,
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

    async fn ready(&self, _ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }

    async fn cache_ready(&self, ctx: Context, _guilds: Vec<GuildId>) {
        println!("Cache built successfully!");
        let ctx = Arc::new(ctx);
        let bets = Arc::new(self.bets.clone());
        let front = Arc::new(self.front.clone());
        if !self.is_loop_running.load(Ordering::Relaxed) {
            let ctx1 = Arc::clone(&ctx);
            let bets1 = Arc::clone(&bets);
            let front1 = Arc::clone(&front);
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(3600 * INTERVAL)).await;
                    passive_income(Arc::clone(&ctx1), Arc::clone(&bets1), Arc::clone(&front1))
                        .await;
                }
            });
            self.is_loop_running.swap(true, Ordering::Relaxed);
        }
    }

    async fn guild_create(&self, ctx: Context, guild: Guild, _is_new: bool) {
        self.register_guild(&ctx.http, guild.id).await;
    }
}

#[tokio::main]
async fn main() {
    // Configure the client with your Discord bot token in the environment.
    let token = env::var("GOTOH_TOKEN").expect("Expected a token in the environment");

    let http = Http::new(&token);

    // The Application Id is usually the Bot User Id.
    let bot_id = match http.get_current_application_info().await {
        Ok(info) => info.id,
        Err(why) => panic!("Could not access application info: {:?}", why),
    };
    // Build our client.
    let mut client = Client::builder(
        token, GatewayIntents::non_privileged()
        | GatewayIntents::GUILD_MEMBERS
        | GatewayIntents::GUILD_PRESENCES
    )
        .event_handler(Handler::new())
        .application_id(bot_id.into())
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
