//! TradingControl: signed quotes and atomic, idempotent trade execution.
//!
//! A quote is a pure preview signed with HMAC. Execution re-validates every
//! signed claim under the instance and account locks, moves the all-in amount
//! (engine amount plus fee) through one ledger transfer, and stores the
//! receipt in the idempotency row inside the same transaction.
use crate::{
    amm::{self, ENGINE_VERSION, Side},
    auth,
    error::{Error, Result, conflict, invalid},
    events, fee,
    market::Instance,
    store::{Store, balance, db_now, lock_accounts, transfer},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgConnection;
use std::time::Duration;
use uuid::Uuid;

/// Longest validity of a signed quote.
pub const QUOTE_TTL_MS: i64 = 15_000;
/// Total attempts for transient lock/serialization failures.
const MAX_ATTEMPTS: usize = 3;
/// Microcredits paid per winning millishare.
const MICROS_PER_MILLISHARE: i64 = amm::CREDIT_SCALE / amm::SHARE_SCALE;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuoteRequest {
    pub instance_id: String,
    pub outcome_id: String,
    pub side: Side,
    pub quantity_millis: i64,
}

/// The signed body of a quote token. `amount_micros` is the all-in amount
/// (engine amount plus fee for a buy, minus fee for a sale), exactly as the
/// quote response reports it; `fee_micros` states the fee separately.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuoteClaims {
    pub account_id: String,
    pub instance_id: String,
    pub outcome_id: String,
    pub side: Side,
    pub quantity_millis: i64,
    pub amount_micros: i64,
    pub fee_micros: i64,
    pub version: i64,
    pub expires_ms: i64,
    pub engine: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TradeRequest {
    pub quote_token: String,
    pub limit_micros: String,
}

/// Nonnegative microcredit amount from 1–16 ASCII digits.
pub fn parse_micros(value: &str) -> Result<i64> {
    if value.is_empty() || value.len() > 16 || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid("Amounts must be 1–16 decimal digits"));
    }
    value
        .parse()
        .map_err(|_| invalid("Amounts must be 1–16 decimal digits"))
}

fn valid_key(key: &str) -> bool {
    (1..=120).contains(&key.len())
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Millishares as a share count without trailing zeros: 10000 -> "10".
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

fn holding_error(owned: i64) -> Error {
    if owned <= 0 {
        conflict("You do not own shares in this outcome")
    } else {
        Error::Conflict(format!(
            "You own {} shares in this outcome; sell at most that many",
            shares(owned)
        ))
    }
}

const CREATOR_BAN: &str = "Creators cannot trade in their own markets";

/// Engine amount, fee, and all-in amount for one trade on one instance state.
struct Priced {
    calculation: amm::Calculation,
    fee_micros: i64,
    total_micros: i64,
}

fn price(instance: &Instance, outcome: usize, side: Side, quantity: i64) -> Result<Priced> {
    let calculation = amm::calculate(
        &instance.inventory,
        instance.liquidity_units,
        outcome,
        side,
        quantity,
    )?;
    let total_micros = fee::charged(
        calculation.amount_micros,
        side == Side::Buy,
        instance.fee_charged,
    )?;
    let fee_micros = (total_micros - calculation.amount_micros).abs();
    Ok(Priced {
        calculation,
        fee_micros,
        total_micros,
    })
}

fn is_transient(error: &Error) -> bool {
    match error {
        Error::Database(sqlx::Error::Database(db)) => {
            matches!(db.code().as_deref(), Some("40001" | "40P01" | "55P03"))
        }
        _ => false,
    }
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
    /// Read-only signed preview of one trade.
    pub async fn quote(&self, account: &str, req: &QuoteRequest, secret: &str) -> Result<Value> {
        let instance = self.instance_cached(&req.instance_id).await?;
        let now = self.now_cached();
        if !instance.tradable(now) {
            return Err(conflict("This market is not open for trading"));
        }
        if instance.creator_account_id.as_deref() == Some(account) {
            return Err(conflict(CREATOR_BAN));
        }
        let outcome = instance.outcome_index(&req.outcome_id)?;
        if req.side == Side::Sell {
            let owned = position(
                &mut *self.pool.acquire().await?,
                account,
                &instance.id,
                outcome,
            )
            .await?;
            if owned <= 0 || req.quantity_millis > owned {
                return Err(holding_error(owned));
            }
        }
        let priced = price(&instance, outcome, req.side, req.quantity_millis)?;
        let claims = QuoteClaims {
            account_id: account.to_string(),
            instance_id: instance.id.clone(),
            outcome_id: req.outcome_id.clone(),
            side: req.side,
            quantity_millis: req.quantity_millis,
            amount_micros: priced.total_micros,
            fee_micros: priced.fee_micros,
            version: instance.version,
            expires_ms: (now + QUOTE_TTL_MS).min(instance.close_ms),
            engine: ENGINE_VERSION.to_string(),
        };
        let token = auth::sign(&claims, secret.as_bytes())?;
        let average_price = priced.total_micros as f64
            / (req.quantity_millis as f64 * MICROS_PER_MILLISHARE as f64);
        Ok(json!({
            "quote_token": token,
            "instance_id": claims.instance_id,
            "outcome_id": claims.outcome_id,
            "side": claims.side,
            "quantity_millis": claims.quantity_millis,
            "amount_micros": claims.amount_micros.to_string(),
            "fee_micros": claims.fee_micros.to_string(),
            "version": claims.version,
            "expires_ms": claims.expires_ms,
            "server_time_ms": now,
            "average_price": average_price,
            "price_before": priced.calculation.prices_before[outcome],
            "price_after": priced.calculation.prices_after[outcome],
            "prices_before": priced.calculation.prices_before,
            "prices_after": priced.calculation.prices_after,
        }))
    }

    /// Execute a signed quote idempotently under the account-scoped key.
    pub async fn execute(
        &self,
        account: &str,
        key: &str,
        req: &TradeRequest,
        secret: &str,
    ) -> Result<Value> {
        if !valid_key(key) {
            return Err(invalid(
                "Idempotency-Key must be 1–120 letters, digits, hyphens, or underscores",
            ));
        }
        let claims: QuoteClaims = auth::verify(&req.quote_token, secret.as_bytes())?;
        if claims.account_id != account {
            return Err(conflict("This quote was issued to a different account"));
        }
        if claims.engine != ENGINE_VERSION {
            return Err(conflict(
                "This quote is from an incompatible engine; preview again",
            ));
        }
        let limit = parse_micros(&req.limit_micros)?;
        let body = serde_json::to_vec(req).map_err(|e| Error::Internal(e.to_string()))?;
        let request_hash = auth::hash(&body);
        let mut attempt = 0;
        loop {
            attempt += 1;
            match self
                .execute_once(account, key, &request_hash, &claims, limit)
                .await
            {
                Err(error) if is_transient(&error) => {
                    if attempt >= MAX_ATTEMPTS {
                        return Err(conflict(
                            "The market is busy; retry the same request with the same key",
                        ));
                    }
                    tokio::time::sleep(Duration::from_millis(20 * attempt as u64)).await;
                }
                other => return other,
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
    ) -> Result<Value> {
        let mut tx = self.pool.begin().await?;

        // 1. Claim the idempotency key, or replay / reject an existing claim.
        // The insert waits on an in-flight claim for the same key, so after it
        // returns the other transaction has either committed or rolled back.
        let inserted = sqlx::query(
            "INSERT INTO idempotency(account_id,key,request_hash) VALUES($1,$2,$3) ON CONFLICT DO NOTHING",
        )
        .bind(account)
        .bind(key)
        .bind(request_hash)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if inserted == 0 {
            let existing: Option<(String, Option<sqlx::types::Json<Value>>)> = sqlx::query_as(
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
                Some((_, Some(response))) => Ok(response.0),
                _ => Err(conflict(
                    "This request is still in progress; retry with the same key",
                )),
            };
        }

        // 2. Lock the instance; every trade on it serializes here.
        let instance: Instance = sqlx::query_as("SELECT * FROM instances WHERE id=$1 FOR UPDATE")
            .bind(&claims.instance_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(Error::NotFound)?;

        // 3. Read the clock only after the lock wait, in its own statement.
        let now = db_now(&mut tx).await?;
        if now >= claims.expires_ms {
            return Err(conflict("This quote has expired; preview again"));
        }
        if !instance.tradable(now) {
            return Err(conflict("This market is not open for trading"));
        }
        if instance.creator_account_id.as_deref() == Some(account) {
            return Err(conflict(CREATOR_BAN));
        }
        if claims.version != instance.version {
            return Err(conflict("The price changed; preview again"));
        }
        let outcome = instance.outcome_index(&claims.outcome_id)?;
        let priced = price(&instance, outcome, claims.side, claims.quantity_millis)?;
        if priced.total_micros != claims.amount_micros || priced.fee_micros != claims.fee_micros {
            return Err(conflict("The price changed; preview again"));
        }
        let buy = claims.side == Side::Buy;
        if buy && priced.total_micros > limit {
            return Err(conflict("The cost exceeds your limit"));
        }
        if !buy && priced.total_micros < limit {
            return Err(conflict("The proceeds are below your limit"));
        }

        // 4. Lock trader and reserve, then check funds and holdings.
        let reserve = instance.reserve_account_id.clone();
        lock_accounts(&mut tx, &[account.to_string(), reserve.clone()]).await?;
        let trader_balance = balance(&mut tx, account).await?;
        let reserve_balance = balance(&mut tx, &reserve).await?;
        let owned = position(&mut tx, account, &instance.id, outcome).await?;
        if buy && trader_balance < priced.total_micros {
            return Err(conflict("Insufficient units for this trade"));
        }
        if !buy && claims.quantity_millis > owned {
            return Err(holding_error(owned));
        }
        let inventory = priced.calculation.inventory.clone();
        let liability = inventory.iter().copied().max().unwrap_or(0) * MICROS_PER_MILLISHARE;
        let reserve_after = if buy {
            reserve_balance + priced.total_micros
        } else {
            reserve_balance - priced.total_micros
        };
        if reserve_after < liability {
            return Err(conflict("The market reserve cannot cover this trade"));
        }

        // 5. Ledger movement of the all-in amount; the trigger updates balances.
        let trade_id = Uuid::new_v4().to_string();
        let (from, to, kind) = if buy {
            (account, reserve.as_str(), "buy")
        } else {
            (reserve.as_str(), account, "sell")
        };
        transfer(
            &mut tx,
            from,
            to,
            priced.total_micros,
            kind,
            &format!("trade:{trade_id}"),
            now,
        )
        .await?;

        // 6. Position, inventory/version, trade, receipt, and outbox event.
        let owned_after: i64 = if buy {
            sqlx::query_scalar(
                "INSERT INTO positions(account_id,instance_id,outcome_index,quantity_millis) VALUES($1,$2,$3,$4) \
                 ON CONFLICT(account_id,instance_id,outcome_index) DO UPDATE SET quantity_millis=positions.quantity_millis+EXCLUDED.quantity_millis \
                 RETURNING quantity_millis",
            )
        } else {
            sqlx::query_scalar(
                "UPDATE positions SET quantity_millis=quantity_millis-$4 \
                 WHERE account_id=$1 AND instance_id=$2 AND outcome_index=$3 RETURNING quantity_millis",
            )
        }
        .bind(account)
        .bind(&instance.id)
        .bind(outcome as i32)
        .bind(claims.quantity_millis)
        .fetch_one(&mut *tx)
        .await?;
        let version: i64 = sqlx::query_scalar(
            "UPDATE instances SET inventory=$2, version=version+1 WHERE id=$1 RETURNING version",
        )
        .bind(&instance.id)
        .bind(&inventory)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO trades(id,account_id,instance_id,outcome_index,side,quantity_millis,amount_micros,instance_version,engine_version,created_ms,fee_micros) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)")
            .bind(&trade_id).bind(account).bind(&instance.id).bind(outcome as i32).bind(kind)
            .bind(claims.quantity_millis).bind(priced.total_micros).bind(version).bind(ENGINE_VERSION)
            .bind(now).bind(priced.fee_micros)
            .execute(&mut *tx).await?;
        let balance_after = balance(&mut tx, account).await?;
        let receipt = json!({
            "trade_id": trade_id,
            "instance_id": instance.id,
            "outcome_id": claims.outcome_id,
            "side": claims.side,
            "quantity_millis": claims.quantity_millis,
            "amount_micros": priced.total_micros.to_string(),
            "fee_micros": priced.fee_micros.to_string(),
            "balance_micros": balance_after.to_string(),
            "owned_millis": owned_after,
            "version": version,
            "created_ms": now,
        });
        sqlx::query("UPDATE idempotency SET response=$3 WHERE account_id=$1 AND key=$2")
            .bind(account)
            .bind(key)
            .bind(sqlx::types::Json(&receipt))
            .execute(&mut *tx)
            .await?;
        // Trade events carry the inventory so listening caches update in place.
        sqlx::query("WITH o AS (INSERT INTO outbox(instance_id,instance_version,event_type,created_ms) VALUES($1,$2,'trade',$3) RETURNING id) \
                     SELECT pg_notify($4, json_build_object('id',o.id,'instance_id',$1::TEXT,'version',$2::BIGINT,'type','trade'::TEXT,'created_ms',$3::BIGINT,'inventory',$5::BIGINT[])::text) FROM o")
            .bind(&instance.id).bind(version).bind(now).bind(events::CHANNEL).bind(&inventory)
            .execute(&mut *tx).await?;
        tx.commit().await?;
        self.cache.apply_trade(&instance.id, version, &inventory);
        Ok(receipt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn micros_parse_strictly() {
        assert_eq!(parse_micros("0").unwrap(), 0);
        assert_eq!(parse_micros("5137761").unwrap(), 5_137_761);
        assert_eq!(
            parse_micros("9999999999999999").unwrap(),
            9_999_999_999_999_999
        );
        for bad in ["", "-1", "+1", "1.0", " 1", "12345678901234567", "1e6"] {
            assert!(parse_micros(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn keys_and_share_formatting() {
        assert!(valid_key("same-request_1"));
        assert!(!valid_key(""));
        assert!(!valid_key("has space"));
        assert!(!valid_key(&"k".repeat(121)));
        assert_eq!(shares(10_000), "10");
        assert_eq!(shares(1_500), "1.5");
        assert_eq!(shares(1), "0.001");
    }
}
