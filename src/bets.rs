use rusqlite::{Connection, ErrorCode, Result};
use std::collections::HashMap;

pub struct Bets {
    db_path: String,
}

pub struct AccountUpdate {
    user: String,
    diff: i32,
    balance: u32,
}

pub struct OptionStatus {
    option: String,
    // [(user, amount), ]
    wagers: Vec<(String, u32)>,
}

pub struct AccountStatus {
    user: String,
    balance: u32,
    in_bet: u32,
}

#[derive(Debug)]
pub enum BetError {
    MultiOpt(Vec<(String, u32)>),
    UserNotFound,
    NotEnoughMoney,
    BetLocked,
    BetNotFound,
    AlreadyExists,
    InternalError(rusqlite::Error),
}

impl From<rusqlite::Error> for BetError {
    fn from(err: rusqlite::Error) -> Self {
        // the only error we want to separate is the unique constraint violation
        if let rusqlite::Error::SqliteFailure(sqlerr, _) = err {
            if sqlerr.extended_code == 1555 {
                return BetError::AlreadyExists;
            }
        }
        BetError::InternalError(err)
    }
}

impl Bets {
    pub fn new(db_path: &str) -> Result<Self, BetError> {
        let conn = Connection::open(db_path)?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS Account (
                server_id TEXT,
                user_id TEXT,
                balance INTEGER NOT NULL,
                PRIMARY KEY(server_id, user_id)
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS Bet (
                server_id TEXT,
                bet_id TEXT,
                is_open INTEGER NOT NULL,
                PRIMARY KEY(server_id, bet_id)
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS Option (
                server_id TEXT,
                option_id TEXT,
                bet_id TEXT,
                FOREIGN KEY(server_id, bet_id) REFERENCES Bet(server_id, bet_id) ON DELETE CASCADE,
                PRIMARY KEY(server_id, option_id)
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS Wager (
                server_id TEXT,
                option_id TEXT,
                user_id TEXT,
                amount INTEGER NOT NULL,
                FOREIGN KEY(server_id, option_id) REFERENCES Option(server_id, option_id) ON DELETE CASCADE,
                FOREIGN KEY(server_id, user_id) REFERENCES Account(server_id, user_id) ON DELETE CASCADE,
                PRIMARY KEY(server_id, option_id, user_id)
            )",
            [],
        )?;
        Ok(Bets {
            db_path: db_path.to_string(),
        })
    }

    pub fn create_account(&self, server: &str, user: &str, amount: u32) -> Result<(), BetError> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute(
            "INSERT 
            INTO Account (server_id, user_id, balance) 
            VALUES (?1, ?2, ?3)",
            &[server, user, &format!("{}", amount)],
        )?;
        Ok(())
    }

    pub fn create_bet(&self, server: &str, bet: &str, options: Vec<&str>) -> Result<(), BetError> {
        let mut conn = Connection::open(&self.db_path)?;
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT 
            INTO Bet (server_id, bet_id, is_open) 
            VALUES (?1, ?2, ?3)",
            &[server, bet, "1"],
        )?;
        for option in options {
            tx.execute(
                "INSERT 
                INTO Option (server_id, option_id, bet_id) 
                VALUES (?1, ?2, ?3)",
                &[server, option, bet],
            )?;
        }
        Ok(tx.commit()?)
    }

    fn bet_of_option(conn: &Connection, server: &str, option: &str) -> Result<String, BetError> {
        match conn
            .prepare(
                "SELECT bet_id 
                FROM Option
                WHERE server_id = ?1 AND option_id = ?2
                ",
            )
            .unwrap()
            .query_map(&[server, option], |row| row.get::<usize, String>(0))?
            .next()
        {
            Some(res) => Ok(res?),
            None => Err(BetError::InternalError(
                rusqlite::Error::QueryReturnedNoRows,
            )),
        }
    }

    fn options_of_bet(conn: &Connection, server: &str, bet: &str) -> Result<Vec<String>, BetError> {
        Ok(conn
            .prepare(
                "SELECT option_id 
                FROM Option
                WHERE server_id = ?1 AND bet_id = ?2
                ",
            )
            .unwrap()
            .query_map(&[server, bet], |row| row.get::<usize, String>(0))?
            .collect::<Result<Vec<_>, _>>()?)
    }

    fn other_options(
        conn: &Connection,
        server: &str,
        option: &str,
    ) -> Result<Vec<String>, BetError> {
        let bet = Bets::bet_of_option(conn, server, option)?;
        Ok(Bets::options_of_bet(conn, server, &bet)?
            .into_iter()
            .filter(|opt| opt != option)
            .collect())
    }

    fn wager(
        conn: &Connection,
        server: &str,
        option: &str,
        user: &str,
    ) -> Result<Option<u32>, BetError> {
        match conn
            .prepare(
                "SELECT amount 
                    FROM Wager
                    WHERE server_id = ?1 AND option_id = ?2 AND user_id = ?3
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
        conn: &Connection,
        server: &str,
        option: &str,
        user: &str,
    ) -> Result<Vec<(String, u32)>, BetError> {
        Ok(Bets::other_options(conn, server, option)?
            .iter()
            .map(|opt| match Bets::wager(conn, server, opt, user) {
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

    fn option_status(
        conn: &Connection,
        server: &str,
        option: &str,
    ) -> Result<OptionStatus, BetError> {
        let mut stmt = conn
            .prepare(
                "SELECT user_id, amount
                FROM Wager
                WHERE server_id = ?1 AND option_id = ?2",
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

    fn options_statuses(
        conn: &Connection,
        server: &str,
        bet: &str,
    ) -> Result<Vec<OptionStatus>, BetError> {
        let options = Bets::options_of_bet(conn, server, bet)?;
        options
            .into_iter()
            .map(|opt| Bets::option_status(conn, server, &opt))
            .collect::<Result<Vec<_>, _>>()
    }

    fn is_bet_open(conn: &Connection, server: &str, bet: &str) -> Result<bool, BetError> {
        match conn
            .prepare(
                "SELECT is_open 
                FROM Bet
                WHERE server_id = ?1 AND bet_id = ?2
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
        &self,
        server: &str,
        option: &str,
        user: &str,
        fraction: f32,
    ) -> Result<(AccountUpdate, Vec<OptionStatus>), BetError> {
        let mut conn = Connection::open(&self.db_path)?;
        // check if the bet is open
        let bet = Bets::bet_of_option(&conn, server, option)?;
        if !Bets::is_bet_open(&conn, server, &bet)? {
            return Err(BetError::BetLocked);
        }
        // check if the user has not already bet on other options of the same bet
        let other_wagers = Bets::other_wagers(&conn, server, option, user)?;
        if other_wagers.len() > 0 {
            return Err(BetError::MultiOpt(other_wagers));
        }
        // compute the amount to bet
        assert!(0. <= fraction && fraction <= 1.);
        let balance = Bets::_balance(&conn, server, user)?;
        let amount = f32::ceil(balance as f32 * fraction) as u32;
        if amount == 0 {
            return Err(BetError::NotEnoughMoney);
        }
        // bet
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE Account
            SET balance = ?1
            WHERE server_id = ?2 AND user_id = ?3
            ",
            &[&format!("{}", balance - amount), server, user],
        )?;
        tx.execute(
            "INSERT or ignore
            INTO Wager (server_id, option_id, user_id, amount)
            VALUES (?1, ?2, ?3, ?4)",
            &[server, option, user, "0"],
        )?;
        tx.execute(
            "UPDATE Wager
            SET amount = amount + ?1
            WHERE server_id = ?2 AND option_id = ?3 AND user_id = ?4
            ",
            &[&format!("{}", amount), server, option, user],
        )?;
        tx.commit()?;
        // retrieve the options
        Ok((
            AccountUpdate {
                user: user.to_string(),
                diff: -(amount as i32),
                balance: Bets::_balance(&conn, server, user)?,
            },
            Bets::options_statuses(&conn, server, &bet)?,
        ))
    }

    pub fn lock_bet(&self, server: &str, bet: &str) -> Result<(), BetError> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute(
            "UPDATE Bet
            SET is_open = 0
            WHERE server_id = ?1 AND bet_id = ?2",
            &[server, bet],
        )?;
        Ok(())
    }

    pub fn abort_bet(&self, server: &str, bet: &str) -> Result<Vec<AccountUpdate>, BetError> {
        let conn = Connection::open(&self.db_path)?;
        let options_statuses = Bets::options_statuses(&conn, server, &bet)?;
        let mut account_updates = Vec::new();
        for (user, amount) in options_statuses
            .into_iter()
            .flat_map(|option_status| option_status.wagers)
        {
            let balance = Bets::_balance(&conn, server, &user)? + amount;
            conn.execute(
                "UPDATE Account
                SET balance = ?1
                WHERE server_id = ?2 AND user_id = ?3",
                &[&format!("{}", balance), server, &user],
            )?;
            account_updates.push(AccountUpdate {
                user: user,
                diff: amount as i32,
                balance: balance,
            });
        }
        conn.execute(
            "DELETE FROM Bet
            WHERE server_id = ?1 AND bet_id = ?2",
            &[server, bet],
        )?;
        Ok(account_updates)
    }

    pub fn close_bet(
        &self,
        server: &str,
        winning_option: &str,
    ) -> Result<Vec<AccountUpdate>, BetError> {
        let conn = Connection::open(&self.db_path)?;
        // retrieve the total of the bet and the normalized winning parts
        let bet = Bets::bet_of_option(&conn, server, winning_option)?;
        let options_statuses = Bets::options_statuses(&conn, server, &bet)?;
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
        // distribute the gains by dropping the decimal part first
        let mut gains: Vec<(&str, u32)> = winning_wagers
            .iter()
            .map(|(user, gain)| (user.as_str(), *gain as u32))
            .collect();
        to_distribute -= gains.iter().fold(0, |init, (_, gain)| init + *gain);
        assert!(to_distribute <= gains.len() as u32);
        // distribute the remaining coins to those with the bigger decimal parts
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
        // update the accounts
        let mut account_updates = Vec::new();
        for (user, gain) in gains {
            let balance = Bets::_balance(&conn, server, user)? + gain;
            conn.execute(
                "UPDATE Account
                SET balance = ?1
                WHERE server_id = ?2 AND user_id = ?3",
                &[&format!("{}", balance), server, user],
            )?;
            account_updates.push(AccountUpdate {
                user: user.to_string(),
                diff: gain as i32,
                balance: balance,
            });
        }
        // delete the bet
        conn.execute(
            "DELETE FROM Bet
            WHERE server_id = ?1 AND bet_id = ?2",
            &[server, &bet],
        )?;
        Ok(account_updates)
    }

    fn _balance(conn: &Connection, server: &str, user: &str) -> Result<u32, BetError> {
        match conn
            .prepare(
                "SELECT balance 
                    FROM Account
                    WHERE server_id = ?1 AND user_id = ?2
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

    pub fn balance(&self, server: &str, user: &str) -> Result<u32, BetError> {
        let conn = Connection::open(&self.db_path)?;
        Bets::_balance(&conn, server, user)
    }

    pub fn accounts(&self, server: &str) -> Result<Vec<AccountStatus>, BetError> {
        let conn = Connection::open(&self.db_path)?;
        // Map <user, balance>
        let mut accounts = HashMap::new();
        let mut stmt = conn
            .prepare(
                "SELECT user_id, balance 
                    FROM Account
                    WHERE server_id = ?1
                    ",
            )
            .unwrap();
        let mut rows = stmt.query(&[server])?;
        while let Some(row) = rows.next()? {
            accounts.insert(row.get::<usize, String>(0)?, row.get::<usize, u32>(1)?);
        }
        // Map <user, total wagered>
        let mut stmt = conn
            .prepare(
                "SELECT user_id, amount 
                    FROM Wager
                    WHERE server_id = ?1
                    ",
            )
            .unwrap();
        let mut rows = stmt.query(&[server])?;
        let mut wagers = HashMap::new();
        while let Some(row) = rows.next()? {
            let user = row.get::<usize, String>(0)?;
            let amount = match wagers.get(&user) {
                Some(amount) => *amount,
                None => 0,
            };
            wagers.insert(user, amount + row.get::<usize, u32>(1)?);
        }
        // return the account statuses
        Ok(accounts
            .into_iter()
            .map(|(user, balance)| AccountStatus {
                user: user.clone(),
                balance: balance,
                in_bet: wagers[&user],
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use crate::bets::Bets;

    #[test]
    fn create_db() {
        match Bets::new("bets.db") {
            Ok(bets) => {
                if let Err(why) = bets.create_account("server", "Teo", 10) {
                    println!("{:?}", why);
                }
                if let Err(why) = bets.create_account("server", "Teo", 10) {
                    println!("{:?}", why);
                }
            }
            Err(why) => println!("{:?}", why),
        };
    }
}
