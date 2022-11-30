use std::str::FromStr;
use itertools::Itertools;
use serde::{Serialize, Deserialize};
use lazy_static::lazy_static;
use confy;
use crate::amount::Amount;

#[derive(Serialize, Deserialize)]
struct PartialConfig {
    pub currency: String,
    pub starting_coins: u32,
    pub income: u32,
    pub interval: u64,
    pub bet_amounts: Vec<String>
}

impl Default for PartialConfig {
    fn default() -> Self {
        Self { 
            currency: "💵".to_string(), starting_coins: 300, 
            income: 5, interval: 3, 
            bet_amounts: vec!["10%".to_string(), "50%".to_string(), "100%".to_string()]
        }
    }
}

pub struct Config {
    pub currency: String,
    pub starting_coins: u32,
    pub income: u32,
    pub interval: u64,
    pub bet_amounts: Vec<Amount>
}

impl Config {
    fn from(part_cfg: PartialConfig) -> Self {
        Self {
            currency: part_cfg.currency,
            starting_coins: part_cfg.starting_coins,
            income: part_cfg.income,
            interval: part_cfg.interval,
            bet_amounts: part_cfg.bet_amounts.iter().map(
                |str_amount| Amount::from_str(str_amount).unwrap()
            ).collect_vec()
        }
    }
}

lazy_static! {
    pub static ref config: Config = Config::from(confy::load_path("./config.toml").unwrap());
}
