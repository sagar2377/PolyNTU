use crate::{
    auth,
    error::{Error, Result, conflict, invalid},
    market::{EvidenceInput, Instance, Resolution},
    store::{Store, audit, balance, db_now, event, lock_accounts, transfer},
};
use serde_json::{Value, json};
use sqlx::{Row, types::Json};
use uuid::Uuid;

impl Store {
    pub async fn ingest_evidence(&self, id: &str, input: &EvidenceInput) -> Result<Value> {
        self.record_evidence(id, input, false).await
    }

    pub(crate) async fn record_evidence(
        &self,
        id: &str,
        input: &EvidenceInput,
        simulator: bool,
    ) -> Result<Value> {
        if input.event_id.is_empty()
            || input.event_id.len() > 120
            || input.source_revision < 0
            || input.source_revision > 1_000_000
            || input.reference.trim().is_empty()
            || input.reference.len() > 2000
        {
            return Err(invalid(
                "Evidence requires a bounded event ID, revision, and source reference",
            ));
        }
        let encoded = serde_json::to_vec(input).map_err(|e| Error::Internal(e.to_string()))?;
        if encoded.len() > 65536 {
            return Err(invalid("Evidence payload is too large"));
        }
        let fingerprint = auth::hash(&encoded);
        let mut tx = self.pool.begin().await?;
        let instance: Instance = sqlx::query_as("SELECT * FROM instances WHERE id=$1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(Error::NotFound)?;
        let now = db_now(&mut tx).await?;
        let existing = sqlx::query("SELECT id,payload_hash FROM evidence WHERE instance_id=$1 AND source_id=$2 AND event_id=$3")
            .bind(id).bind(&input.source_id).bind(&input.event_id).fetch_optional(&mut *tx).await?;
        if let Some(row) = existing {
            if row.get::<String, _>("payload_hash") != fingerprint {
                return Err(conflict(
                    "Evidence event ID was reused with different content",
                ));
            }
            tx.commit().await?;
            return Ok(json!({"evidence_id":row.get::<String,_>("id"),"duplicate":true}));
        }
        if !matches!(instance.state.as_str(), "open" | "closed") {
            audit(&mut tx, "late_evidence_rejected", Some(id), json!({"event_id":input.event_id,"source_id":input.source_id,"payload_hash":fingerprint}), now).await?;
            tx.commit().await?;
            return Err(conflict(
                "Resolution is already final; evidence cannot change credited results",
            ));
        }
        let replay = simulator
            && self.demo_mode
            && instance.data_mode == "simulated"
            && input.source_id == "polyntu-simulator-v1";
        if simulator && !replay {
            return Err(Error::Forbidden);
        }
        if !simulator && instance.data_mode == "simulated" {
            return Err(invalid(
                "Simulator evidence is generated only by the demo worker",
            ));
        }
        if now < instance.observation_end_ms {
            return Err(conflict("The observation window has not ended"));
        }
        if !replay && now >= instance.evidence_deadline_ms {
            return Err(conflict("The published evidence deadline has passed"));
        }
        if input.source_id != instance.source_id
            || input.window_start_ms != instance.observation_start_ms
            || input.window_end_ms != instance.observation_end_ms
        {
            return Err(invalid(
                "Evidence source or observation window differs from the published rule",
            ));
        }
        let result = instance.rule.evaluate(
            &input.observation,
            instance.observation_start_ms,
            instance.observation_end_ms,
        )?;
        let revision_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM evidence WHERE instance_id=$1 AND source_revision=$2)",
        )
        .bind(id)
        .bind(input.source_revision)
        .fetch_one(&mut *tx)
        .await?;
        if revision_exists {
            return Err(conflict(
                "This source revision already exists. Corrections need a higher revision",
            ));
        }
        let evidence_id = Uuid::new_v4().to_string();
        let received = if replay {
            instance.observation_end_ms
        } else {
            now
        };
        sqlx::query("INSERT INTO evidence(id,instance_id,source_id,event_id,source_revision,received_ms,parser_version,payload,payload_hash,evaluated_result) VALUES($1,$2,$3,$4,$5,$6,'category-evidence-v1',$7,$8,$9)")
            .bind(&evidence_id).bind(id).bind(&input.source_id).bind(&input.event_id).bind(input.source_revision).bind(received)
            .bind(Json(input)).bind(fingerprint).bind(result.as_ref().map(Json)).execute(&mut *tx).await?;
        sqlx::query("UPDATE instances SET evidence_id=(SELECT id FROM evidence WHERE instance_id=$1 ORDER BY source_revision DESC LIMIT 1),version=version+1 WHERE id=$1")
            .bind(id).execute(&mut *tx).await?;
        audit(
            &mut tx,
            if replay {
                "simulated_evidence"
            } else {
                "evidence_received"
            },
            Some(id),
            json!({"evidence_id":evidence_id,"source_revision":input.source_revision}),
            now,
        )
        .await?;
        event(&mut tx, id, instance.version + 1, "evidence", now).await?;
        tx.commit().await?;
        Ok(json!({"evidence_id":evidence_id,"duplicate":false,"evaluated_result":result}))
    }

    pub async fn suspend(&self, id: &str, suspended: bool, reason: &str) -> Result<()> {
        if reason.trim().len() < 5 || reason.len() > 1000 {
            return Err(invalid("Supply a suspension reason of 5–1000 characters"));
        }
        let mut tx = self.pool.begin().await?;
        let instance: Instance = sqlx::query_as("SELECT * FROM instances WHERE id=$1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(Error::NotFound)?;
        if instance.state != "open" {
            return Err(conflict(
                "Only open instances can change trading suspension",
            ));
        }
        let now = db_now(&mut tx).await?;
        sqlx::query("UPDATE instances SET suspended=$2,version=version+1 WHERE id=$1")
            .bind(id)
            .bind(suspended)
            .execute(&mut *tx)
            .await?;
        audit(
            &mut tx,
            "suspension",
            Some(id),
            json!({"suspended":suspended,"reason":reason}),
            now,
        )
        .await?;
        event(&mut tx, id, instance.version + 1, "suspension", now).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn close_due(&self) -> Result<usize> {
        let mut tx = self.pool.begin().await?;
        let now = db_now(&mut tx).await?;
        let due: Vec<Instance> = sqlx::query_as("SELECT * FROM instances WHERE state='open' AND close_ms <= $1 ORDER BY close_ms,id LIMIT 100 FOR UPDATE SKIP LOCKED")
            .bind(now).fetch_all(&mut *tx).await?;
        for instance in &due {
            sqlx::query("UPDATE instances SET state='closed',version=version+1 WHERE id=$1")
                .bind(&instance.id)
                .execute(&mut *tx)
                .await?;
            event(&mut tx, &instance.id, instance.version + 1, "closed", now).await?;
        }
        tx.commit().await?;
        Ok(due.len())
    }

    /// Finalize evidence once, then process <=100 account claims per transaction.
    pub async fn settle_batch(&self, id: &str) -> Result<usize> {
        let mut tx = self.pool.begin().await?;
        let mut instance: Instance =
            sqlx::query_as("SELECT * FROM instances WHERE id=$1 FOR UPDATE")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(Error::NotFound)?;
        let now = db_now(&mut tx).await?;
        if instance.state == "closed" && now >= instance.finalize_after_ms {
            let latest = sqlx::query("SELECT id,evaluated_result FROM evidence WHERE instance_id=$1 ORDER BY source_revision DESC LIMIT 1")
                .bind(id).fetch_optional(&mut *tx).await?;
            let result = latest
                .as_ref()
                .and_then(|r| r.get::<Option<Json<Resolution>>, _>("evaluated_result"))
                .map(|r| r.0);
            let result = match result {
                Some(result) => result,
                None if now >= instance.evidence_deadline_ms => Resolution::Void {
                    reason: "No complete final evidence by the published deadline".into(),
                },
                None => {
                    tx.commit().await?;
                    return Ok(0);
                }
            };
            sqlx::query(
                "UPDATE instances SET state='resolving',result=$2,version=version+1 WHERE id=$1",
            )
            .bind(id)
            .bind(Json(&result))
            .execute(&mut *tx)
            .await?;
            instance.result = Some(Json(result));
            instance.state = "resolving".into();
            instance.version += 1;
            event(&mut tx, id, instance.version, "resolving", now).await?;
            audit(
                &mut tx,
                "finalize",
                Some(id),
                json!({"result":instance.result,"evidence_id":instance.evidence_id}),
                now,
            )
            .await?;
        }
        if instance.state != "resolving" {
            tx.commit().await?;
            return Ok(0);
        }
        let result = &instance
            .result
            .as_ref()
            .ok_or_else(|| Error::Internal("Resolving instance lacks a result".into()))?
            .0;
        let accounts: Vec<String> = sqlx::query_scalar("SELECT DISTINCT p.account_id FROM positions p WHERE p.instance_id=$1 AND p.quantity_millis>0 AND NOT EXISTS(SELECT 1 FROM settlement_claims c WHERE c.instance_id=p.instance_id AND c.account_id=p.account_id) ORDER BY p.account_id LIMIT 100")
            .bind(id).fetch_all(&mut *tx).await?;
        let mut locks = accounts.clone();
        locks.push(instance.reserve_account_id.clone());
        locks.push("treasury".into());
        lock_accounts(&mut tx, &locks).await?;
        for account in &accounts {
            let holdings = sqlx::query("SELECT outcome_index,quantity_millis FROM positions WHERE instance_id=$1 AND account_id=$2")
                .bind(id).bind(account).fetch_all(&mut *tx).await?;
            let quantity: i64 = match result {
                Resolution::Winner { outcome } => holdings
                    .iter()
                    .filter(|r| r.get::<i32, _>("outcome_index") as usize == *outcome)
                    .map(|r| r.get::<i64, _>("quantity_millis"))
                    .sum(),
                Resolution::Void { .. } => holdings
                    .iter()
                    .map(|r| r.get::<i64, _>("quantity_millis"))
                    .sum(),
            };
            let credit = match result {
                Resolution::Winner { .. } => quantity * 1000,
                Resolution::Void { .. } => quantity * 1000 / instance.outcomes.len() as i64,
            };
            if balance(&mut tx, &instance.reserve_account_id).await? < credit {
                return Err(Error::Internal(
                    "Settlement reserve invariant failed".into(),
                ));
            }
            transfer(
                &mut tx,
                &instance.reserve_account_id,
                account,
                credit,
                "resolution",
                &format!("claim:{id}:{account}"),
                now,
            )
            .await?;
            sqlx::query("INSERT INTO settlement_claims(instance_id,account_id,credit_micros,created_ms) VALUES($1,$2,$3,$4)")
                .bind(id).bind(account).bind(credit).bind(now).execute(&mut *tx).await?;
        }
        let outstanding: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM positions p WHERE p.instance_id=$1 AND p.quantity_millis>0 AND NOT EXISTS(SELECT 1 FROM settlement_claims c WHERE c.instance_id=p.instance_id AND c.account_id=p.account_id))")
            .bind(id).fetch_one(&mut *tx).await?;
        if !outstanding {
            let unused = balance(&mut tx, &instance.reserve_account_id).await?;
            transfer(
                &mut tx,
                &instance.reserve_account_id,
                "treasury",
                unused,
                "release",
                &format!("release:{id}"),
                now,
            )
            .await?;
            let terminal = if matches!(result, Resolution::Void { .. }) {
                "voided"
            } else {
                "resolved"
            };
            sqlx::query("UPDATE instances SET state=$2,version=version+1 WHERE id=$1")
                .bind(id)
                .bind(terminal)
                .execute(&mut *tx)
                .await?;
            event(&mut tx, id, instance.version + 1, terminal, now).await?;
        }
        tx.commit().await?;
        Ok(accounts.len())
    }

    pub async fn advance_demo_clock(&self, minutes: i64) -> Result<i64> {
        if !self.demo_mode {
            return Err(Error::Forbidden);
        }
        if !(1..=10080).contains(&minutes) {
            return Err(invalid("Advance between 1 minute and 7 days"));
        }
        let mut tx = self.pool.begin().await?;
        let updated = sqlx::query("UPDATE settings SET clock_offset_ms=clock_offset_ms+$1 WHERE singleton AND clock_offset_ms <= 315360000000-$1")
            .bind(minutes * 60000).execute(&mut *tx).await?;
        if updated.rows_affected() == 0 {
            return Err(invalid("Demo clock is limited to a ten-year offset"));
        }
        let now = db_now(&mut tx).await?;
        audit(
            &mut tx,
            "advance_demo_clock",
            None,
            json!({"minutes":minutes,"server_time_ms":now}),
            now,
        )
        .await?;
        tx.commit().await?;
        Ok(now)
    }
}
