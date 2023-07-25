use serde::{Serialize, Deserialize};
use lazy_static::lazy_static;
use confy;

#[derive(Serialize, Deserialize)]
struct PartialConfig {
    pub currency: String,
    pub starting_coins: u32,
    pub income: u32,
    pub interval: u64
}

impl Default for PartialConfig {
    fn default() -> Self {
        Self { 
            currency: "💵".to_string(), starting_coins: 350, 
            income: 50, interval: 24
        }
    }
}

pub struct Config {
    pub currency: String,
    pub starting_coins: u32,
    pub income: u32,
    pub interval: u64
}

impl Config {
    fn from(part_cfg: PartialConfig) -> Self {
        Self {
            currency: part_cfg.currency,
            starting_coins: part_cfg.starting_coins,
            income: part_cfg.income,
            interval: part_cfg.interval
        }
    }
}

lazy_static! {
    pub static ref config: Config = Config::from(confy::load_path("./config.toml").unwrap());
}
