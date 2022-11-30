use crate::bets::{AccountUpdate, BetStatus, OptionStatus};
use crate::utils::lrm;
use crate::config::config;
use itertools;
use rusqlite::{Connection, Result};
use serde_json::{map::Map, value::Value};
use serenity::{
    client::Context,
    http::Http,
    model::{
        channel::Channel,
        channel::GuildChannel,
        id::{ChannelId, GuildId, UserId},
        misc::ChannelIdParseError,
    },
    model::{ModelError, Permissions},
};
use std::{cmp::min, fmt::Display, str::FromStr};

const NUM_SUFFIX: [&str; 5] = ["", "K", "M", "B", "T"];

fn number_display<R>(x: R) -> String
where
    R: Into<f64>,
{
    let a: f64 = x.into();
    if !a.is_finite() {
        return format!("{}", a);
    }
    let digit_len = (a as u32).to_string().len();
    let suffix_id = min(
        NUM_SUFFIX.len(),
        (digit_len as f32 / 3 as f32).ceil() as usize - 1,
    );
    let a = a / 10.0_f64.powi(3 * suffix_id as i32);
    let repr = if digit_len % 3 == 1 {
        format!("{:.1}", a).trim_end_matches(".0").to_string()
    } else {
        format!("{:.0}", a)
    };
    repr + NUM_SUFFIX[suffix_id]
}

pub async fn is_readable(ctx: &Context, channel_id: ChannelId) -> bool {
    if let Ok(Channel::Guild(channel)) = channel_id.to_channel(&ctx.http).await {
        if let Ok(me) = ctx.http.get_current_user().await {
            if let Ok(perms) = channel.permissions_for_user(&ctx.cache, me.id) {
                return perms.read_message_history();
            }
        }
    }
    false
}

fn option_stub(option_desc: &String) -> OptionStatus {
    OptionStatus {
        option: String::new(),
        desc: option_desc.clone(),
        wagers: Vec::new(),
    }
}

pub fn bet_stub(options_desc: &Vec<String>) -> BetStatus {
    BetStatus {
        bet: String::new(),
        desc: String::new(),
        options: options_desc.iter().map(option_stub).collect(),
    }
}

fn option_display(desc: &str, percent: u32, odd: f32, sum: u32, people: u32) -> String {
    format!(
        "> {}\n` {: >3}%  | {: >6} 🏆  {: >4} {}  {: >4} 👥 `",
        desc,
        percent,
        "1:".to_string() + &number_display(if odd.is_nan() { 1. } else { odd }),
        number_display(sum),
        config.currency,
        number_display(people)
    )
}

pub fn options_display(bet_status: &BetStatus) -> Vec<String> {
    let sums: Vec<u32> = bet_status
        .options
        .iter()
        .map(|option| {
            option
                .wagers
                .iter()
                .fold(0, |init, (_, amount)| init + amount)
        })
        .collect();

    let total = sums.iter().fold(0, |init, sum| init + *sum);

    let percents = lrm(100, &sums);

    let odds: Vec<f32> = sums.iter().map(|sum| total as f32 / *sum as f32).collect();

    let peoples: Vec<usize> = bet_status
        .options
        .iter()
        .map(|option| option.wagers.len())
        .collect();

    itertools::izip!(&bet_status.options, percents, odds, sums, peoples)
        .map(|(option, percent, odd, sum, people)| {
            option_display(&option.desc, percent, odd, sum, people as u32)
        })
        .collect()
}

pub async fn update_options(
    http: &Http,
    channel: &GuildChannel,
    bet_status: &BetStatus,
) -> Result<(), serenity::Error> {
    let options_msg = options_display(bet_status);
    for (option, msg) in itertools::izip!(&bet_status.options, options_msg) {
        channel
            .edit_message(http, option.option.parse::<u64>().unwrap(), |message| {
                message.content(msg)
            })
            .await?;
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct Front {
    db_path: String,
}

#[derive(Debug)]
pub enum FrontError {
    NotFound,
    LackPermission(Permissions),
    AlreadyExists,
    InternalError(String),
}

impl From<rusqlite::Error> for FrontError {
    fn from(err: rusqlite::Error) -> Self {
        // the only error we want to separate is the unique constraint violation
        if let rusqlite::Error::SqliteFailure(sqlerr, _) = err {
            if sqlerr.extended_code == 1555 {
                return FrontError::AlreadyExists;
            }
        } else if let rusqlite::Error::QueryReturnedNoRows = err {
            return FrontError::NotFound;
        }
        FrontError::InternalError(format!("{:?}", err))
    }
}

impl From<serenity::Error> for FrontError {
    fn from(err: serenity::Error) -> Self {
        if let serenity::Error::Model(ModelError::InvalidPermissions(perms)) = err {
            return FrontError::LackPermission(perms);
        }
        FrontError::InternalError(format!("{:?}", err))
    }
}

impl From<ChannelIdParseError> for FrontError {
    fn from(_: ChannelIdParseError) -> Self {
        FrontError::InternalError(String::from("Invalid Account Thread id"))
    }
}

impl Front {
    pub fn new(db_path: &str) -> Result<Self, FrontError> {
        let conn = Connection::open(db_path)?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS AccountThread (
                server_id TEXT,
                user_id TEXT,
                thread_id TEXT not null,
                PRIMARY KEY(server_id, user_id)
            )",
            [],
        )?;
        Ok(Front {
            db_path: db_path.to_string(),
        })
    }

    fn set(&self, server: &str, user: &str, thread: &str) -> Result<(), FrontError> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute(
            "INSERT OR REPLACE
			INTO AccountThread (server_id, user_id, thread_id)
			VALUES (?1, ?2, ?3)",
            &[server, user, thread],
        )?;
        Ok(())
    }

    fn get(&self, server: &str, user: &str) -> Result<String, FrontError> {
        let conn = Connection::open(&self.db_path)?;
        let res = conn
            .prepare(
                "SELECT thread_id 
                FROM AccountThread
                WHERE server_id = ?1 AND user_id = ?2
                ",
            )
            .unwrap()
            .query_row(&[server, user], |row| row.get::<usize, String>(0));
        Ok(res?)
    }

    fn get_all(&self, server: &str) -> Result<Vec<String>, FrontError> {
        let conn = Connection::open(&self.db_path)?;
        let res = conn
            .prepare(
                "SELECT thread_id 
            FROM AccountThread
            WHERE server_id = ?1
            ",
            )
            .unwrap()
            .query_map(&[server], |row| row.get::<usize, String>(0))?
            .into_iter()
            .collect::<Result<Vec<String>, _>>()?;
        Ok(res)
    }

    pub async fn create_account_thread(
        &self,
        ctx: &Context,
        server: GuildId,
        channel: ChannelId,
        user: UserId,
    ) -> Result<(), FrontError> {
        match self.get(&format!("{}", server), &format!("{}", user)) {
            // There's already a thread in the db
            Ok(thread_str) => {
                // If the thread is valid we stop here
                if let Ok(thread) = ChannelId::from_str(&thread_str) {
                    if is_readable(ctx, thread).await {
                        return Err(FrontError::AlreadyExists);
                    }
                }
            }
            // This is what we expect
            Err(FrontError::NotFound) => {}
            Err(err) => {
                return Err(err);
            }
        }
        let parent_msg = channel
            .say(&ctx.http, "Creating the Account Thread")
            .await?;
        let mut json_map = Map::new();
        json_map.insert("name".to_string(), Value::String("XXX".to_string()));
        let thread = ctx
            .http
            .create_public_thread(channel.into(), parent_msg.id.into(), &json_map)
            .await?;
        if let Err(_) = parent_msg.delete(&ctx.http).await {};
        ctx.http
            .add_thread_channel_member(thread.id.into(), user.into())
            .await?;
        self.set(
            &format!("{}", server),
            &format!("{}", user),
            &format!("{}", thread.id),
        )?;
        Ok(())
    }

    pub async fn update_account_thread<D>(&self, http: &Http, acc_update: AccountUpdate, msg: D)
    where
        D: Display,
    {
        self.update_account_threads(http, vec![acc_update], msg)
            .await
    }

    async fn _update_account_threads<D>(
        &self,
        http: &Http,
        acc_updates: Vec<AccountUpdate>,
        msg: D,
    ) -> Result<(), FrontError>
    where
        D: Display,
    {
        let threads_str = acc_updates
            .iter()
            .map(|acc_update| self.get(&format!("{}", &acc_update.server), &acc_update.user))
            .collect::<Result<Vec<_>, _>>()?;
        let threads: Vec<_> = threads_str
            .into_iter()
            .map(|thread_str| ChannelId::from_str(&thread_str).unwrap())
            .collect();
        for (thread, acc_update) in itertools::izip!(&threads, &acc_updates) {
            thread
                .say(
                    http,
                    format!(
                        "{}\nNew balance: **{}** {}",
                        msg, acc_update.balance, config.currency
                    )
                    .replace("{diff}", &acc_update.diff.to_string()),
                )
                .await?;
        }
        for (thread, acc_update) in itertools::izip!(&threads, &acc_updates) {
            thread
                .edit(http, |edit| {
                    edit.name(format!(
                        "{} {}",
                        number_display(acc_update.balance),
                        config.currency
                    ))
                })
                .await?;
        }
        Ok(())
    }

    pub async fn update_account_threads<D>(
        &self,
        http: &Http,
        acc_updates: Vec<AccountUpdate>,
        msg: D,
    ) where
        D: Display,
    {
        if let Err(why) = self._update_account_threads(http, acc_updates, msg).await {
            println!("Failed to update account threads: {:?}", why);
        }
    }

    async fn _update_account_thread_reset(
        &self,
        http: &Http,
        server: &str,
    ) -> Result<(), FrontError> {
        let threads_str = self.get_all(server)?;
        let threads: Vec<_> = threads_str
            .into_iter()
            .map(|thread_str| ChannelId::from_str(&thread_str).unwrap())
            .collect();
        for thread in &threads {
            thread
                .say(
                    http,
                    format!(
                        "ACCOUNT RESET\nNew balance: **{}** {}",
                        config.starting_coins, config.currency
                    ),
                )
                .await?;
        }
        for thread in threads {
            thread
                .edit(http, |edit| {
                    edit.name(format!("{} {}", number_display(config.starting_coins), config.currency))
                })
                .await?;
        }
        Ok(())
    }

    pub async fn update_account_thread_reset(&self, http: &Http, server: &str) {
        if let Err(why) = self._update_account_thread_reset(http, server).await {
            println!("Failed to update account thread for reset: {:?}", why);
        }
    }

    async fn _error_account_thread<D>(
        &self,
        http: &Http,
        server: &str,
        user: &str,
        msg: D,
    ) -> Result<(), FrontError>
    where
        D: Display,
    {
        let thread_str = self.get(server, user)?;
        let thread = ChannelId::from_str(&thread_str).unwrap();
        thread.say(http, msg).await?;
        Ok(())
    }

    pub async fn error_account_thread<D>(&self, http: &Http, server: &str, user: &str, msg: D)
    where
        D: Display,
    {
        if let Err(why) = self._error_account_thread(http, server, user, msg).await {
            println!("Error when sending error msg to account thread: {:?}", why);
        }
    }
}
