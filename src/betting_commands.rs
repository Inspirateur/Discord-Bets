use anyhow::{Result, bail, Ok, anyhow};
use chrono::prelude::*;
use itertools::Itertools;
use serenity::{
    all::{
        CommandInteraction, CommandOptionType, CreateActionRow, CreateButton, CreateCommand, CreateCommandOption, CreateInputText, CreateInteractionResponse, CreateInteractionResponseMessage, CreateMessage, CreateModal, EditMessage
    }, 
    http::Http, model::{
        application::{
            ActionRowComponent, ButtonStyle, ComponentInteraction, 
            InputTextStyle, ModalInteraction
        }, 
        prelude::{CommandDataOptionValue, GuildId}
    }, prelude::*
};
use shellwords::split;
use crate::{betting_bot::BettingBot, config::config, serialize_utils::{BetOutcome, BetAction}, front_utils::{shorten, outcomes_display, bet_stub}};

impl BettingBot {
    pub async fn account_command(&self, ctx: Context, command: CommandInteraction) -> Result<()> {
        let server_uuid = command.guild_id.ok_or(anyhow!("command used outside a server"))?.get();
        let user_uuid = command.user.id.get();
        let account: betting::AccountStatus = self.account_create(server_uuid, user_uuid)?;
        command.create_response(
            &ctx.http, CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(format!("Balance: {} {} | In bet: {} {}", 
                        account.balance, config.currency, account.in_bet, config.currency
                    ))
                    .ephemeral(true)
            )
        ).await?;
        Ok(())
    }

    fn bet_parse(
        command: &CommandInteraction,
    ) -> Result<(String, Vec<String>)> {
        let desc = if let CommandDataOptionValue::String(value) = command
            .data
            .options
            .get(0)
            .expect("Expected a description of the bet")
            .value.clone()
        {
            value.clone()
        } else {
            String::new()
        };
        let outcomes_raw = if let CommandDataOptionValue::String(value) = command
            .data
            .options
            .get(1)
            .expect("Expected outcomes for the bet")
            .value.clone() {
            value
        } else {
            String::new()
        };
        let outcomes = split(&outcomes_raw)?;
        Ok((desc, outcomes))
    }

    pub async fn bet_command(
        &self,
        ctx: Context,
        command: CommandInteraction,
    ) -> Result<()> {
        let server_uuid = command.guild_id.ok_or(anyhow!("command used outside a server"))?;
        let (desc, outcomes) = Self::bet_parse(&command)?;
        if outcomes.len() < 2 {
            command.create_response(
                &ctx.http,
                CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                    .content("You must define 2 outcomes or more to create a bet.")
                ),
            )
            .await?;
            bail!("Less than 2 ouctomes");
        }
        command.create_response(
            &ctx.http, 
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                .content(&desc)
                .components(vec![CreateActionRow::Buttons(vec![
                    CreateButton::new(BetAction::Lock).label("🔒 Lock").style(ButtonStyle::Secondary),
                    CreateButton::new(BetAction::Abort).label("🚫 Abort").style(ButtonStyle::Secondary),
                ])])
        )).await?;
        let bet_msg = command.get_response(&ctx.http).await?;
        let bet_uuid = bet_msg.id.get();
        self.bets.create_bet(bet_uuid, server_uuid.get(), command.user.id.get(), desc, &outcomes)?;
        let outcome_displays = outcomes_display(&bet_stub(&outcomes));
        for (i, outcome) in outcome_displays.iter().enumerate() {
            let outcome_msg = command.channel_id.send_message(&ctx.http, 
                CreateMessage::new().content(outcome)
                .components(vec![CreateActionRow::Buttons(vec![
                    CreateButton::new(BetAction::BetClick(BetOutcome { bet_id: bet_uuid, outcome_id: i }))
                        .label(format!("{} Bet", config.currency))
                        .style(ButtonStyle::Primary)
                ])])
            ).await?;
            self.msg_map.insert(BetOutcome {bet_id: bet_uuid, outcome_id: i}, outcome_msg.id.get())?;
        }
        Ok(())
    }

    pub async fn leaderboard_command(
        &self,
        ctx: Context,
        command: CommandInteraction,
    ) -> Result<()> {
        let guild_id = command.guild_id.ok_or(anyhow!("command used outside a server"))?;
        let mut accounts = self.bets.accounts(guild_id.get())?;
        // sort by balance+inbet first and balance to tie break
        accounts.sort_by_key(|acc| (acc.balance+acc.in_bet, acc.balance));
        accounts.reverse();
        let msg = format!("{}  ({} in bet)   user\n", config.currency, config.currency) 
        + &accounts.into_iter().take(10).map(|acc| 
            format!("{}  ({})   <@{}>", acc.balance, acc.in_bet, acc.user)
        ).join("\n") + "\n...";
        command.create_response(&ctx.http, 
            CreateInteractionResponse::Message(CreateInteractionResponseMessage::new().content(msg))
        ).await?;
        Ok(())
    }

    async fn is_admin(&self, command: &ComponentInteraction) -> Result<bool> {
        if let Some(member) = &command.user.member {
            let permissions = member.permissions.ok_or(anyhow!("couldn't get permissions"))?;
            return Ok(permissions.administrator());
        }
        bail!("couldn't get member");
    }

    pub async fn check_rights(&self, ctx: &Context, command: &ComponentInteraction, bet_id: u64) -> Result<()> {
        let user_uuid = command.user.id.get();
        let info = self.bets.get_info(bet_id)?;
        if info.author != user_uuid && !self.is_admin(command).await? {
            command.create_response(
                &ctx.http, 
                CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .content("Only the bet author or admins can perform this action")
                        .ephemeral(true)
                    )
            ).await?;
            bail!("user is not bet author and not admin");
        }
        Ok(())
    }
    
    pub async fn lock_action(&self, ctx: Context, command: &ComponentInteraction, bet_id: u64) -> Result<()> {
        self.check_rights(&ctx, command, bet_id).await?;
        self.bets.lock_bet(bet_id)?;
        command.create_response(
            &ctx.http,
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new().components(vec![CreateActionRow::Buttons(vec![
                    CreateButton::new(BetAction::Abort).label("🚫 Abort").style(ButtonStyle::Secondary)
                ])])
            )
        ).await?;
        for outcome_id in self.bets.outcomes_of_bet(bet_id)? {
            let outcome = BetOutcome { bet_id, outcome_id: outcome_id as usize };
            let msg_id = self.msg_map.get(outcome.clone())?;
            let mut message = ctx.http.get_message(command.channel_id, msg_id.into()).await?;
            message.edit(&ctx.http, 
                EditMessage::new().components(vec![CreateActionRow::Buttons(vec![
                    CreateButton::new(BetAction::Resolve(outcome))
                        .label("🏆 Resolve")
                        .style(ButtonStyle::Secondary)
                ])])
            ).await?;
        }
        Ok(())
    }

    pub async fn abort_action(&self, ctx: Context, command: &ComponentInteraction, bet_id: u64) -> Result<()> {
        self.check_rights(&ctx, command, bet_id).await?;
        self.bets.abort_bet(bet_id)?;
        command.create_response(
            &ctx.http, 
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new().content("*Bet aborted, participants have been refunded.*")
            )
        ).await?;
        for outcome_id in self.bets.outcomes_of_bet(bet_id)? {
            let outcome = BetOutcome { bet_id, outcome_id: outcome_id as usize };
            let msg_id = self.msg_map.get(outcome.clone())?;
            ctx.http.delete_message(command.channel_id, msg_id.into(), None).await?;
        }
        Ok(())
    }

    pub async fn bet_click_action(&self, ctx: Context, command: &ComponentInteraction, bet_outcome: BetOutcome) -> Result<()> {
        let server_uuid = command.guild_id.ok_or(anyhow!("action triggered outside server"))?.get();
        let user_uuid = command.user.id.get();
        let balance = self.balance_create(server_uuid, user_uuid)?;
        let bet_info = self.bets.get_info(bet_outcome.bet_id)?;
        let previous_bet = match self.bets.position(user_uuid, bet_outcome.bet_id) {
            Result::Ok(position) => {
                if position.outcome != bet_outcome.outcome_id {
                    command.create_response(
                        &ctx.http, 
                CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new().content(
                                format!("You put a bet on option #{} and can only bet on one option", position.outcome+1)
                            )
                            .ephemeral(true)
                        )
                    ).await?;
                    bail!("user tried to bet on multiple option");        
                }
                position.amount
            },
            Err(betting::BetError::NotFound) => 0,
            Err(err) => bail!(err)
        };
        command.create_response(
            &ctx.http, 
            CreateInteractionResponse::Modal(
                CreateModal::new(
                    BetAction::BetOrder, 
                    format!("[{} {}] {}", balance, config.currency, shorten(&bet_info.desc, 20))
                ).components(vec![
                    CreateActionRow::InputText(
                        CreateInputText::new(
                            InputTextStyle::Short, 
                            format!(
                                "[{} {}] Bet on: {}", previous_bet, config.currency, 
                                shorten(&command.message.content, 20)
                            ),
                            bet_outcome.to_string()
                        ).placeholder("100").required(true)
                    )
                ])
            )).await?;
        Ok(())
    }

    pub async fn bet_order_action(&self, ctx: Context, command: &ModalInteraction) -> Result<()> {
        let user = command.user.id.get();
        if let ActionRowComponent::InputText(input) = &(&command.data.components[0]).components[0] {
            let bet_outcome = BetOutcome::try_from(input.custom_id.as_ref())?;
            let amount: u64 = <Option<String> as Clone>::clone(&input.value).unwrap().parse()?;
            let (acc_update, bet) = self.bets.bet_on(bet_outcome.bet_id, bet_outcome.outcome_id, user, amount)?;
            let total: u64 = bet.outcomes[bet_outcome.outcome_id].wagers
                .iter().filter(|(u, _)| *u == user).map(|(_, a)| a).sum();
            command.create_response(
                &ctx.http, 
                CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .content(format!(
                            "Succesfully bet {} {} (total {} {}) on:\n> {}\nnew balance: {} {}", 
                            amount, config.currency, total, config.currency, bet.outcomes[bet_outcome.outcome_id].desc, acc_update.balance, config.currency
                        ))
                        .ephemeral(true)
                )
            ).await?;
            for (i, outcome) in outcomes_display(&bet).iter().enumerate() {
                let msg_id = self.msg_map.get(BetOutcome { bet_id: bet_outcome.bet_id, outcome_id: i })?;
                let mut msg = ctx.http.get_message(command.channel_id, msg_id.into()).await?;
                msg.edit(&ctx.http, EditMessage::new().content(outcome)).await?;
            }
        }
        Ok(())
    }

    pub async fn resolve_action(&self, ctx: Context, command: &ComponentInteraction, bet_outcome: BetOutcome) -> Result<()> {
        self.check_rights(&ctx, command, bet_outcome.bet_id).await?;
        self.bets.resolve(bet_outcome.bet_id, bet_outcome.outcome_id)?;

        command.create_response(
            &ctx.http, 
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(format!("🏆 Winner\n{}", command.message.content.clone()))
            )
        ).await?;

        let mut bet_msg = ctx.http.get_message(command.channel_id, bet_outcome.bet_id.into()).await?;
        let bet_msg_content = bet_msg.content.clone();
        bet_msg.edit(&ctx.http, 
            EditMessage::new()
                .content(format!("*Resolved {}*\n{}", Local::now().format("%d/%m/%Y"), bet_msg_content))
                .components(vec![])
        ).await?;
        for outcome_id in self.bets.outcomes_of_bet(bet_outcome.bet_id)? {
            let msg_id = self.msg_map.get(BetOutcome { bet_id: bet_outcome.bet_id, outcome_id: outcome_id as usize })?;
            let mut message = ctx.http.get_message(command.channel_id, msg_id.into()).await?;
            message.edit(&ctx.http, EditMessage::new().components(vec![])).await?;
        }
        Ok(())
    }

    pub async fn register_commands(&self, http: &Http, id: GuildId) {
        println!("Registering slash commands for Guild {}", id);
        if let Err(why) =
            id.set_commands(http, vec![
                CreateCommand::new("account").description("Check how much you have in your account."),
                CreateCommand::new("bet")
                    .description("Create a bet.")
                    .add_option(CreateCommandOption::new(
                        CommandOptionType::String, 
                        "desc", 
                        "The description of the bet"
                    ).required(true))
                    .add_option(CreateCommandOption::new(
                        CommandOptionType::String, 
                        "options", 
                        "The possible outcomes of the bet"
                    ).required(true)),
                CreateCommand::new("leaderboard")    
                    .description("Displays the leadeboard.")
                    .add_option(CreateCommandOption::new(
                        CommandOptionType::Boolean, 
                        "permanent", 
                        "To make a ever updating leaderboard").required(false)
                    )
            ]
        ).await
        {
            println!("Couldn't register slash commmands: {}", why);
        };
    }
}