use crate::brain::Brain;
use anyhow::Result;

const KEY: &str = "price";

/// Dollars per million input tokens. A mid-tier model's rate, so the estimate
/// errs low for anyone on a bigger one.
pub const DEFAULTPRICE: f64 = 2.0;

pub async fn price(brain: &Brain) -> f64 {
    brain
        .preference(KEY)
        .await
        .ok()
        .flatten()
        .and_then(|value| value.trim().parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or(DEFAULTPRICE)
}

pub async fn setprice(brain: &Brain, dollars: f64) -> Result<()> {
    anyhow::ensure!(
        dollars.is_finite() && (0.0..=1_000.0).contains(&dollars),
        "expected dollars per million input tokens, between 0 and 1000"
    );
    brain.setpreference(KEY, &dollars.to_string()).await
}

pub fn dollars(tokens: i64, price: f64) -> f64 {
    tokens as f64 * price / 1_000_000.0
}
