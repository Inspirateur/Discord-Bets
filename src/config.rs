use serde::{Serialize, Deserialize};
use lazy_static::lazy_static;
use crate::amount::Amount;

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub currency: String,
    pub starting_coins: u32,
    pub income: u32,
    pub interval: u64,
    pub bet_amounts: Vec<Amount>
}

impl Default for Config {
    fn default() -> Self {
        Self { 
            currency: "💵".to_string(), starting_coins: 300, 
            income: 5, interval: 3, 
            bet_amounts: vec![Amount::FRACTION(0.1), Amount::FRACTION(0.5), Amount::FRACTION(1.0)]
        }
    }
}

lazy_static! {
    pub static ref config: Config = confy::load("discord-bets", None).unwrap();
}
