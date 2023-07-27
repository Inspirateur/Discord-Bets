use anyhow::{Result, bail, Ok, anyhow};
use chrono::prelude::*;
use itertools::Itertools;
use serenity::{
    prelude::*, 
    model::{
        application::{
            component::{ButtonStyle, InputTextStyle, ActionRowComponent}, 
            interaction::{
                InteractionResponseType, application_command::ApplicationCommandInteraction, 
                message_component::MessageComponentInteraction, modal::ModalSubmitInteraction
            }
        }, 
        prelude::{GuildId, command::CommandOptionType, application_command::CommandDataOptionValue}
    }, http::Http, builder::CreateComponents
};
use serenity_utils::{BotUtil, MessageBuilder, Button, CommandUtil, MessageUtil};
use shellwords::split;
use crate::{betting_bot::BettingBot, config::config, serialize_utils::{BetOutcome, BetAction}, front_utils::{shorten, outcomes_display, bet_stub}};

impl BettingBot {
    pub async fn account_command(&self, ctx: Context, command: ApplicationCommandInteraction) -> Result<()> {
        let server_uuid = command.guild_id.ok_or(anyhow!("command used outside a server"))?.0;
        let user_uuid = command.user.id.0;
        let account = self.account_create(server_uuid, user_uuid)?;
        command.response(
            &ctx.http, MessageBuilder::new(format!(
            "Balance: {} {} | In bet: {} {}", account.balance, config.currency, account.in_bet, config.currency
            )).ephemeral(true), InteractionResponseType::ChannelMessageWithSource
        ).await?;
        Ok(())
    }

    fn bet_parse(
        command: &ApplicationCommandInteraction,
    ) -> Result<(String, Vec<String>)> {
        let desc = if let CommandDataOptionValue::String(value) = command
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
            if let CommandDataOptionValue::String(value) = command
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

    pub async fn bet_command(
        &self,
        ctx: Context,
        command: ApplicationCommandInteraction,
    ) -> Result<()> {
        let server_uuid = command.guild_id.ok_or(anyhow!("command used outside a server"))?;
        let (desc, outcomes) = Self::bet_parse(&command)?;
        if outcomes.len() < 2 {
            command.response(
                &ctx.http,
                MessageBuilder::new("You must define 2 outcomes or more to create a bet."),
                InteractionResponseType::ChannelMessageWithSource
            )
            .await?;
            bail!("Less than 2 ouctomes");
        }
        let bet_msg = command.response(
            &ctx.http, 
            MessageBuilder::new(&desc).buttons(vec![
                Button { custom_id: BetAction::Lock().to_string(), label: "🔒 Lock".to_string(), style: ButtonStyle::Secondary },
                Button { custom_id: BetAction::Abort().to_string(), label: "🚫 Abort".to_string(), style: ButtonStyle::Secondary }
            ]),
            InteractionResponseType::ChannelMessageWithSource
        ).await?;
        let bet_uuid = bet_msg.id.0;
        self.bets.create_bet(bet_uuid, server_uuid.0, command.user.id.0, desc, &outcomes)?;
        let outcome_displays = outcomes_display(&bet_stub(&outcomes));
        for (i, outcome) in outcome_displays.iter().enumerate() {
            let outcome_msg = ctx.http.send(bet_msg.channel_id, MessageBuilder::new(outcome).buttons(vec![
                Button { 
                    custom_id: BetAction::BetClick(BetOutcome { bet_id: bet_uuid, outcome_id: i }).to_string(), 
                    label: format!("{} Bet", config.currency), style: ButtonStyle::Primary 
                }
            ])).await?;
            self.msg_map.insert(BetOutcome {bet_id: bet_uuid, outcome_id: i}, outcome_msg.id.0)?;
        }
        Ok(())
    }

    pub async fn leaderboard_command(
        &self,
        ctx: Context,
        command: ApplicationCommandInteraction,
    ) -> Result<()> {
        let guild_id = command.guild_id.ok_or(anyhow!("command used outside a server"))?;
        let mut accounts = self.bets.accounts(guild_id.0)?;
        // sort by balance+inbet first and balance to tie break
        accounts.sort_by_key(|acc| (acc.balance+acc.in_bet, acc.balance));
        accounts.reverse();
        let msg = format!("{}  ({} in bet)   user\n", config.currency, config.currency) 
        + &accounts.into_iter().take(10).map(|acc| 
            format!("{}  ({})   <@{}>", acc.balance, acc.in_bet, acc.user)
        ).join("\n") + "\n...";
        command.response(&ctx.http, MessageBuilder::new(msg), InteractionResponseType::ChannelMessageWithSource).await?;
        Ok(())
    }

    async fn is_admin(&self, command: &MessageComponentInteraction) -> Result<bool> {
        if let Some(member) = &command.user.member {
            let permissions = member.permissions.ok_or(anyhow!("couldn't get permissions"))?;
            return Ok(permissions.administrator());
        }
        bail!("couldn't get member");
    }

    pub async fn check_rights(&self, ctx: &Context, command: &MessageComponentInteraction, bet_id: u64) -> Result<()> {
        let user_uuid = command.user.id.0;
        let info = self.bets.get_info(bet_id)?;
        if info.author != user_uuid && !self.is_admin(command).await? {
            command.response(
                &ctx.http, 
                MessageBuilder::new("Only the bet author or admins can perform this action").ephemeral(true),
                InteractionResponseType::ChannelMessageWithSource
            ).await?;
            bail!("user is not bet author and not admin");
        }
        Ok(())
    }
    
    pub async fn lock_action(&self, ctx: Context, command: &MessageComponentInteraction, bet_id: u64) -> Result<()> {
        self.check_rights(&ctx, command, bet_id).await?;
        self.bets.lock_bet(bet_id)?;
        command.response(
            &ctx.http, 
            MessageBuilder::new(command.message.content.clone()).buttons(vec![
                Button { custom_id: BetAction::Abort().to_string(), label: "🚫 Abort".to_string(), style: ButtonStyle::Secondary }
            ]), 
            InteractionResponseType::UpdateMessage
        ).await?;
        for outcome_id in self.bets.outcomes_of_bet(bet_id)? {
            let outcome = BetOutcome { bet_id, outcome_id: outcome_id as usize };
            let msg_id = self.msg_map.get(outcome.clone())?;
            let mut message = ctx.http.get_message(command.channel_id.0, msg_id).await?;
            message.edit(&ctx.http, |msg| msg
                .set_buttons(vec![Button {
                    custom_id: BetAction::Resolve(outcome).to_string(), 
                    label: "🏆 Resolve".to_string(), 
                    style: ButtonStyle::Secondary
                }])
            ).await?;
        }
        Ok(())
    }

    pub async fn abort_action(&self, ctx: Context, command: &MessageComponentInteraction, bet_id: u64) -> Result<()> {
        self.check_rights(&ctx, command, bet_id).await?;
        self.bets.abort_bet(bet_id)?;
        command.response(
            &ctx.http, 
            MessageBuilder::new("*Bet aborted, participants have been refunded.*"), 
            InteractionResponseType::UpdateMessage
        ).await?;
        for outcome_id in self.bets.outcomes_of_bet(bet_id)? {
            let outcome = BetOutcome { bet_id, outcome_id: outcome_id as usize };
            let msg_id = self.msg_map.get(outcome.clone())?;
            ctx.http.delete_message(command.channel_id.0, msg_id).await?;
        }
        Ok(())
    }

    pub async fn bet_click_action(&self, ctx: Context, command: &MessageComponentInteraction, bet_outcome: BetOutcome) -> Result<()> {
        let server_uuid = command.guild_id.ok_or(anyhow!("action triggered outside server"))?.0;
        let user_uuid = command.user.id.0;
        let balance = self.balance_create(server_uuid, user_uuid)?;
        let bet_info = self.bets.get_info(bet_outcome.bet_id)?;
        let previous_bet = match self.bets.position(user_uuid, bet_outcome.bet_id) {
            Result::Ok(position) => {
                if position.outcome != bet_outcome.outcome_id {
                    command.response(
                        &ctx.http, 
                        MessageBuilder::new(
                            format!("You put a bet on option #{} and can only bet on one option", position.outcome+1)
                        ).ephemeral(true),
                        InteractionResponseType::ChannelMessageWithSource
                    ).await?;
                    bail!("user tried to bet on multiple option");        
                }
                position.amount
            },
            Err(betting::BetError::NotFound) => 0,
            Err(err) => bail!(err)
        };
        command.create_interaction_response(
            &ctx.http, |response| 
            response.kind(InteractionResponseType::Modal).interaction_response_data(|modal|
                modal.custom_id(BetAction::BetOrder().to_string())
                    .title(format!("[{} {}] {}", balance, config.currency, shorten(&bet_info.desc, 20)))
                    .components(|act_row| {
                        act_row.create_action_row(|field| field.create_input_text(|input| {
                            input.custom_id(bet_outcome.to_string())
                                .style(InputTextStyle::Short)
                                .label(format!(
                                    "[{} {}] Bet on: {}", previous_bet, config.currency, 
                                    shorten(&command.message.content, 20)
                                ))
                                .placeholder("100")
                                .required(true)
                        }))
                    })
            )).await?;
        Ok(())
    }

    pub async fn bet_order_action(&self, ctx: Context, command: &ModalSubmitInteraction) -> Result<()> {
        let user = command.user.id.0;
        if let ActionRowComponent::InputText(input) = &(&command.data.components[0]).components[0] {
            let bet_outcome = BetOutcome::try_from(input.custom_id.as_ref())?;
            let amount: u64 = input.value.parse()?;
            let (acc_update, bet) = self.bets.bet_on(bet_outcome.bet_id, bet_outcome.outcome_id, user, amount)?;
            let total: u64 = bet.outcomes[bet_outcome.outcome_id].wagers
                .iter().filter(|(u, _)| *u == user).map(|(_, a)| a).sum();
            command.response(
                &ctx.http, 
                MessageBuilder::new(format!(
                    "Succesfully bet {} {} (total {} {}) on:\n> {}\nnew balance: {} {}", 
                    amount, config.currency, total, config.currency, bet.outcomes[bet_outcome.outcome_id].desc, acc_update.balance, config.currency
                )).ephemeral(true),
                InteractionResponseType::ChannelMessageWithSource
            ).await?;
            for (i, outcome) in outcomes_display(&bet).iter().enumerate() {
                let msg_id = self.msg_map.get(BetOutcome { bet_id: bet_outcome.bet_id, outcome_id: i })?;
                let mut msg = ctx.http.get_message(command.channel_id.0, msg_id).await?;
                msg.edit(&ctx.http, |msg| msg.content(outcome)).await?;
            }
        }
        Ok(())
    }

    pub async fn resolve_action(&self, ctx: Context, command: &MessageComponentInteraction, bet_outcome: BetOutcome) -> Result<()> {
        self.check_rights(&ctx, command, bet_outcome.bet_id).await?;
        self.bets.resolve(bet_outcome.bet_id, bet_outcome.outcome_id)?;

        command.response(
            &ctx.http, 
            MessageBuilder::new(format!("🏆 Winner\n{}", command.message.content.clone())), 
            InteractionResponseType::UpdateMessage
        ).await?;

        let mut bet_msg = ctx.http.get_message(command.channel_id.0, bet_outcome.bet_id).await?;
        let bet_msg_content = bet_msg.content.clone();
        bet_msg.edit(&ctx.http, |msg| 
            msg.content(format!("*Resolved {}*\n{}", Local::now().format("%d/%m/%Y"), bet_msg_content))
            .set_components(CreateComponents::default())
        ).await?;
        for outcome_id in self.bets.outcomes_of_bet(bet_outcome.bet_id)? {
            let msg_id = self.msg_map.get(BetOutcome { bet_id: bet_outcome.bet_id, outcome_id: outcome_id as usize })?;
            let mut message = ctx.http.get_message(command.channel_id.0, msg_id).await?;
            message.edit(&ctx.http, |msg| msg
                .set_components(CreateComponents::default())
            ).await?;
        }
        Ok(())
    }

    pub async fn register_commands(&self, http: &Http, id: GuildId) {
        println!("Registering slash commands for Guild {}", id);
        if let Err(why) =
            GuildId::set_application_commands(&id, http, |commands| {
                commands
                    .create_application_command(|command| {
                        command.name("account").description("Check how much you have in your account.")
                    })
                    .create_application_command(|command| {
                        command
                            .name("bet")
                            .description("Create a bet.")
                            .create_option(|option| {
                                option
                                    .name("desc")
                                    .description("The description of the bet")
                                    .kind(CommandOptionType::String)
                                    .required(true)
                            })
                            .create_option(|option| {
                                option
                                    .name("options")
                                    .description("The possible outcomes of the bet")
                                    .kind(CommandOptionType::String)
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
                                    .kind(CommandOptionType::Boolean)
                                    .required(false)
                            })
                    })
            })
            .await
        {
            println!("Couldn't register slash commmands: {}", why);
        };
    }
}