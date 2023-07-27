use std::sync::atomic::AtomicBool;
use betting::{Bets, BetError, AccountStatus};
use serenity_utils::DBMap;
use anyhow::{Result, Ok};
use crate::{serialize_utils::BetOutcome, config::config};

pub struct BettingBot {
    pub bets: Bets,
    pub msg_map: DBMap<BetOutcome, u64>,
    pub is_loop_running: AtomicBool
}

impl BettingBot {
    pub fn new() -> Self {
        BettingBot { 
            bets: Bets::new("bets.db").unwrap(), 
            msg_map: DBMap::new("msg_map.db").unwrap(),
            is_loop_running: AtomicBool::new(false)
        }
    }

    pub fn balance_create(&self, server: u64, user: u64) -> Result<u64> {
        Ok(match self.bets.balance(server, user) {
            Err(BetError::NotFound) => {
                self.bets.create_account(server, user, config.starting_coins as u64)?;
                config.starting_coins as u64
            },
            res => res?
        })
    }

    pub fn account_create(&self, server: u64, user: u64) -> Result<AccountStatus> {
        Ok(match self.bets.account(server, user) {
            Err(BetError::NotFound) => {
                self.bets.create_account(server, user, config.starting_coins as u64)?;
                AccountStatus { user, balance: config.starting_coins as u64, in_bet: 0 }
            },
            res => res?
        })
    }
}
