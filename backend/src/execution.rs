//! TradingControl: signed quotes and atomic trade execution.
//!
//! A quote is a read-only preview signed with HMAC; execution re-validates
//! every signed claim inside one database transaction under the documented
//! lock order (idempotency row -> instance row -> sorted accounts), so a stale
//! or tampered quote can only be rejected, never executed at a wrong price.
use crate::{
    amm::{self, Side},
    auth,
    error::{Error, Result, conflict, invalid},
    events, fee,
    market::Instance,
    store::{Store, balance, db_now, lock_accounts, transfer},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgConnection, types::Json};
use std::time::Duration;
use uuid::Uuid;

/// How long a signed quote stays executable, capped at the market close.
pub const QUOTE_TTL_MS: i64 = 15_000;
/// Total attempts for transient lock/serialization failures.
const MAX_ATTEMPTS: u32 = 3;

/// A participant's preview request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuoteRequest {
    pub instance_id: String,
    pub outcome_id: String,
    pub side: Side,
    pub quantity_millis: i64,
}

/// The signed body of a quote token. Clients treat the token as opaque.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuoteClaims {
    pub account_id: String,
    pub instance_id: String,
    pub outcome_id: String,
    pub side: Side,
    pub quantity_millis: i64,
    /// Exact rounded LMSR amount, excluding the fee.
    pub amount_micros: i64,
    /// Fee on `amount_micros`; zero on fee-free markets.
    pub fee_micros: i64,
    pub instance_version: i64,
    pub expires_ms: i64,
    pub engine_version: String,
}

/// A confirmed trade: the signed quote plus the trader's all-in bound.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TradeRequest {
    pub quote_token: String,
    pub limit_micros: String,
}

/// Parse a nonnegative microcredit string of 1–16 ASCII digits.
pub fn parse_micros(value: &str) -> Result<i64> {
    if value.is_empty() || value.len() > 16 || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid("Amounts are 1–16 digit nonnegative integers"));
    }
    value
        .parse()
        .map_err(|_| invalid("Amounts are 1–16 digit nonnegative integers"))
}

fn valid_key(key: &str) -> bool {
    (1..=120).contains(&key.len()) && key.bytes().all(|b| b.is_ascii_graphic())
}

fn side_name(side: Side) -> &'static str {
    match side {
        Side::Buy => "buy",
        Side::Sell => "sell",
    }
}

/// Millishares as a human share count: 10000 -> "10", 1500 -> "1.5".
fn shares(millis: i64) -> String {
    let whole = millis / amm::SHARE_SCALE;
    let fraction = millis % amm::SHARE_SCALE;
    if fraction == 0 {
        whole.to_string()
    } else {
        format!("{whole}.{fraction:03}")
            .trim_end_matches('0')
            .to_string()
    }
}

fn check_quantity(quantity: i64) -> Result<()> {
    if !(1..=amm::MAX_QUANTITY).contains(&quantity) {
        return Err(invalid("Choose a valid outcome and 0.001–100 shares"));
    }
    Ok(())
}

/// A sale must be covered by the current holding. Checked before the engine
/// runs, so an uncovered sale reports the holding instead of an engine range.
fn check_holding(owned: i64, quantity: i64) -> Result<()> {
    if owned <= 0 {
        return Err(conflict("You do not own shares in this outcome"));
    }
    if quantity > owned {
        return Err(Error::Conflict(format!(
            "You own {} shares in this outcome; sell at most that quantity",
            shares(owned)
        )));
    }
    Ok(())
}

fn creator_ban(instance: &Instance, account: &str) -> Result<()> {
    if instance.creator_account_id.as_deref() == Some(account) {
        return Err(conflict("Creators cannot trade in their own markets"));
    }
    Ok(())
}

/// Engine amount and fee for one trade on this instance.
fn price(instance: &Instance, outcome: usize, side: Side, quantity: i64) -> Result<Priced> {
    let calculation = amm::calculate(
        &instance.inventory,
        instance.liquidity_units,
        outcome,
        side,
        quantity,
    )?;
    let fee_micros = if instance.fee_charged {
        fee::trade_fee(calculation.amount_micros)
    } else {
        0
    };
    let all_in = fee::charged(
        calculation.amount_micros,
        side == Side::Buy,
        instance.fee_charged,
    )?;
    Ok(Priced {
        calculation,
        fee_micros,
        all_in,
    })
}

struct Priced {
    calculation: amm::Calculation,
    fee_micros: i64,
    all_in: i64,
}

fn retryable(error: &Error) -> bool {
    match error {
        Error::Database(sqlx::Error::Database(db)) => {
            matches!(db.code().as_deref(), Some("40001" | "40P01" | "55P03"))
        }
        _ => false,
    }
}

/// What one execution attempt produced.
enum Outcome {
    /// A new trade committed; the cache can apply its effect.
    Executed {
        receipt: Value,
        instance_id: String,
        version: i64,
        inventory: Vec<i64>,
    },
    /// The stored receipt of an earlier identical request.
    Replayed(Value),
}

async fn position(
    connection: &mut PgConnection,
    account: &str,
    instance: &str,
    outcome: usize,
) -> Result<i64> {
    let owned: Option<i64> = sqlx::query_scalar(
        "SELECT quantity_millis FROM positions WHERE account_id=$1 AND instance_id=$2 AND outcome_index=$3",
    )
    .bind(account)
    .bind(instance)
    .bind(outcome as i32)
    .fetch_optional(connection)
    .await?;
    Ok(owned.unwrap_or(0))
}

impl Store {
    /// Issue a signed, read-only quote. Changes no database state and does not
    /// guarantee later execution.
    pub async fn quote(&self, account: &str, req: &QuoteRequest, secret: &str) -> Result<Value> {
        check_quantity(req.quantity_millis)?;
        let instance = self.instance_cached(&req.instance_id).await?;
        let now = self.now_cached();
        if !instance.tradable(now) {
            return Err(conflict("This market is not open for trading"));
        }
        creator_ban(&instance, account)?;
        let outcome = instance.outcome_index(&req.outcome_id)?;
        if req.side == Side::Sell {
            let owned = position(
                &mut *self.pool.acquire().await?,
                account,
                &instance.id,
                outcome,
            )
            .await?;
            check_holding(owned, req.quantity_millis)?;
        }
        let priced = price(&instance, outcome, req.side, req.quantity_millis)?;
        let claims = QuoteClaims {
            account_id: account.to_string(),
            instance_id: instance.id.clone(),
            outcome_id: req.outcome_id.clone(),
            side: req.side,
            quantity_millis: req.quantity_millis,
            amount_micros: priced.calculation.amount_micros,
            fee_micros: priced.fee_micros,
            instance_version: instance.version,
            expires_ms: (now + QUOTE_TTL_MS).min(instance.close_ms),
            engine_version: amm::ENGINE_VERSION.to_string(),
        };
        let token = auth::sign(&claims, secret.as_bytes())?;
        let average_price = priced.all_in as f64 / (req.quantity_millis * 1_000) as f64;
        Ok(json!({
            "quote_token": token,
            "instance_id": claims.instance_id,
            "outcome_id": claims.outcome_id,
            "side": side_name(claims.side),
            "quantity_millis": claims.quantity_millis,
            "amount_micros": priced.all_in.to_string(),
            "fee_micros": priced.fee_micros.to_string(),
            "engine_amount_micros": priced.calculation.amount_micros.to_string(),
            "average_price": average_price,
            "price_before": priced.calculation.prices_before[outcome],
            "price_after": priced.calculation.prices_after[outcome],
            "prices_after": priced.calculation.prices_after,
            "instance_version": claims.instance_version,
            "expires_ms": claims.expires_ms,
            "server_time_ms": now,
        }))
    }

    /// Execute a signed quote exactly once per (account, idempotency key).
    pub async fn execute(
        &self,
        account: &str,
        key: &str,
        req: &TradeRequest,
        secret: &str,
    ) -> Result<Value> {
        if !valid_key(key) {
            return Err(invalid(
                "Idempotency-Key must be 1–120 visible ASCII characters",
            ));
        }
        let claims: QuoteClaims = auth::verify(&req.quote_token, secret.as_bytes())
            .map_err(|_| invalid("The quote token is invalid; preview a new quote"))?;
        if claims.account_id != account {
            return Err(invalid("This quote was issued to another account"));
        }
        if claims.engine_version != amm::ENGINE_VERSION {
            return Err(conflict(
                "This quote was priced by another engine version; preview again",
            ));
        }
        let limit = parse_micros(&req.limit_micros)?;
        let encoded = serde_json::to_vec(req).map_err(|e| Error::Internal(e.to_string()))?;
        let request_hash = auth::hash(&encoded);
        let mut attempt = 1;
        loop {
            match self
                .execute_once(account, key, &request_hash, &claims, limit)
                .await
            {
                Ok(Outcome::Executed {
                    receipt,
                    instance_id,
                    version,
                    inventory,
                }) => {
                    self.cache.apply_trade(&instance_id, version, &inventory);
                    return Ok(receipt);
                }
                Ok(Outcome::Replayed(receipt)) => return Ok(receipt),
                Err(e) if retryable(&e) => {
                    if attempt >= MAX_ATTEMPTS {
                        return Err(conflict(
                            "The market is busy; retry the same request with the same key",
                        ));
                    }
                    tokio::time::sleep(Duration::from_millis(20 * u64::from(attempt))).await;
                    attempt += 1;
                }
                Err(e) => return Err(e),
            }
        }
    }

    async fn execute_once(
        &self,
        account: &str,
        key: &str,
        request_hash: &str,
        claims: &QuoteClaims,
        limit: i64,
    ) -> Result<Outcome> {
        let mut tx = self.pool.begin().await?;

        // 1. Claim the idempotency key. A concurrent insert of the same key
        // waits here until the other transaction commits or rolls back.
        let claimed = sqlx::query(
            "INSERT INTO idempotency(account_id,key,request_hash) VALUES($1,$2,$3) ON CONFLICT DO NOTHING",
        )
        .bind(account)
        .bind(key)
        .bind(request_hash)
        .execute(&mut *tx)
        .await?
        .rows_affected()
            == 1;
        if !claimed {
            let existing: Option<(String, Option<Json<Value>>)> = sqlx::query_as(
                "SELECT request_hash,response FROM idempotency WHERE account_id=$1 AND key=$2 FOR UPDATE",
            )
            .bind(account)
            .bind(key)
            .fetch_optional(&mut *tx)
            .await?;
            return match existing {
                Some((hash, _)) if hash != request_hash => Err(conflict(
                    "This idempotency key was already used for a different request",
                )),
                Some((_, Some(Json(response)))) => {
                    tx.commit().await?;
                    Ok(Outcome::Replayed(response))
                }
                _ => Err(conflict(
                    "This trade is still in progress; retry the same request with the same key",
                )),
            };
        }

        // 2. Lock the instance; this serializes inventory and position updates.
        let instance: Instance = sqlx::query_as("SELECT * FROM instances WHERE id=$1 FOR UPDATE")
            .bind(&claims.instance_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(Error::NotFound)?;

        // 3. Read the clock in its own statement, after the lock wait.
        let now = db_now(&mut tx).await?;
        if now >= claims.expires_ms {
            return Err(conflict("This quote has expired; preview a new quote"));
        }
        if !instance.tradable(now) {
            return Err(conflict("This market is not open for trading"));
        }
        creator_ban(&instance, account)?;
        if claims.instance_version != instance.version {
            return Err(conflict("The market moved since this quote; preview again"));
        }
        let outcome = instance.outcome_index(&claims.outcome_id)?;
        check_quantity(claims.quantity_millis)?;

        // 4. Lock the participant and reserve accounts, then read state.
        lock_accounts(
            &mut tx,
            &[account.to_string(), instance.reserve_account_id.clone()],
        )
        .await?;
        let trader_balance = balance(&mut tx, account).await?;
        let reserve_balance = balance(&mut tx, &instance.reserve_account_id).await?;
        let owned = position(&mut tx, account, &instance.id, outcome).await?;
        if claims.side == Side::Sell {
            check_holding(owned, claims.quantity_millis)
                .map_err(|_| conflict("You do not own enough shares to sell this quantity"))?;
        }

        let priced = price(&instance, outcome, claims.side, claims.quantity_millis)?;
        if priced.calculation.amount_micros != claims.amount_micros
            || priced.fee_micros != claims.fee_micros
        {
            return Err(conflict(
                "The price changed since this quote; preview again",
            ));
        }
        let all_in = priced.all_in;
        let (owned_after, reserve_after) = match claims.side {
            Side::Buy => {
                if all_in > limit {
                    return Err(conflict("The cost exceeds your price limit"));
                }
                if trader_balance < all_in {
                    return Err(conflict("Insufficient balance for this trade"));
                }
                (owned + claims.quantity_millis, reserve_balance + all_in)
            }
            Side::Sell => {
                if all_in < limit {
                    return Err(conflict("The proceeds are below your price limit"));
                }
                (owned - claims.quantity_millis, reserve_balance - all_in)
            }
        };
        let inventory = priced.calculation.inventory;
        let liability = inventory.iter().copied().max().unwrap_or(0) * 1_000;
        if reserve_after < liability {
            return Err(Error::Internal(
                "Trade would leave the reserve below its maximum liability".into(),
            ));
        }

        // 5. Effects: ledger, position, inventory/version, trade, receipt, event.
        let trade_id = Uuid::new_v4().to_string();
        let version = instance.version + 1;
        let (from, to) = match claims.side {
            Side::Buy => (account, instance.reserve_account_id.as_str()),
            Side::Sell => (instance.reserve_account_id.as_str(), account),
        };
        transfer(
            &mut tx,
            from,
            to,
            all_in,
            side_name(claims.side),
            &format!("trade:{trade_id}"),
            now,
        )
        .await?;
        sqlx::query(
            "INSERT INTO positions(account_id,instance_id,outcome_index,quantity_millis) VALUES($1,$2,$3,$4) \
             ON CONFLICT (account_id,instance_id,outcome_index) DO UPDATE SET quantity_millis=EXCLUDED.quantity_millis",
        )
        .bind(account)
        .bind(&instance.id)
        .bind(outcome as i32)
        .bind(owned_after)
        .execute(&mut *tx)
        .await?;
        let updated =
            sqlx::query("UPDATE instances SET inventory=$2,version=$3 WHERE id=$1 AND version=$4")
                .bind(&instance.id)
                .bind(&inventory)
                .bind(version)
                .bind(instance.version)
                .execute(&mut *tx)
                .await?;
        if updated.rows_affected() != 1 {
            return Err(Error::Internal(
                "Locked instance changed during execution".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO trades(id,account_id,instance_id,outcome_index,side,quantity_millis,amount_micros,fee_micros,instance_version,engine_version,created_ms) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
        )
        .bind(&trade_id)
        .bind(account)
        .bind(&instance.id)
        .bind(outcome as i32)
        .bind(side_name(claims.side))
        .bind(claims.quantity_millis)
        .bind(all_in)
        .bind(priced.fee_micros)
        .bind(version)
        .bind(amm::ENGINE_VERSION)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        let balance_after = balance(&mut tx, account).await?;
        let receipt = json!({
            "trade_id": trade_id,
            "instance_id": instance.id,
            "outcome_id": claims.outcome_id,
            "outcome_index": outcome,
            "side": side_name(claims.side),
            "quantity_millis": claims.quantity_millis,
            "amount_micros": all_in.to_string(),
            "fee_micros": priced.fee_micros.to_string(),
            "balance_micros": balance_after.to_string(),
            "owned_millis": owned_after,
            "instance_version": version,
            "created_ms": now,
        });
        sqlx::query("UPDATE idempotency SET response=$3 WHERE account_id=$1 AND key=$2")
            .bind(account)
            .bind(key)
            .bind(Json(&receipt))
            .execute(&mut *tx)
            .await?;
        // The trade notification carries the new inventory so other
        // processes' caches can apply it without a database read.
        sqlx::query(
            "WITH o AS (INSERT INTO outbox(instance_id,instance_version,event_type,created_ms) VALUES($1,$2,$3,$4) RETURNING id) \
             SELECT pg_notify($5, json_build_object('id',o.id,'instance_id',$1::TEXT,'version',$2::BIGINT,'type',$3::TEXT,'created_ms',$4::BIGINT,'inventory',$6::BIGINT[])::text) FROM o",
        )
        .bind(&instance.id)
        .bind(version)
        .bind("trade")
        .bind(now)
        .bind(events::CHANNEL)
        .bind(&inventory)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Outcome::Executed {
            receipt,
            instance_id: instance.id,
            version,
            inventory,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn micros_parse_only_bounded_digit_strings() {
        assert_eq!(parse_micros("0").unwrap(), 0);
        assert_eq!(parse_micros("5137761").unwrap(), 5_137_761);
        assert_eq!(
            parse_micros("9999999999999999").unwrap(),
            9_999_999_999_999_999
        );
        for bad in ["", "-1", "+1", "1.5", " 1", "12345678901234567", "1e3"] {
            assert!(parse_micros(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn share_counts_format_without_trailing_zeros() {
        assert_eq!(shares(10_000), "10");
        assert_eq!(shares(1_500), "1.5");
        assert_eq!(shares(1), "0.001");
        assert_eq!(shares(12_340), "12.34");
    }

    #[test]
    fn idempotency_keys_are_bounded_visible_ascii() {
        assert!(valid_key("a"));
        assert!(valid_key(&"k".repeat(120)));
        assert!(!valid_key(""));
        assert!(!valid_key(&"k".repeat(121)));
        assert!(!valid_key("has space"));
    }
}
