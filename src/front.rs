use rusqlite::{Connection, Result};
use serde_json::{map::Map, value::Value};
use serenity::{
    client::Context,
    http::Http,
    model::{
        channel::Channel,
        id::{ChannelId, GuildId, UserId},
        misc::ChannelIdParseError,
    },
    model::{ModelError, Permissions},
};
use std::{fmt::Display, str::FromStr};

use crate::bets::{BetStatus, OptionStatus};

fn number_display(balance: u32) -> String {
    // TODO: implement clever display with shorteners such as k, m, b
    format!("{}", balance)
}

pub async fn is_readable(ctx: &Context, channel_id: ChannelId) -> bool {
    if let Ok(Channel::Guild(channel)) = channel_id.to_channel(&ctx.http).await {
        if let Ok(me) = ctx.http.get_current_user().await {
            if let Ok(perms) = channel.permissions_for_user(&ctx.cache, me.id).await {
                return perms.read_messages();
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

pub fn options_display(bet_status: &BetStatus) -> Vec<String> {
    todo!()
}

pub async fn update_options(ctx: &Context, channel_id: ChannelId, bet_status: &BetStatus) {
    todo!()
}

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
        let res = match conn
            .prepare(
                "SELECT thread_id 
                	FROM AccountThread
                	WHERE server_id = ?1 AND user_id = ?2
                	",
            )
            .unwrap()
            .query_map(&[server, user], |row| row.get::<usize, String>(0))?
            .next()
        {
            Some(res) => Ok(res?),
            None => Err(FrontError::NotFound),
        };
        res
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

    pub async fn update_account_thread<D>(
        &self,
        http: &Http,
        server: GuildId,
        user: UserId,
        balance: u32,
        msg: D,
    ) -> Result<(), FrontError>
    where
        D: Display,
    {
        let thread_str = self.get(&format!("{}", server), &format!("{}", user))?;
        let thread = ChannelId::from_str(&thread_str)?;
        thread
            .edit(http, |edit| edit.name(number_display(balance)))
            .await?;
        thread.say(http, msg).await?;
        Ok(())
    }
}
