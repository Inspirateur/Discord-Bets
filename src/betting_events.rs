use anyhow::anyhow;
use log::warn;
use serenity::{
    all::{CreateInteractionResponse, CreateInteractionResponseMessage}, async_trait, model::{
        application::Interaction, gateway::Ready, guild::Guild, id::GuildId
    }, prelude::*
};
use crate::{betting_bot::BettingBot, serialize_utils::BetAction};

#[async_trait]
impl EventHandler for BettingBot {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        let can_view_channel = interaction.app_permissions().is_some_and(|p| p.view_channel());
        match interaction {
            Interaction::Command(command) => {
                let command_name = command.data.name.to_string();
                // only answer if the bot has access to the channel
                if can_view_channel {
                    if let Err(why) = match command_name.as_str() {
                        "account" => self.account_command(ctx, command).await,
                        "bet" => self.bet_command(ctx, command).await,
                        "leaderboard" => self.leaderboard_command(ctx, command).await,
                        _ => Err(anyhow!("Unknown command")),
                    } {
                        warn!(target: "betting-bot", "\\{}: {}", command_name, why);
                    }
                } else {
                    if let Err(why) = command.create_response(
                        &ctx.http, CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new()
                                .content("Sorry, I only answer to commands in the channels that I can read.")
                                .ephemeral(true)
                        )).await 
                    {
                        warn!(target: "betting-bot", "\\{} in non writable channel: {}", command_name, why);
                    }
                }
            }
            Interaction::Component(command) => if let Err(why) = match BetAction::try_from(command.data.custom_id.clone()) {
                Ok(BetAction::Lock) => self.lock_action(ctx, &command, command.message.id.get()).await,
                Ok(BetAction::Abort) => self.abort_action(ctx, &command, command.message.id.get()).await,
                Ok(BetAction::BetClick(bet_outcome)) => self.bet_click_action(ctx, &command, bet_outcome).await,
                Ok(BetAction::Resolve(bet_outcome)) => self.resolve_action(ctx, &command, bet_outcome).await,
                Err(why) => Err(why),
                other => Err(anyhow!("Unhandled BetAction variant {:?}", other))
            } {
                warn!(target: "betting-bot", "Component '{}': {}", command.data.custom_id, why);
            },
            Interaction::Modal(command) => if let Err(why) = match BetAction::try_from(command.data.custom_id.clone()) {
                Ok(BetAction::BetOrder) => self.bet_order_action(ctx, &command).await,
                Err(why) => Err(why),
                other => Err(anyhow!("Unhandled BetAction variant {:?}", other))
            } {
                warn!(target: "betting-bot", "Modal '{}': {}", command.data.custom_id, why);
            }
            _ => {}
        }
    }

    async fn ready(&self, _ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }

    async fn cache_ready(&self, _ctx: Context, _guilds: Vec<GuildId>) {
        println!("Cache built successfully!");
    }

    async fn guild_create(&self, ctx: Context, guild: Guild, _is_new: Option<bool>) {
        self.register_commands(&ctx.http, guild.id).await;
    }
}