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
    if status == WIN {
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

pub fn option_components<'a>(
    components: &'a mut CreateComponents,
    status: &str,
) -> &'a mut CreateComponents {
    if status == WIN {
        return components;
    }
    components.create_action_row(|action_row| {
        if status == OPEN {
            for (i, percent) in BET_OPTS.into_iter().enumerate() {
                action_row.create_button(|button| {
                    button
                        .custom_id(i)
                        .style(ButtonStyle::Secondary)
                        .label(bet_opts_display(percent))
                });
            }
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
