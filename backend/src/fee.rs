//! Trading-fee policy. Fees are charged on top of the pure LMSR amounts and
//! accumulate inside the instance reserve until settlement, where the pot is
//! split between the platform treasury and the recorded market creator. The
//! AMM engine itself stays fee-free so its numerical contract is unchanged.
use crate::error::{Result, invalid};

/// Fee rate in basis points of the engine amount, charged on every trade.
pub const FEE_BPS: i64 = 25;

/// Rounding is always against the trader, so the fee can never exceed the
/// engine amount it is derived from and a sale never credits below zero.
pub fn trade_fee(amount_micros: i64) -> i64 {
    let scaled = amount_micros * FEE_BPS;
    let (quotient, remainder) = (scaled / 10_000, scaled % 10_000);
    if remainder > 0 {
        quotient + 1
    } else {
        quotient
    }
}

/// The creator's half of a settled fee pot; the treasury keeps the remainder,
/// including any odd microcredit. `None` creator means the full pot is treasury
/// revenue, so the share is zero.
pub fn creator_share(pot_micros: i64, has_creator: bool) -> i64 {
    if has_creator { pot_micros / 2 } else { 0 }
}

/// All-in amount a trader debits (buy) or credits (sell) for one trade.
/// Fee-free markets (chosen at creation, like the welfare bus series) trade
/// at the pure engine amount.
pub fn charged(amount_micros: i64, buy: bool, fee_charged: bool) -> Result<i64> {
    if !fee_charged {
        return Ok(amount_micros);
    }
    let fee = trade_fee(amount_micros);
    if buy {
        amount_micros.checked_add(fee)
    } else {
        amount_micros.checked_sub(fee)
    }
    .filter(|total| *total >= 0)
    .ok_or_else(|| invalid("Fee is outside the supported range"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fee_rounds_up_and_never_exceeds_the_amount() {
        assert_eq!(trade_fee(0), 0);
        assert_eq!(trade_fee(1), 1);
        assert_eq!(trade_fee(4_000), 10);
        assert_eq!(trade_fee(5_124_948), 12_813);
        for amount in [1, 2, 3, 7, 100, 9_999, 10_000, 123_457, 100_000_000] {
            assert!(trade_fee(amount) <= amount);
        }
    }

    #[test]
    fn charged_amounts_stay_nonnegative_and_creator_split_sums() {
        assert_eq!(charged(5_124_948, true, true).unwrap(), 5_137_761);
        assert_eq!(charged(5_124_948, false, true).unwrap(), 5_112_135);
        assert_eq!(charged(0, false, true).unwrap(), 0);
        assert_eq!(charged(1, false, true).unwrap(), 0);
        // Fee-free markets trade at the pure engine amount.
        assert_eq!(charged(5_124_948, true, false).unwrap(), 5_124_948);
        assert_eq!(charged(5_124_948, false, false).unwrap(), 5_124_948);
        let pot = 12_813;
        assert_eq!(
            creator_share(pot, true) + (pot - creator_share(pot, true)),
            pot
        );
        assert_eq!(creator_share(pot, false), 0);
        assert_eq!(creator_share(1, true), 0);
    }
}
