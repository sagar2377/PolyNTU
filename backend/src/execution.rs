use crate::{
    amm::{self, Side},
    auth,
    error::{Error, Result, conflict, invalid},
    events, fee,
    market::Instance,
    store::{Store, transfer},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{FromRow, Row, types::Json};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuoteRequest {
    pub instance_id: String,
    pub outcome_id: String,
    pub side: Side,
    pub quantity_millis: i64,
}

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

pub fn parse_micros(value: &str) -> Result<i64> {
    if value.is_empty() || value.len() > 16 || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid("Unit amounts must be nonnegative integer strings"));
    }
    value
        .parse::<i64>()
        .map_err(|_| invalid("Amount is out of range"))
}

impl Store {
    pub async fn quote(
        &self,
        account_id: &str,
        request: &QuoteRequest,
        secret: &str,
    ) -> Result<Value> {
        // Preview path: served from the read-through cache and the local
        // clock estimate, so it costs no database round trips on a hit.
        // Execution re-validates every claim transactionally.
        let instance = self.instance_cached(&request.instance_id).await?;
        let now = self.now_cached();
        if !instance.tradable(now) {
            return Err(conflict("This market is closed or suspended"));
        }
        let outcome = instance.outcome_index(&request.outcome_id)?;
        let calculation = amm::calculate(
            &instance.inventory,
            instance.liquidity_units,
            outcome,
            request.side,
            request.quantity_millis,
        )?;
        let fee_micros = if instance.fee_charged {
            fee::trade_fee(calculation.amount_micros)
        } else {
            0
        };
        // The signed amount is the all-in debit/credit the trader confirms.
        let total_micros = fee::charged(
            calculation.amount_micros,
            request.side == Side::Buy,
            instance.fee_charged,
        )?;
        let claims = QuoteClaims {
            account_id: account_id.into(),
            instance_id: instance.id.clone(),
            outcome_id: request.outcome_id.clone(),
            side: request.side,
            quantity_millis: request.quantity_millis,
            amount_micros: calculation.amount_micros,
            fee_micros,
            version: instance.version,
            expires_ms: (now + 15000).min(instance.close_ms),
            engine: amm::ENGINE_VERSION.into(),
        };
        Ok(
            json!({"quote_token":auth::sign(&claims, secret.as_bytes())?,"instance_id":claims.instance_id,
            "outcome_id":request.outcome_id,"side":request.side,"quantity_millis":request.quantity_millis,
            "amount_micros":total_micros.to_string(),"fee_micros":fee_micros.to_string(),"version":instance.version,"expires_ms":claims.expires_ms,
            "server_time_ms":now,"average_price":total_micros as f64 / (request.quantity_millis as f64 * 1000.0),
            "price_before":calculation.prices_before[outcome],"price_after":calculation.prices_after[outcome]}),
        )
    }

    pub async fn execute(
        &self,
        account: &str,
        key: &str,
        request: &TradeRequest,
        secret: &str,
    ) -> Result<Value> {
        if key.is_empty()
            || key.len() > 120
            || !key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(invalid(
                "Supply an Idempotency-Key of 1–120 letters, digits, hyphens or underscores",
            ));
        }
        let claims: QuoteClaims = auth::verify(&request.quote_token, secret.as_bytes())?;
        if claims.account_id != account || claims.engine != amm::ENGINE_VERSION {
            return Err(Error::Unauthorized);
        }
        let limit = parse_micros(&request.limit_micros)?;
        let request_hash =
            auth::hash(&serde_json::to_vec(request).map_err(|e| Error::Internal(e.to_string()))?);
        for attempt in 0..3 {
            let result = self
                .execute_once(account, key, &request_hash, &claims, limit)
                .await;
            let retry = matches!(&result, Err(Error::Database(sqlx::Error::Database(e)))
                if matches!(e.code().as_deref(), Some("40001" | "40P01" | "55P03")));
            if !retry {
                return result;
            }
            if attempt < 2 {
                tokio::time::sleep(std::time::Duration::from_millis(20 * (attempt + 1))).await;
            }
        }
        Err(conflict(
            "Market is busy. Retry the same request and idempotency key",
        ))
    }

    /// One statement applies every remaining trade write and publishes the
    /// commit-time notification. Semantics are identical to issuing the
    /// statements separately inside the same transaction; the single round
    /// trip halves the statement count of the execution path. The
    /// `pg_notify` call must stay in the primary SELECT: PostgreSQL does not
    /// evaluate a common table expression arm that nothing references.
    const FINALIZE_TRADE: &str = r#"
        WITH updated AS (
            UPDATE instances SET inventory=$4,version=$5
            WHERE id=$1 AND version=$5-1 RETURNING id
        ), position AS (
            INSERT INTO positions(account_id,instance_id,outcome_index,quantity_millis)
            VALUES($2,$1,$6,$7)
            ON CONFLICT(account_id,instance_id,outcome_index)
            DO UPDATE SET quantity_millis=EXCLUDED.quantity_millis
            RETURNING account_id
        ), trade AS (
            INSERT INTO trades(id,account_id,instance_id,outcome_index,side,quantity_millis,amount_micros,fee_micros,instance_version,engine_version,created_ms)
            VALUES($8,$2,$1,$6,$9,$10,$11,$16,$5,$12,$13) RETURNING id
        ), reply AS (
            UPDATE idempotency SET response=$14
            WHERE account_id=$2 AND key=$3 RETURNING key
        ), outbox AS (
            INSERT INTO outbox(instance_id,instance_version,event_type,created_ms)
            VALUES($1,$5,'trade',$13) RETURNING id
        )
        SELECT (SELECT count(*) FROM updated) AS updated_count,
               (SELECT count(*) FROM reply) AS reply_count,
               (SELECT id FROM outbox) AS outbox_id,
               (SELECT pg_notify($15, json_build_object('id',o.id,'instance_id',$1,'version',$5,'type','trade','created_ms',$13,'inventory',$4)::text) FROM outbox o) AS notified
    "#;

    async fn execute_once(
        &self,
        account: &str,
        key: &str,
        request_hash: &str,
        claims: &QuoteClaims,
        limit: i64,
    ) -> Result<Value> {
        let mut tx = self.pool.begin().await?;
        // Reserve the idempotency key and replay any committed response.
        let inserted =
            sqlx::query("INSERT INTO idempotency(account_id,key,request_hash) VALUES($1,$2,$3) ON CONFLICT DO NOTHING RETURNING request_hash")
                .bind(account)
                .bind(key)
                .bind(request_hash)
                .fetch_optional(&mut *tx)
                .await?;
        if inserted.is_none() {
            let record = sqlx::query("SELECT request_hash,response FROM idempotency WHERE account_id=$1 AND key=$2 FOR UPDATE")
                .bind(account)
                .bind(key)
                .fetch_one(&mut *tx)
                .await?;
            if record.get::<String, _>("request_hash") != request_hash {
                return Err(conflict(
                    "Idempotency key was already used for a different request",
                ));
            }
            if let Some(response) = record.get::<Option<Json<Value>>, _>("response") {
                tx.commit().await?;
                return Ok(response.0);
            }
        }
        // Lock the instance and read the database clock in one round trip.
        let row = sqlx::query("SELECT i.*,(SELECT (extract(epoch FROM clock_timestamp())*1000)::BIGINT + s.clock_offset_ms FROM settings s WHERE s.singleton) AS db_now_ms FROM instances i WHERE i.id=$1 FOR UPDATE")
            .bind(&claims.instance_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(Error::NotFound)?;
        let instance = Instance::from_row(&row)?;
        let now: i64 = row.get("db_now_ms");
        if !instance.tradable(now) {
            return Err(conflict("This market is closed or suspended"));
        }
        if now >= claims.expires_ms {
            return Err(conflict("Quote expired. Request a new quote"));
        }
        if claims.version != instance.version {
            return Err(conflict("Market price changed. Request a new quote"));
        }
        let outcome = instance.outcome_index(&claims.outcome_id)?;
        let calculation = amm::calculate(
            &instance.inventory,
            instance.liquidity_units,
            outcome,
            claims.side,
            claims.quantity_millis,
        )?;
        if calculation.amount_micros != claims.amount_micros {
            return Err(conflict("Quote calculation changed. Request a new quote"));
        }
        let amount = calculation.amount_micros;
        let fee_micros = if instance.fee_charged {
            fee::trade_fee(amount)
        } else {
            0
        };
        if fee_micros != claims.fee_micros {
            return Err(conflict("Quote calculation changed. Request a new quote"));
        }
        // The ledger moves one all-in amount; the fee rides inside it and is
        // split out of the reserve when the instance settles.
        let total = fee::charged(amount, claims.side == Side::Buy, instance.fee_charged)?;
        let is_buy = claims.side == Side::Buy;
        if is_buy && total > limit || !is_buy && total < limit {
            return Err(conflict("Trade exceeds your cost/proceeds limit"));
        }
        // Lock both accounts and read balances and the current position in
        // one round trip.
        let locked = sqlx::query("SELECT a.id, a.balance_micros, (SELECT p.quantity_millis FROM positions p WHERE p.account_id=$1 AND p.instance_id=$2 AND p.outcome_index=$3) AS position_millis FROM accounts a WHERE a.id = ANY($4) ORDER BY a.id FOR NO KEY UPDATE")
            .bind(account)
            .bind(&instance.id)
            .bind(outcome as i32)
            .bind(vec![account.to_string(), instance.reserve_account_id.clone()])
            .fetch_all(&mut *tx)
            .await?;
        let balance_of = |id: &str| {
            locked
                .iter()
                .find(|row| row.get::<String, _>("id") == id)
                .map(|row| row.get::<i64, _>("balance_micros"))
        };
        let current: i64 = locked
            .first()
            .and_then(|row| row.get::<Option<i64>, _>("position_millis"))
            .unwrap_or(0);
        let user_balance = balance_of(account)
            .ok_or_else(|| Error::Internal("User account row missing under lock".into()))?;
        let reserve_balance = balance_of(&instance.reserve_account_id)
            .ok_or_else(|| Error::Internal("Reserve account row missing under lock".into()))?;
        if is_buy && user_balance < total {
            return Err(conflict("Insufficient simulated units"));
        }
        if !is_buy && current < claims.quantity_millis {
            return Err(conflict("You can only sell shares you own"));
        }
        let owned = current
            + if is_buy {
                claims.quantity_millis
            } else {
                -claims.quantity_millis
            };
        let remaining_reserve = reserve_balance + if is_buy { total } else { -total };
        let liability = calculation.inventory.iter().max().copied().unwrap_or(0) * 1000;
        if remaining_reserve < liability {
            return Err(conflict(
                "Trade would violate the market's reserve coverage",
            ));
        }
        let trade_id = Uuid::new_v4().to_string();
        let side = if is_buy { "buy" } else { "sell" };
        let (from, to) = if is_buy {
            (account, instance.reserve_account_id.as_str())
        } else {
            (instance.reserve_account_id.as_str(), account)
        };
        transfer(
            &mut tx,
            from,
            to,
            total,
            side,
            &format!("trade:{trade_id}"),
            now,
        )
        .await?;
        let balance_after = user_balance + if is_buy { -total } else { total };
        let version = instance.version + 1;
        let response = json!({"trade_id":trade_id,"instance_id":instance.id,"outcome_id":claims.outcome_id,
            "side":claims.side,"quantity_millis":claims.quantity_millis,"amount_micros":total.to_string(),
            "fee_micros":fee_micros.to_string(),
            "balance_micros":balance_after.to_string(),"owned_millis":owned,
            "version":version,"created_ms":now});
        let finalized = sqlx::query(Self::FINALIZE_TRADE)
            .bind(&instance.id)
            .bind(account)
            .bind(key)
            .bind(&calculation.inventory)
            .bind(version)
            .bind(outcome as i32)
            .bind(owned)
            .bind(&trade_id)
            .bind(side)
            .bind(claims.quantity_millis)
            .bind(total)
            .bind(amm::ENGINE_VERSION)
            .bind(now)
            .bind(Json(&response))
            .bind(events::CHANNEL)
            .bind(fee_micros)
            .fetch_one(&mut *tx)
            .await?;
        if finalized.get::<i64, _>("updated_count") != 1
            || finalized.get::<i64, _>("reply_count") != 1
            || finalized.get::<Option<i64>, _>("outbox_id").is_none()
        {
            return Err(Error::Internal(
                "Trade finalization did not complete every write".into(),
            ));
        }
        tx.commit().await?;
        self.cache
            .apply_trade(&instance.id, version, &calculation.inventory);
        Ok(response)
    }
}
