use betting::{Bet, Outcome};
use betting::utils::lrm;
use crate::config::config;
use itertools;
use std::cmp::min;

const NUM_SUFFIX: [&str; 5] = ["", "K", "M", "B", "T"];

fn number_display<R>(x: R) -> String
where
    R: Into<f64>,
{
    let a: f64 = x.into();
    if !a.is_finite() {
        return format!("{}", a);
    }
    let digit_len = (a as u32).to_string().len();
    let suffix_id = min(
        NUM_SUFFIX.len(),
        (digit_len as f32 / 3 as f32).ceil() as usize - 1,
    );
    let a = a / 10.0_f64.powi(3 * suffix_id as i32);
    let repr = if digit_len % 3 == 1 {
        format!("{:.1}", a).trim_end_matches(".0").to_string()
    } else {
        format!("{:.0}", a)
    };
    repr + NUM_SUFFIX[suffix_id]
}

fn outcome_stub(outcome_desc: &String) -> Outcome {
    Outcome {
        desc: outcome_desc.clone(),
        wagers: Vec::new(),
    }
}

pub fn bet_stub(outcomes_desc: &Vec<String>) -> Bet {
    Bet {
        bet: 0,
        desc: String::new(),
        outcomes: outcomes_desc.iter().map(outcome_stub).collect(),
        server: 0,
        author: 0,
        is_open: true
    }
}

fn outcome_display(desc: &str, percent: u64, odd: f32, sum: u32, people: u32) -> String {
    format!(
        "## > {}\n` {: >3}%  | {: >6} 🏆  {: >4} {}  {: >4} 👥 `",
        desc.trim().trim_start_matches("#"),
        percent,
        "1:".to_string() + &number_display(if odd.is_nan() { 1. } else { odd }),
        number_display(sum),
        config.currency,
        number_display(people)
    )
}

pub fn outcomes_display(bet_status: &Bet) -> Vec<String> {
    let sums: Vec<u64> = bet_status
        .outcomes
        .iter()
        .map(|outcome| {
            outcome
                .wagers
                .iter()
                .fold(0, |init, (_, amount)| init + amount)
        })
        .collect();

    let total = sums.iter().fold(0, |init, sum| init + *sum);

    let percents = lrm(100, &sums);

    let odds: Vec<f32> = sums.iter().map(|sum| total as f32 / *sum as f32).collect();

    let peoples: Vec<usize> = bet_status
        .outcomes
        .iter()
        .map(|outcome| outcome.wagers.len())
        .collect();

    itertools::izip!(&bet_status.outcomes, percents, odds, sums, peoples)
        .map(|(outcome, percent, odd, sum, people)| {
            outcome_display(&outcome.desc, percent, odd, sum as u32, people as u32)
        })
        .collect()
}

pub fn shorten(text: &str, length: usize) -> String {
    let res = text.split_once("\n").and_then(|(first, _)| Some(first)).unwrap_or(text);
    if res.len() > length {
        res.split_at(length-1).0.to_owned() + "…"
    } else {
        res.to_owned()
    }
}