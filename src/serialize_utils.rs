use std::fmt::Display;

use anyhow::{anyhow, bail};
use itertools::Itertools;
use rusqlite::{ToSql, types::{ToSqlOutput, Value}};
pub const LOCK: &str = "lock";
pub const BET_CLICK: &str = "bet_click";
pub const RESOLVE: &str = "resolve";
pub const ABORT: &str = "abort";
pub const BET_ORDER: &str = "bet_order";

#[derive(Debug)]
pub enum BetAction {
    Lock,
    Abort,
    BetClick(BetOutcome),
    Resolve(BetOutcome),
    BetOrder
}

impl Display for BetAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", match self {
            BetAction::Lock => format!("{}-", LOCK),
            BetAction::BetClick(bet_outcome) => format!("{}-{}", BET_CLICK, bet_outcome.to_string()),
            BetAction::Resolve(bet_outcome) => format!("{}-{}", RESOLVE, bet_outcome.to_string()),
            BetAction::Abort => format!("{}-", ABORT),
            BetAction::BetOrder => format!("{}-", BET_ORDER)
        })
    }
}

impl From<BetAction> for String {
    fn from(value: BetAction) -> Self {
        value.to_string()
    }
}

impl TryFrom<String> for BetAction {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let (action, data) = value.splitn(2, "-").collect_tuple().ok_or(
            anyhow!("'{}' is not a BetAction. Expecting <action>-<data>", value)
        )?;
        Ok(match action {
            LOCK => BetAction::Lock,
            BET_CLICK => BetAction::BetClick(BetOutcome::try_from(data)?),
            RESOLVE => BetAction::Resolve(BetOutcome::try_from(data)?),
            ABORT => BetAction::Abort,
            BET_ORDER => BetAction::BetOrder,
            _ => bail!("Bet action '{}' not recognized", action)
        })
    }
}

#[derive(Debug, Clone)]
pub struct BetOutcome {
    pub bet_id: u64,
    pub outcome_id: usize
}

impl ToString for BetOutcome {
    fn to_string(&self) -> String {
        format!("{}-{}", self.bet_id, self.outcome_id)
    }
}

impl TryFrom<&str> for BetOutcome {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let (outcome_id, bet_id) = value.rsplitn(2, "-").collect_tuple().ok_or(
            anyhow!("'{}' is not a BetOutcome. Expecting <outcome_id>-<bet_id>", value)
        )?;
        Ok(BetOutcome { bet_id: bet_id.parse()?, outcome_id: outcome_id.parse()? })
    }
}

impl ToSql for BetOutcome {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Owned(Value::Text(self.to_string())))
    }
}
