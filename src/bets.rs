use std::collections::HashMap;

use rusqlite::{Connection, Result, Transaction, NO_PARAMS};
use serenity::constants::OpCode;

struct Bets {
    conn: Connection,
}

struct AccountUpdate {
    user: String,
    diff: i32,
    balance: u32,
}

struct OptionStatus {
    option: String,
    // [(user, amount), ]
    wagers: Vec<(String, u32)>,
}

struct AccountStatus {
    user: String,
    balance: u32,
    in_bet: u32,
}

#[derive(Debug)]
enum BetError {
    MultiOpt(Vec<(String, u32)>),
    UserNotFound,
    NotEnoughMoney,
    BetLocked,
    BetNotFound,
    SQLiteError(rusqlite::Error),
}

impl From<rusqlite::Error> for BetError {
    fn from(err: rusqlite::Error) -> Self {
        BetError::SQLiteError(err)
    }
}

impl Bets {
    pub fn new() -> Result<Self, rusqlite::Error> {
        let conn = Connection::open("bets.db")?;
        conn.execute(
            "create table if not exists Account (
                server_id text,
                user_id text,
                balance integer not null,
                primary key(server_id, user_id)
            )",
            [],
        )?;
        conn.execute(
            "create table if not exists Bet (
                server_id text,
                bet_id text,
                is_open integer not null,
                primary key(server_id, bet_id)
            )",
            [],
        )?;
        conn.execute(
            "create table if not exists Option (
                server_id text,
                option_id text,
                bet_id text,
                foreign key(server_id, bet_id) references Bet(server_id, bet_id),
                primary key(server_id, option_id)
            )",
            [],
        )?;
        conn.execute(
            "create table if not exists Wager (
                server_id text,
                option_id text,
                user_id text,
                amount integer not null,
                foreign key(server_id, option_id) references Option(server_id, option_id),
                foreign key(server_id, user_id) references Account(server_id, user_id),
                primary key(server_id, option_id, user_id)
            )",
            [],
        )?;
        Ok(Bets { conn: conn })
    }

    pub fn create_account(
        &mut self,
        server: &str,
        user: &str,
        amount: u32,
    ) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "insert 
            into Account (server_id, user_id, balance) 
            values (?1, ?2, ?3)",
            &[server, user, &format!("{}", amount)],
        )?;
        Ok(())
    }

    pub fn create_bet(
        &mut self,
        server: &str,
        bet: &str,
        options: Vec<&str>,
    ) -> Result<(), rusqlite::Error> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "insert 
            into Bet (server_id, bet_id, is_open) 
            values (?1, ?2, ?3)",
            &[server, bet, "1"],
        )?;
        for option in options {
            tx.execute(
                "insert 
                into Option (server_id, option_id, bet_id) 
                values (?1, ?2, ?3)",
                &[server, option, bet],
            )?;
        }
        tx.commit()
    }

    fn bet_of_option(&mut self, server: &str, option: &str) -> Result<String, BetError> {
        match self
            .conn
            .prepare(
                "select bet_id 
                from Option
                where server_id=?1 and option_id=?2
                ",
            )
            .unwrap()
            .query_map(&[server, option], |row| row.get::<usize, String>(0))?
            .next()
        {
            Some(res) => Ok(res?),
            None => Err(BetError::SQLiteError(rusqlite::Error::QueryReturnedNoRows)),
        }
    }

    fn options_of_bet(&mut self, server: &str, bet: &str) -> Result<Vec<String>, BetError> {
        Ok(self
            .conn
            .prepare(
                "select option_id 
                from Option
                where server_id=?1 and bet_id=?2
                ",
            )
            .unwrap()
            .query_map(&[server, bet], |row| row.get::<usize, String>(0))?
            .collect::<Result<Vec<_>, _>>()?)
    }

    fn other_options(&mut self, server: &str, option: &str) -> Result<Vec<String>, BetError> {
        let bet = self.bet_of_option(server, option)?;
        Ok(self
            .options_of_bet(server, &bet)?
            .into_iter()
            .filter(|opt| opt != option)
            .collect())
    }

    fn wager(&mut self, server: &str, option: &str, user: &str) -> Result<Option<u32>, BetError> {
        match self
            .conn
            .prepare(
                "select amount 
                    from Wager
                    where server_id=?1 and option_id=?2 and user_id=?3
                    ",
            )
            .unwrap()
            .query_map(&[server, option, user], |row| row.get::<usize, u32>(0))?
            .next()
        {
            Some(res) => Ok(Some(res?)),
            None => Ok(None),
        }
    }

    fn other_wagers(
        &mut self,
        server: &str,
        option: &str,
        user: &str,
    ) -> Result<Vec<(String, u32)>, BetError> {
        Ok(self
            .other_options(server, option)?
            .iter()
            .map(|opt| match self.wager(server, opt, user) {
                Ok(wager_opt) => Ok((opt, wager_opt)),
                Err(err) => Err(err),
            })
            .collect::<Result<Vec<_>, _>>()?
            .iter()
            .filter_map(|(opt, wager_opt)| match wager_opt {
                Some(wager) => Some(((*opt).clone(), *wager)),
                None => None,
            })
            .collect::<Vec<(String, u32)>>())
    }

    fn option_status(&mut self, server: &str, option: &str) -> Result<OptionStatus, BetError> {
        let mut stmt = self
            .conn
            .prepare(
                "select option_id, amount
        from Wager
        where server_id = ?1 and option_id = ?2",
            )
            .unwrap();
        let mut rows = stmt.query(&[server, option])?;
        let mut wagers = Vec::new();
        while let Some(row) = rows.next()? {
            wagers.push((row.get::<usize, String>(0)?, row.get::<usize, u32>(1)?));
        }
        Ok(OptionStatus {
            option: option.to_string(),
            wagers: wagers,
        })
    }

    fn options_statuses(&mut self, server: &str, bet: &str) -> Result<Vec<OptionStatus>, BetError> {
        let options = self.options_of_bet(server, bet)?;
        options
            .into_iter()
            .map(|opt| self.option_status(server, &opt))
            .collect::<Result<Vec<_>, _>>()
    }

    fn is_bet_open(&mut self, server: &str, bet: &str) -> Result<bool, BetError> {
        match self
            .conn
            .prepare(
                "select is_open 
                from Bet
                where server_id=?1 and bet_id=?2
                ",
            )
            .unwrap()
            .query_map(&[server, bet], |row| row.get::<usize, u32>(0))?
            .next()
        {
            Some(res) => Ok(res? != 0),
            None => Err(BetError::BetNotFound),
        }
    }

    pub fn bet_on(
        &mut self,
        server: &str,
        option: &str,
        user: &str,
        fraction: f32,
    ) -> Result<(AccountUpdate, Vec<OptionStatus>), BetError> {
        // check if the bet is open
        let bet = self.bet_of_option(server, option)?;
        if !self.is_bet_open(server, &bet)? {
            return Err(BetError::BetLocked);
        }
        // check if the user has not already bet on other options of the same bet
        let other_wagers = self.other_wagers(server, option, user)?;
        if other_wagers.len() > 0 {
            return Err(BetError::MultiOpt(other_wagers));
        }
        // compute the amount to bet
        assert!(0. <= fraction && fraction <= 1.);
        let balance = self.account(server, user)?;
        let amount = f32::ceil(balance as f32 * fraction) as u32;
        if amount == 0 {
            return Err(BetError::NotEnoughMoney);
        }
        // bet
        let tx = self.conn.transaction()?;
        tx.execute(
            "update Account
            set balance = ?1
            where server_id = ?2 and user_id = ?3
            ",
            &[&format!("{}", balance - amount), server, user],
        )?;
        tx.execute(
            "insert or ignore
            into Wager (server_id, option_id, user_id, amount)
            values (?1, ?2, ?3, ?4)",
            &[server, option, user, "0"],
        )?;
        tx.execute(
            "update Wager
            set amount = amount + ?1
            where server_id = ?2 and option_id = ?3 and user_id = ?4
            ",
            &[&format!("{}", amount), server, option, user],
        )?;
        tx.commit()?;
        // retrieve the options
        Ok((
            AccountUpdate {
                user: user.to_string(),
                diff: -(amount as i32),
                balance: self.account(server, user)?,
            },
            self.options_statuses(server, &bet)?,
        ))
    }

    pub fn lock_bet(&mut self, server: &str, bet: &str) -> Result<(), BetError> {
        self.conn.execute(
            "update Bet
            set is_open = 0
            where server_id = ?1 and bet_id = ?2",
            &[server, bet],
        )?;
        Ok(())
    }

    pub fn abort_bet(&mut self, server: &str, bet: &str) -> Result<Vec<AccountUpdate>, BetError> {
        todo!()
    }

    pub fn close_bet(
        &mut self,
        server: &str,
        winning_option: &str,
    ) -> Result<Vec<AccountUpdate>, BetError> {
        let bet = self.bet_of_option(server, winning_option)?;
        let options_statuses = self.options_statuses(server, &bet)?;
        let mut winning_wagers: Vec<(String, f32)> = Vec::new();
        let mut to_distribute = 0;
        for option_status in options_statuses {
            let option_sum = option_status
                .wagers
                .iter()
                .fold(0, |init, wager| init + wager.1);
            to_distribute += option_sum;
            if option_status.option == winning_option {
                winning_wagers = option_status
                    .wagers
                    .into_iter()
                    .map(|(user, wager)| (user, wager as f32 / option_sum as f32))
                    .collect();
            }
        }
        winning_wagers = winning_wagers
            .into_iter()
            .map(|(user, part)| (user, part * to_distribute as f32))
            .collect();
        let mut gains: Vec<(&str, u32)> = winning_wagers
            .iter()
            .map(|(user, gain)| (user.as_str(), *gain as u32))
            .collect();
        to_distribute -= gains.iter().fold(0, |init, (_, gain)| init + *gain);
        assert!(to_distribute <= gains.len() as u32);
        let winner_parts: HashMap<String, f32> = winning_wagers.clone().into_iter().collect();
        gains.sort_unstable_by(|(user1, gain1), (user2, gain2)| {
            (winner_parts[*user1] - winner_parts[*user1].floor())
                .partial_cmp(&(winner_parts[*user2] - winner_parts[*user2]))
                .unwrap()
        });
        gains.reverse();
        for i in 0..to_distribute as usize {
            gains[i].1 += 1;
        }
        let mut account_updates = Vec::new();
        for (user, gain) in gains {
            let balance = self.account(server, user)? + gain;
            self.conn.execute(
                "update Account
                set balance = ?1
                where server_id = ?2 and user_id = ?3",
                &[&format!("{}", balance), server, user],
            )?;
            account_updates.push(AccountUpdate {
                user: user.to_string(),
                diff: gain as i32,
                balance: balance,
            });
        }
        Ok(account_updates)
    }

    fn account(&mut self, server: &str, user: &str) -> Result<u32, BetError> {
        match self
            .conn
            .prepare(
                "select balance 
                    from Account
                    where server_id=?1 and user_id=?2
                    ",
            )
            .unwrap()
            .query_map(&[server, user], |row| row.get::<usize, u32>(0))?
            .next()
        {
            Some(res) => Ok(res?),
            None => Err(BetError::UserNotFound),
        }
    }

    pub fn accounts(&mut self, server: &str) -> Vec<AccountStatus> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use crate::bets::Bets;

    #[test]
    fn create_db() {
        match Bets::new() {
            Ok(mut bets) => {
                if let Err(why) = bets.create_account("server", "user", 10) {
                    println!("{}", why);
                }

                if let Err(why) = bets.create_bet("server", "bet", vec!["option1", "option2"]) {
                    println!("{}", why);
                }

                if let Err(why) = bets.bet_on("server", "option2", "user", 0.5) {
                    println!("{:?}", why);
                }
            }
            Err(why) => println!("{}", why),
        };
    }
}
