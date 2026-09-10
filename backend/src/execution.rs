use crate::{
    amm::{self, Side},
    auth,
    error::{Error, Result, conflict, invalid},
    market::Instance,
    store::{Account, Store, balance, db_now, event, lock_accounts, transfer},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Row, types::Json};
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
        account: &Account,
        request: &QuoteRequest,
        secret: &str,
    ) -> Result<Value> {
        let instance = self.instance(&request.instance_id).await?;
        let now = self.now().await?;
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
        let claims = QuoteClaims {
            account_id: account.id.clone(),
            instance_id: instance.id,
            outcome_id: request.outcome_id.clone(),
            side: request.side,
            quantity_millis: request.quantity_millis,
            amount_micros: calculation.amount_micros,
            version: instance.version,
            expires_ms: (now + 15000).min(instance.close_ms),
            engine: amm::ENGINE_VERSION.into(),
        };
        Ok(
            json!({"quote_token":auth::sign(&claims, secret.as_bytes())?,"instance_id":claims.instance_id,
            "outcome_id":request.outcome_id,"side":request.side,"quantity_millis":request.quantity_millis,
            "amount_micros":calculation.amount_micros.to_string(),"version":instance.version,"expires_ms":claims.expires_ms,
            "server_time_ms":now,"average_price":calculation.amount_micros as f64 / (request.quantity_millis as f64 * 1000.0),
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

    async fn execute_once(
        &self,
        account: &str,
        key: &str,
        request_hash: &str,
        claims: &QuoteClaims,
        limit: i64,
    ) -> Result<Value> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("INSERT INTO idempotency(account_id,key,request_hash) VALUES($1,$2,$3) ON CONFLICT DO NOTHING")
            .bind(account).bind(key).bind(request_hash).execute(&mut *tx).await?;
        let record = sqlx::query("SELECT request_hash,response FROM idempotency WHERE account_id=$1 AND key=$2 FOR UPDATE")
            .bind(account).bind(key).fetch_one(&mut *tx).await?;
        if record.get::<String, _>("request_hash") != request_hash {
            return Err(conflict(
                "Idempotency key was already used for a different request",
            ));
        }
        if let Some(response) = record.get::<Option<Json<Value>>, _>("response") {
            tx.commit().await?;
            return Ok(response.0);
        }
        let instance: Instance = sqlx::query_as("SELECT * FROM instances WHERE id=$1 FOR UPDATE")
            .bind(&claims.instance_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(Error::NotFound)?;
        lock_accounts(
            &mut tx,
            &[account.into(), instance.reserve_account_id.clone()],
        )
        .await?;
        let now = db_now(&mut tx).await?;
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
        let current: i64 = sqlx::query_scalar("SELECT quantity_millis FROM positions WHERE account_id=$1 AND instance_id=$2 AND outcome_index=$3")
            .bind(account).bind(&instance.id).bind(outcome as i32).fetch_optional(&mut *tx).await?.unwrap_or(0);
        let amount = calculation.amount_micros;
        let is_buy = claims.side == Side::Buy;
        if is_buy && amount > limit || !is_buy && amount < limit {
            return Err(conflict("Trade exceeds your cost/proceeds limit"));
        }
        if is_buy && balance(&mut tx, account).await? < amount {
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
        let remaining_reserve = balance(&mut tx, &instance.reserve_account_id).await?
            + if is_buy { amount } else { -amount };
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
            amount,
            side,
            &format!("trade:{trade_id}"),
            now,
        )
        .await?;
        sqlx::query("INSERT INTO positions(account_id,instance_id,outcome_index,quantity_millis) VALUES($1,$2,$3,$4) ON CONFLICT(account_id,instance_id,outcome_index) DO UPDATE SET quantity_millis=EXCLUDED.quantity_millis")
            .bind(account).bind(&instance.id).bind(outcome as i32).bind(owned).execute(&mut *tx).await?;
        sqlx::query("UPDATE instances SET inventory=$2,version=version+1 WHERE id=$1")
            .bind(&instance.id)
            .bind(&calculation.inventory)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO trades(id,account_id,instance_id,outcome_index,side,quantity_millis,amount_micros,instance_version,engine_version,created_ms) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
            .bind(&trade_id).bind(account).bind(&instance.id).bind(outcome as i32).bind(side).bind(claims.quantity_millis)
            .bind(amount).bind(instance.version + 1).bind(amm::ENGINE_VERSION).bind(now).execute(&mut *tx).await?;
        let response = json!({"trade_id":trade_id,"instance_id":instance.id,"outcome_id":claims.outcome_id,
            "side":claims.side,"quantity_millis":claims.quantity_millis,"amount_micros":amount.to_string(),
            "balance_micros":balance(&mut tx, account).await?.to_string(),"owned_millis":owned,
            "version":instance.version+1,"created_ms":now});
        sqlx::query("UPDATE idempotency SET response=$3 WHERE account_id=$1 AND key=$2")
            .bind(account)
            .bind(key)
            .bind(Json(&response))
            .execute(&mut *tx)
            .await?;
        event(&mut tx, &instance.id, instance.version + 1, "trade", now).await?;
        tx.commit().await?;
        Ok(response)
    }
}
