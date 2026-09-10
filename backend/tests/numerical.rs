use polyntu::amm::{Side, calculate};
use serde::Deserialize;
#[derive(Deserialize)]
struct Case {
    inventory: Vec<i64>,
    liquidity: i64,
    outcome: usize,
    side: Side,
    quantity: i64,
    amount_micros: i64,
}
#[test]
fn matches_independent_eighty_digit_decimal_reference() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/lmsr-reference.json")).unwrap();
    assert!(cases.len() >= 200);
    for (i, c) in cases.iter().enumerate() {
        let value = calculate(&c.inventory, c.liquidity, c.outcome, c.side, c.quantity).unwrap();
        assert_eq!(value.amount_micros, c.amount_micros, "reference case {i}");
    }
}
