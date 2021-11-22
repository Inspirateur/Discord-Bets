use serenity::builder::CreateComponents;
use serenity::model::interactions::message_component::ButtonStyle;
pub const BET_OPTS: [u32; 3] = [10, 50, 100];
pub const LOCK: &str = "lock";
pub const ABORT: &str = "abort";
pub const WIN: &str = "win";
pub const OPEN: &str = "open";

fn bet_opts_display(percent: u32) -> String {
    match percent {
        100 => "All in".to_string(),
        _ => format!("{} %", percent),
    }
}

pub fn bet_components<'a>(
    components: &'a mut CreateComponents,
    status: &str,
) -> &'a mut CreateComponents {
    components.create_action_row(|action_row| {
        action_row
            .create_button(|button| {
                button
                    .custom_id(LOCK)
                    .style(ButtonStyle::Primary)
                    .label("Lock")
                    .disabled(status == LOCK || status == WIN)
            })
            .create_button(|button| {
                button
                    .custom_id(ABORT)
                    .style(ButtonStyle::Danger)
                    .label("Abort")
                    .disabled(status == WIN)
            })
    })
}

pub fn option_components<'a>(
    components: &'a mut CreateComponents,
    status: &str,
) -> &'a mut CreateComponents {
    components.create_action_row(|action_row| {
        for (i, percent) in BET_OPTS.into_iter().enumerate() {
            action_row.create_button(|button| {
                button
                    .custom_id(i)
                    .style(ButtonStyle::Secondary)
                    .label(bet_opts_display(percent))
                    .disabled(status == LOCK || status == WIN)
            });
        }
        action_row.create_button(|button| {
            button
                .custom_id(WIN)
                .style(ButtonStyle::Success)
                .disabled(status != LOCK)
                .label("🏆")
        })
    })
}
