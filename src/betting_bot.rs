use betting::Bets;
use db_map::DBMap;
use crate::serialize_utils::BetOutcome;

pub struct BettingBot {
    pub bets: Bets,
    pub msg_map: DBMap<BetOutcome, u64>,
}

impl BettingBot {
    pub fn new() -> Self {
        BettingBot { 
            bets: Bets::new("bets.db").unwrap(), 
            msg_map: DBMap::new("msg_map.db").unwrap(),
        }
    }
}
