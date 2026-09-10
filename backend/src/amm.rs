//! Pure LMSR engine. Balances are microcredits; quantities are millishares.
//! Decimal arithmetic is authoritative; floating point is display-only.
use crate::error::{Result, invalid};
use rust_decimal::{Decimal, MathematicalOps, prelude::ToPrimitive};
use serde::{Deserialize, Serialize};

pub const CREDIT_SCALE: i64 = 1_000_000;
pub const SHARE_SCALE: i64 = 1_000;
pub const MAX_QUANTITY: i64 = 100_000; // 100 shares per request
pub const MAX_INVENTORY: i64 = 1_000_000_000;
pub const ENGINE_VERSION: &str = "lmsr-decimal-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone)]
pub struct Calculation {
    pub amount_micros: i64,
    pub inventory: Vec<i64>,
    pub prices_before: Vec<f64>,
    pub prices_after: Vec<f64>,
}

fn exp(value: Decimal) -> Result<Decimal> {
    value
        .checked_exp_with_tolerance(Decimal::new(1, 25))
        .ok_or_else(|| invalid("Numerical range exceeded"))
}

pub fn validate(inventory: &[i64], liquidity: i64) -> Result<()> {
    if !(2..=8).contains(&inventory.len()) || !(10..=100_000).contains(&liquidity) {
        return Err(invalid(
            "Use 2–8 outcomes and liquidity between 10 and 100000 units",
        ));
    }
    if inventory.iter().any(|q| !(0..=MAX_INVENTORY).contains(q)) {
        return Err(invalid("Inventory is outside the supported range"));
    }
    let spread = inventory.iter().max().unwrap() - inventory.iter().min().unwrap();
    if spread > 20 * liquidity * SHARE_SCALE {
        return Err(invalid(
            "This trade exceeds the market's price concentration limit",
        ));
    }
    Ok(())
}

fn probabilities(inventory: &[i64], liquidity: i64) -> Result<Vec<Decimal>> {
    validate(inventory, liquidity)?;
    let maximum = *inventory.iter().max().unwrap();
    let denominator = Decimal::from(liquidity * SHARE_SCALE);
    let weights: Vec<Decimal> = inventory
        .iter()
        .map(|q| exp(Decimal::from(q - maximum) / denominator))
        .collect::<Result<_>>()?;
    let total: Decimal = weights.iter().sum();
    Ok(weights.into_iter().map(|w| w / total).collect())
}

pub fn prices(inventory: &[i64], liquidity: i64) -> Result<Vec<f64>> {
    Ok(probabilities(inventory, liquidity)?
        .iter()
        .map(|p| p.to_f64().expect("bounded probability"))
        .collect())
}

pub fn funding(liquidity: i64, outcomes: usize) -> Result<i64> {
    validate(&vec![0; outcomes], liquidity)?;
    let ln = Decimal::from(outcomes as i64)
        .checked_ln()
        .ok_or_else(|| invalid("Invalid outcome count"))?;
    // One extra microcredit protects the funding bound at a numerical boundary.
    (Decimal::from(liquidity * CREDIT_SCALE) * ln)
        .ceil()
        .to_i64()
        .and_then(|x| x.checked_add(1))
        .ok_or_else(|| invalid("Funding overflow"))
}

pub fn calculate(
    inventory: &[i64],
    liquidity: i64,
    outcome: usize,
    side: Side,
    quantity: i64,
) -> Result<Calculation> {
    validate(inventory, liquidity)?;
    if outcome >= inventory.len() || !(1..=MAX_QUANTITY).contains(&quantity) {
        return Err(invalid("Choose a valid outcome and 0.001–100 shares"));
    }
    let delta = if side == Side::Buy {
        quantity
    } else {
        -quantity
    };
    let mut next = inventory.to_vec();
    next[outcome] = next[outcome]
        .checked_add(delta)
        .ok_or_else(|| invalid("Quantity overflow"))?;
    validate(&next, liquidity)?;
    let probabilities = probabilities(inventory, liquidity)?;
    // This avoids subtracting two large cost-function values.
    let growth = exp(Decimal::from(delta) / Decimal::from(liquidity * SHARE_SCALE))?;
    let argument = Decimal::ONE + probabilities[outcome] * (growth - Decimal::ONE);
    let cost = Decimal::from(liquidity * CREDIT_SCALE)
        * argument
            .checked_ln()
            .ok_or_else(|| invalid("Cost is outside the supported range"))?;
    let rounded = if side == Side::Buy {
        cost.ceil()
    } else {
        (-cost).floor()
    };
    let amount_micros = rounded
        .to_i64()
        .filter(|x| *x >= 0)
        .ok_or_else(|| invalid("Cost overflow"))?;
    Ok(Calculation {
        amount_micros,
        inventory: next.clone(),
        prices_before: probabilities.iter().map(|p| p.to_f64().unwrap()).collect(),
        prices_after: prices(&next, liquidity)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn known_binary_quote_and_round_trip() {
        let bought = calculate(&[0, 0], 100, 0, Side::Buy, 10_000).unwrap();
        assert_eq!(bought.amount_micros, 5_124_948);
        let sold = calculate(&bought.inventory, 100, 0, Side::Sell, 10_000).unwrap();
        assert!(sold.amount_micros <= bought.amount_micros);
        assert!(bought.amount_micros - sold.amount_micros <= 1);
        assert_eq!(sold.inventory, vec![0, 0]);
    }

    #[test]
    fn invalid_ranges_and_sell_inventory_are_rejected() {
        assert!(calculate(&[0, 0], 100, 0, Side::Sell, 1).is_err());
        assert!(calculate(&[0, 0], 100, 2, Side::Buy, 1).is_err());
        assert!(calculate(&[0, 0], 100, 0, Side::Buy, i64::MAX).is_err());
        assert!(prices(&[0, 1_000_000], 10).is_err());
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(128))]
        #[test]
        fn normalized_monotone_funded_and_no_roundtrip_gain(
            outcomes in 2usize..9, selected in 0usize..8, quantity in 1i64..100_001,
            initial in 0i64..100_000, liquidity in 100i64..1000
        ) {
            let selected = selected % outcomes;
            let mut q = vec![initial; outcomes];
            let bought = calculate(&q, liquidity, selected, Side::Buy, quantity).unwrap();
            prop_assert!((bought.prices_after.iter().sum::<f64>() - 1.0).abs() < 1e-12);
            prop_assert!(bought.prices_after[selected] > bought.prices_before[selected]);
            let sold = calculate(&bought.inventory, liquidity, selected, Side::Sell, quantity).unwrap();
            prop_assert!(sold.amount_micros <= bought.amount_micros);
            let mut reserve = funding(liquidity, outcomes).unwrap() + initial * 1000;
            for i in 0..outcomes {
                let t = calculate(&q, liquidity, i, Side::Buy, quantity).unwrap();
                reserve += t.amount_micros;
                q = t.inventory;
                prop_assert!(reserve >= q.iter().max().unwrap() * 1000);
            }
        }
    }
}
