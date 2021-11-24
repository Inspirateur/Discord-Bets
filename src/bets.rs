use crate::utils;
use rusqlite::{Connection, Result};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Bets {
    db_path: String,
}

#[derive(Debug, Clone)]
pub struct AccountUpdate {
    pub server: String,
    pub user: String,
    pub diff: i32,
    pub balance: u32,
}

pub struct BetStatus {
    pub bet: String,
    pub desc: String,
    pub options: Vec<OptionStatus>,
}

pub struct OptionStatus {
    pub option: String,
    pub desc: String,
    // [(user, amount), ]
    pub wagers: Vec<(String, u32)>,
}

pub struct AccountStatus {
    pub user: String,
    pub balance: u32,
    pub in_bet: u32,
}

#[derive(Debug)]
pub enum BetError {
    MultiOpt(Vec<String>),
    NotFound,
    NotEnoughMoney,
    BetLocked,
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
        } else if let rusqlite::Error::QueryReturnedNoRows = err {
            return BetError::NotFound;
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
                desc TEXT,
                PRIMARY KEY(server_id, bet_id)
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS Option (
                server_id TEXT,
                option_id TEXT,
                bet_id TEXT,
                desc TEXT,
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
        conn.execute(
            "CREATE TABLE IF NOT EXISTS ToDelete (
                server_id TEXT,
                bet_id TEXT,
                FOREIGN KEY(server_id, bet_id) REFERENCES Bet(server_id, bet_id) ON DELETE CASCADE,
                PRIMARY KEY(server_id, bet_id)
            )",
            [],
        )?;
        conn.execute("DELETE FROM ToDelete", [])?;
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

    pub fn reset(&self, server: &str, amount: u32) -> Result<(), BetError> {
        let mut conn = Connection::open(&self.db_path)?;
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE
            FROM Bet
            WHERE server_id = ?1",
            &[server],
        )?;
        tx.execute(
            "UPDATE Account
            SET balance = ?1
            WHERE server_id = ?2",
            &[&amount.to_string(), server],
        )?;
        Ok(tx.commit()?)
    }

    pub fn income(&self, income: u32) -> Result<Vec<AccountUpdate>, BetError> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute(
            "UPDATE Account
            SET balance = balance + ?1",
            [income],
        )?;
        let mut account_updates = Vec::new();
        let mut stmt = conn
            .prepare(
                "SELECT server_id, user_id, balance 
                    FROM Account
                    ",
            )
            .unwrap();
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            account_updates.push(AccountUpdate {
                server: row.get::<usize, String>(0)?,
                user: row.get::<usize, String>(1)?,
                balance: row.get::<usize, u32>(2)?,
                diff: income as i32,
            });
        }
        Ok(account_updates)
    }

    pub fn create_bet<T: AsRef<str>>(
        &self,
        server: &str,
        bet: &str,
        bet_desc: &str,
        options: &[T],
        options_desc: &[T],
    ) -> Result<(), BetError> {
        assert!(options.len() == options_desc.len());
        let mut conn = Connection::open(&self.db_path)?;
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT 
            INTO Bet (server_id, bet_id, is_open, desc) 
            VALUES (?1, ?2, ?3, ?4)",
            &[server, bet, "1", bet_desc],
        )?;
        for i in 0..options.len() {
            tx.execute(
                "INSERT 
                INTO Option (server_id, option_id, bet_id, desc) 
                VALUES (?1, ?2, ?3, ?4)",
                &[server, options[i].as_ref(), bet, options_desc[i].as_ref()],
            )?;
        }
        Ok(tx.commit()?)
    }

    pub fn bet_of_option(&self, server: &str, option: &str) -> Result<String, BetError> {
        let conn = Connection::open(&self.db_path)?;
        Bets::_bet_of_option(&conn, server, option)
    }

    fn _bet_of_option(conn: &Connection, server: &str, option: &str) -> Result<String, BetError> {
        Ok(conn
            .prepare(
                "SELECT bet_id 
                FROM Option
                WHERE server_id = ?1 AND option_id = ?2
                ",
            )
            .unwrap()
            .query_row(&[server, option], |row| row.get::<usize, String>(0))?)
    }

    pub fn options_of_bet(&self, server: &str, bet: &str) -> Result<Vec<String>, BetError> {
        let conn = Connection::open(&self.db_path)?;
        Bets::_options_of_bet(&conn, server, bet)
    }

    fn _options_of_bet(
        conn: &Connection,
        server: &str,
        bet: &str,
    ) -> Result<Vec<String>, BetError> {
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
        let bet = Bets::_bet_of_option(conn, server, option)?;
        Ok(Bets::_options_of_bet(conn, server, &bet)?
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
        Ok(match conn
            .prepare(
                "SELECT amount 
                    FROM Wager
                    WHERE server_id = ?1 AND option_id = ?2 AND user_id = ?3
                    ",
            )
            .unwrap()
            .query_row(&[server, option, user], |row| row.get::<usize, u32>(0))
        {
            Ok(res) => Ok(Some(res)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(err),
        }?)
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
        let desc = conn
            .prepare(
                "SELECT desc
            FROM Option
            WHERE server_id = ?1 AND option_id = ?2",
            )
            .unwrap()
            .query_row(&[server, option], |row| row.get::<usize, String>(0))?;
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
            desc: desc,
            wagers: wagers,
        })
    }

    fn options_statuses(
        conn: &Connection,
        server: &str,
        bet: &str,
    ) -> Result<Vec<OptionStatus>, BetError> {
        let options = Bets::_options_of_bet(conn, server, bet)?;
        options
            .into_iter()
            .map(|opt| Bets::option_status(conn, server, &opt))
            .collect::<Result<Vec<_>, _>>()
    }

    fn is_bet_open(conn: &Connection, server: &str, bet: &str) -> Result<bool, BetError> {
        Ok(conn
            .prepare(
                "SELECT is_open 
                FROM Bet
                WHERE server_id = ?1 AND bet_id = ?2
                ",
            )
            .unwrap()
            .query_row(&[server, bet], |row| row.get::<usize, u32>(0))?
            != 0)
    }

    fn assert_bet_not_deleted(conn: &Connection, server: &str, bet: &str) -> Result<(), BetError> {
        if conn
            .prepare(
                "SELECT * 
                FROM ToDelete
                WHERE server_id = ?1 AND bet_id = ?2
                ",
            )
            .unwrap()
            .exists(&[server, bet])?
        {
            Err(BetError::NotFound)
        } else {
            Ok(())
        }
    }

    pub fn bet_on(
        &self,
        server: &str,
        option: &str,
        user: &str,
        fraction: f32,
    ) -> Result<(AccountUpdate, BetStatus), BetError> {
        let mut conn = Connection::open(&self.db_path)?;
        // check if the bet is open
        let bet = Bets::_bet_of_option(&conn, server, option)?;
        if !Bets::is_bet_open(&conn, server, &bet)? {
            return Err(BetError::BetLocked);
        }
        Bets::assert_bet_not_deleted(&conn, server, &bet)?;
        // check if the user has not already bet on other options of the same bet
        let other_wagers = Bets::other_wagers(&conn, server, option, user)?;
        if other_wagers.len() >= 1 {
            return Err(BetError::MultiOpt(
                other_wagers.into_iter().map(|(opt, _)| opt).collect(),
            ));
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
        let desc = conn
            .prepare(
                "SELECT desc
            FROM Bet
            WHERE server_id = ?1 AND bet_id = ?2",
            )
            .unwrap()
            .query_row(&[server, &bet], |row| row.get::<usize, String>(0))?;
        Ok((
            AccountUpdate {
                server: server.to_string(),
                user: user.to_string(),
                diff: -(amount as i32),
                balance: Bets::_balance(&conn, server, user)?,
            },
            BetStatus {
                bet: bet.clone(),
                desc: desc,
                options: Bets::options_statuses(&conn, server, &bet)?,
            },
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
        let mut conn = Connection::open(&self.db_path)?;
        Bets::assert_bet_not_deleted(&conn, server, bet)?;
        let wagers: Vec<(String, u32)> = Bets::options_statuses(&conn, server, &bet)?
            .into_iter()
            .flat_map(|option_status| option_status.wagers)
            .collect();
        let mut account_updates = Vec::new();
        // retrieve the balance of winners
        let balances = wagers
            .iter()
            .map(|(user, _)| Ok(Bets::_balance(&conn, server, user)?))
            .collect::<Result<Vec<_>, BetError>>()?;

        let tx = conn.transaction()?;
        for ((user, amount), balance) in itertools::izip!(wagers, balances) {
            tx.execute(
                "UPDATE Account
                SET balance = ?1
                WHERE server_id = ?2 AND user_id = ?3",
                &[&format!("{}", balance + amount), server, &user],
            )?;
            account_updates.push(AccountUpdate {
                server: server.to_string(),
                user: user,
                diff: amount as i32,
                balance: balance + amount,
            });
        }
        // delete the bet
        tx.execute(
            "INSERT 
            INTO ToDelete (server_id, bet_id)
            VALUES (?1, ?2)",
            &[server, &bet],
        )?;
        tx.commit()?;
        Ok(account_updates)
    }

    pub fn close_bet(
        &self,
        server: &str,
        winning_option: &str,
    ) -> Result<Vec<AccountUpdate>, BetError> {
        let mut conn = Connection::open(&self.db_path)?;
        // retrieve the total of the bet and the winning parts
        let bet = Bets::_bet_of_option(&conn, server, winning_option)?;
        Bets::assert_bet_not_deleted(&conn, server, &bet)?;
        let options_statuses = Bets::options_statuses(&conn, server, &bet)?;
        let mut winners: Vec<String> = Vec::new();
        let mut wins: Vec<u32> = Vec::new();
        let mut total = 0;
        for option_status in options_statuses {
            let option_sum = option_status
                .wagers
                .iter()
                .fold(0, |init, wager| init + wager.1);
            total += option_sum;
            if option_status.option == winning_option {
                for (winner, win) in option_status.wagers {
                    winners.push(winner);
                    wins.push(win);
                }
            }
        }
        // compute the gains for each winners
        let gains = utils::lrm(total, &wins);
        // retrieve the balance of winners
        let balances = winners
            .iter()
            .map(|winner| Ok(Bets::_balance(&conn, server, winner)?))
            .collect::<Result<Vec<_>, BetError>>()?;
        // update the accounts
        let mut account_updates = Vec::new();
        let tx = conn.transaction()?;
        for (user, balance, gain) in itertools::izip!(winners, balances, gains) {
            tx.execute(
                "UPDATE Account
                SET balance = ?1
                WHERE server_id = ?2 AND user_id = ?3",
                &[&format!("{}", balance + gain), server, &user],
            )?;
            account_updates.push(AccountUpdate {
                server: server.to_string(),
                user: user.to_string(),
                diff: gain as i32,
                balance: balance + gain,
            });
        }
        // delete the bet
        tx.execute(
            "INSERT 
            INTO ToDelete (server_id, bet_id)
            VALUES (?1, ?2)",
            &[server, &bet],
        )?;
        tx.commit()?;
        Ok(account_updates)
    }

    fn _balance(conn: &Connection, server: &str, user: &str) -> Result<u32, BetError> {
        Ok(conn
            .prepare(
                "SELECT balance 
                    FROM Account
                    WHERE server_id = ?1 AND user_id = ?2
                    ",
            )
            .unwrap()
            .query_row(&[server, user], |row| row.get::<usize, u32>(0))?)
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
                in_bet: *wagers.get(&user).unwrap_or(&0),
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
                    println!("1 {:?}", why);
                }
                if let Err(why) = bets.create_account("server", "Manu", 10) {
                    println!("2 {:?}", why);
                }
                if let Err(why) = bets.create_account("server", "Roux", 10) {
                    println!("3 {:?}", why);
                }
                if let Err(why) = bets.create_bet(
                    "server",
                    "bet1",
                    "Will roux go to sleep soon ?",
                    &vec!["opt1", "opt2"],
                    &vec!["oui", "non"],
                ) {
                    println!("4 {:?}", why);
                }
                if let Err(why) = bets.bet_on("server", "opt1", "Roux", 0.5) {
                    println!("5 {:?}", why);
                }
                if let Err(why) = bets.bet_on("server", "opt2", "Teo", 0.3) {
                    println!("6 {:?}", why);
                }
                if let Err(why) = bets.bet_on("server", "opt2", "Manu", 0.4) {
                    println!("7 {:?}", why);
                }
                if let Err(why) = bets.close_bet("server", "opt1") {
                    println!("8 {:?}", why);
                }
            }
            Err(why) => println!("{:?}", why),
        };
    }
}
