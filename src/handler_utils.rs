use serenity::builder::CreateComponents;
use serenity::model::application::component::ButtonStyle;
use crate::config::config;
pub const LOCK: &str = "lock";
pub const ABORT: &str = "abort";
pub const WIN: &str = "win";
pub const OPEN: &str = "open";
pub const CANCEL: &str = "cancel";
pub const RESET: &str = "reset";
pub const BET: &str = "bet";

pub fn bet_components<'a>(
    components: &'a mut CreateComponents,
    status: &str,
) -> &'a mut CreateComponents {
    if status == WIN || status == ABORT {
        return components;
    }
    components.create_action_row(|action_row| {
        if status != LOCK {
            action_row.create_button(|button| {
                button
                    .custom_id(LOCK)
                    .style(ButtonStyle::Primary)
                    .label("Lock")
            });
        }
        action_row.create_button(|button| {
            button
                .custom_id(ABORT)
                .style(ButtonStyle::Danger)
                .label("Abort")
        })
    })
}

pub fn outcome_components<'a>(
    components: &'a mut CreateComponents,
    status: &str,
) -> &'a mut CreateComponents {
    if status == WIN || status == ABORT {
        return components;
    }
    components.create_action_row(|action_row| {
        if status == OPEN {
            action_row.create_button(|button| {
                button
                    .custom_id(BET)
                    .style(ButtonStyle::Secondary)
                    .label("Bet")
            });
        } else if status == LOCK {
            action_row.create_button(|button| {
                button
                    .custom_id(WIN)
                    .style(ButtonStyle::Success)
                    .label("🏆")
            });
        }
        action_row
    })
}
