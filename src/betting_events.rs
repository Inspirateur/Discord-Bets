use std::{
    sync::{atomic::Ordering, Arc},
    time::Duration,
};
use anyhow::anyhow;
use log::{warn, info};
use serenity::{
    async_trait,
    model::{
        gateway::Ready,
        guild::Guild,
        id::GuildId,
        application::interaction::{Interaction, InteractionResponseType}
    },
    prelude::*,
};
use serenity_utils::{is_writable, MessageBuilder, CommandUtil};
use crate::{config::config, betting_bot::BettingBot, serialize_utils::BetAction};

#[async_trait]
impl EventHandler for BettingBot {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        match interaction {
            Interaction::ApplicationCommand(command) => {
                let command_name = command.data.name.to_string();
                // only answer if the bot has access to the channel
                if is_writable(&ctx, command.channel_id).await {
                    if let Err(why) = match command_name.as_str() {
                        "account" => self.account_command(ctx, command).await,
                        "bet" => self.bet_command(ctx, command).await,
                        "leaderboard" => self.leaderboard_command(ctx, command).await,
                        _ => Err(anyhow!("Unknown command")),
                    } {
                        warn!(target: "betting-bot", "\\{}: {:?}", command_name, why);
                    }
                } else {
                    if let Err(why) = command.response(&ctx.http, MessageBuilder::new(
                        "Sorry, I only answer to commands in the channels that I can read."
                    ), InteractionResponseType::ChannelMessageWithSource).await {
                        warn!(target: "betting-bot", "\\{} in non writable channel: {:?}", command_name, why);
                    }
                }
            }
            Interaction::MessageComponent(command) => if let Err(why) = match BetAction::try_from(command.data.custom_id.clone()) {
                Ok(BetAction::Lock()) => self.lock_action(ctx, &command, command.message.id.0).await,
                Ok(BetAction::Abort()) => self.abort_action(ctx, &command, command.message.id.0).await,
                Ok(BetAction::BetClick(bet_outcome)) => self.bet_click_action(ctx, &command, bet_outcome).await,
                Ok(BetAction::Resolve(bet_outcome)) => self.resolve_action(ctx, &command, bet_outcome).await,
                Err(why) => Err(why),
                other => Err(anyhow!("Unhandled BetAction variant {:?}", other))
            } {
                warn!(target: "betting-bot", "MessageComponent: {} action: {:?}", command.data.custom_id, why);
            },
            Interaction::ModalSubmit(command) => if let Err(why) = match BetAction::try_from(command.data.custom_id.clone()) {
                Ok(BetAction::BetOrder()) => self.bet_order_action(ctx, &command).await,
                Err(why) => Err(why),
                other => Err(anyhow!("Unhandled BetAction variant {:?}", other))
            } {
                warn!(target: "betting-bot", "ModalSubmit: {} action: {:?}", command.data.custom_id, why);
            }
            _ => {}
        }
    }

    async fn ready(&self, _ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }

    async fn cache_ready(&self, _ctx: Context, _guilds: Vec<GuildId>) {
        println!("Cache built successfully!");
        let bets = Arc::new(self.bets.clone());
        if !self.is_loop_running.load(Ordering::Relaxed) {
            let bets1 = Arc::clone(&bets);
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(3600 * config.interval)).await;
                    if let Err(why) = bets1.global_income(config.income as u64) {
                        warn!(target: "betting-bot", "couldn't distribute global income: {}", why)
                    } else {
                        info!(target: "betting-bot", "distributed global income of {}", config.income)
                    }
                }
            });
            self.is_loop_running.swap(true, Ordering::Relaxed);
        }
    }

    async fn guild_create(&self, ctx: Context, guild: Guild, _is_new: bool) {
        self.register_commands(&ctx.http, guild.id).await;
    }
}