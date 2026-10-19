/// `16450` as `16,450`.
pub fn thousands(value: usize) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// `~1,234`, or `−~1,234` when negative. The sign goes first because `~−`
/// looks like a double tilde.
pub fn approximately(value: i64) -> String {
    match value < 0 {
        true => format!("−~{}", thousands(value.unsigned_abs() as usize)),
        false => format!("~{}", thousands(value as usize)),
    }
}

/// Cents below $100, whole dollars above.
pub fn money(amount: f64) -> String {
    let sign = if amount <= -0.005 { "−" } else { "" };
    let amount = amount.abs();
    match amount < 100.0 {
        true => format!("{sign}${amount:.2}"),
        false => format!("{sign}${}", thousands(amount.round() as usize)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_grouped_for_comparing() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_000), "1,000");
        assert_eq!(thousands(16_450), "16,450");
        assert_eq!(thousands(1_234_567), "1,234,567");
    }

    #[test]
    fn a_loss_reads_as_one() {
        assert_eq!(money(-2.5), "−$2.50");
        assert_eq!(approximately(-134_369), "−~134,369");
        assert_eq!(approximately(2_714), "~2,714");
    }

    #[test]
    fn money_is_cents_when_small_and_dollars_when_not() {
        assert_eq!(money(0.0), "$0.00");
        assert_eq!(money(0.004), "$0.00");
        assert_eq!(money(-0.001), "$0.00");
        assert_eq!(money(12.346), "$12.35");
        assert_eq!(money(1_234.4), "$1,234");
    }
}
