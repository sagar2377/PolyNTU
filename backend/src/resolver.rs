//! The external resolver contract (ADR 0007). At the finalize window the
//! settlement worker calls the series' configured endpoint with a fixed
//! request; the response must name exactly one of the published outcome
//! identifiers. Unreachable, malformed, or out-of-options answers count as
//! missing evidence, and the instance voids at the published deadline.
//!
//! This module also hosts the platform's own NTU Bus API adapter, the
//! resolver endpoint the demo bus series points at.
use crate::{
    error::{Error, Result},
    market::{Instance, Resolution, Rule},
    store::Store,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;

/// The fixed request every resolver must accept. Besides identifying the
/// bracket, it carries everything an integrator needs to answer without a
/// second lookup: the published outcomes the answer may name, the instance
/// title for logs, and the evidence deadline after which answers no longer
/// count.
#[derive(Debug, Serialize)]
pub struct ResolverRequest<'a> {
    pub instance_id: &'a str,
    pub series_id: &'a str,
    pub bracket_start_ms: Option<i64>,
    pub window_start_ms: i64,
    pub window_end_ms: i64,
    pub outcomes: Vec<OutcomeRef<'a>>,
    pub title: &'a str,
    pub evidence_deadline_ms: i64,
    pub rule: &'a Rule,
}

/// One published outcome of the request's instance.
#[derive(Debug, Serialize)]
pub struct OutcomeRef<'a> {
    pub id: &'a str,
    pub label: &'a str,
}

impl<'a> ResolverRequest<'a> {
    pub fn of(instance: &'a Instance) -> Self {
        Self {
            instance_id: &instance.id,
            series_id: instance.series_id.as_deref().unwrap_or_default(),
            bracket_start_ms: instance.bracket_start_ms,
            window_start_ms: instance.observation_start_ms,
            window_end_ms: instance.observation_end_ms,
            outcomes: instance
                .outcomes
                .iter()
                .map(|o| OutcomeRef {
                    id: &o.id,
                    label: &o.label,
                })
                .collect(),
            title: &instance.title,
            evidence_deadline_ms: instance.evidence_deadline_ms,
            rule: &instance.rule.0,
        }
    }
}

/// One parsed resolver answer.
#[derive(Debug, PartialEq)]
pub enum ResolverAnswer {
    /// The source is not final yet; retry on the next tick.
    Pending,
    /// Exactly one published outcome identifier.
    Outcome(String),
    /// Malformed or not one of the published options: missing evidence.
    Invalid,
}

/// Parse a resolver response against the published option set. The contract
/// is `{"outcome_id": "<published id>"}` or `{"pending": true}`.
pub fn parse_response(body: &[u8], instance: &Instance) -> ResolverAnswer {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(body) else {
        return ResolverAnswer::Invalid;
    };
    if value.get("pending") == Some(&serde_json::Value::Bool(true)) {
        return ResolverAnswer::Pending;
    }
    let Some(outcome_id) = value.get("outcome_id").and_then(|v| v.as_str()) else {
        return ResolverAnswer::Invalid;
    };
    if !outcome_id.is_empty()
        && outcome_id.len() <= 120
        && instance.outcomes.iter().any(|o| o.id == outcome_id)
    {
        ResolverAnswer::Outcome(outcome_id.to_string())
    } else {
        ResolverAnswer::Invalid
    }
}

/// Call the resolver endpoint with the fixed request. Errors mean unreachable
/// or oversized responses and are treated as missing evidence by the caller.
pub async fn call(
    endpoint: &str,
    request: &ResolverRequest<'_>,
) -> Result<(Vec<u8>, serde_json::Value)> {
    call_within(endpoint, request, Duration::from_secs(5)).await
}

/// The same call with a caller-chosen timeout. The bus adapter uses a short
/// one so its provider retries always fit inside the settlement worker's own
/// 5-second budget for calling the adapter.
pub async fn call_within(
    endpoint: &str,
    request: &ResolverRequest<'_>,
    timeout: Duration,
) -> Result<(Vec<u8>, serde_json::Value)> {
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|e| Error::Internal(e.to_string()))?;
    let response = client
        .post(endpoint)
        .json(request)
        .send()
        .await
        .map_err(|e| Error::Internal(e.to_string()))?;
    let body = response
        .bytes()
        .await
        .map_err(|e| Error::Internal(e.to_string()))?;
    if body.len() > 65536 {
        return Err(Error::Internal("Resolver response too large".into()));
    }
    let request_value =
        serde_json::to_value(request).map_err(|e| Error::Internal(e.to_string()))?;
    Ok((body.to_vec(), request_value))
}

/// The request body the platform's own adapter accepts: the fixed resolver
/// request, of which only the instance identifier is needed; the stored
/// instance is the authority for its rule, window, and outcomes.
#[derive(Debug, Deserialize)]
pub struct ResolverCall {
    pub instance_id: String,
}

/// The platform's built-in NTU Bus API adapter (ADR 0007). Bus brackets
/// resolve through the external-resolver contract like any other series, and
/// this endpoint is the bus timing source. The live data path comes first:
/// the NTU Bus API provider (`provider/ntubus`), a standalone service that
/// speaks this same resolver contract and watches the live Omnibus feed. A
/// few attempts ride out a blip; only when the provider stays without a
/// definitive answer does the deterministic simulated feed answer, so
/// development and CI work without the provider running. The `source` field
/// records which path answered, and the recorded evidence carries it. Before
/// a bracket's observation window has ended the answer is always pending, so
/// nothing is revealed early.
pub async fn ntu_bus_answer(store: &Store, instance_id: &str) -> Result<Value> {
    let pending = || json!({"pending": true});
    let instance: Option<Instance> = sqlx::query_as("SELECT * FROM instances WHERE id=$1")
        .bind(instance_id)
        .fetch_optional(&store.pool)
        .await?;
    let Some(instance) = instance else {
        return Ok(pending());
    };
    let now = store.now().await?;
    if instance.data_mode != "simulated"
        || now < instance.observation_end_ms
        || !matches!(instance.rule.0, Rule::Bus { .. })
    {
        return Ok(pending());
    }
    let provider = store
        .ntubus_provider
        .as_deref()
        .unwrap_or("http://127.0.0.1:8090/resolve");
    if !provider.is_empty() {
        let request = ResolverRequest::of(&instance);
        for attempt in 0..3 {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
            if let Ok((body, _)) = call_within(provider, &request, Duration::from_secs(1)).await
                && let ResolverAnswer::Outcome(outcome_id) = parse_response(&body, &instance)
            {
                return Ok(json!({"outcome_id": outcome_id, "source": "ntubus-live"}));
            }
        }
    }
    let secret: String =
        sqlx::query_scalar("SELECT simulation_secret FROM settings WHERE singleton")
            .fetch_one(&store.pool)
            .await?;
    let private_seed = crate::auth::hash(format!("{secret}:{}", instance.id).as_bytes());
    let observation = instance.rule.simulated(
        &private_seed,
        instance.observation_start_ms,
        instance.observation_end_ms,
    );
    Ok(
        match instance.rule.evaluate(
            &observation,
            instance.observation_start_ms,
            instance.observation_end_ms,
        )? {
            Some(Resolution::Winner { outcome }) => json!({
                "outcome_id": instance.outcomes[outcome].id,
                "source": "simulated-fallback",
            }),
            _ => pending(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::{Outcome, Rule};
    use sqlx::types::Json;

    fn instance_with_outcomes() -> Instance {
        Instance {
            id: "i-1".into(),
            template_id: "t".into(),
            category: "weather".into(),
            title: "Demo".into(),
            resolution_criterion: "criterion".into(),
            rule: Json(Rule::Weather {
                station_id: "s".into(),
                threshold_milli_mm: 10,
            }),
            outcomes: Json(vec![
                Outcome {
                    id: "yes".into(),
                    label: "Yes".into(),
                },
                Outcome {
                    id: "no".into(),
                    label: "No".into(),
                },
            ]),
            source_id: "src".into(),
            data_mode: "manual".into(),
            state: "closed".into(),
            suspended: false,
            close_ms: 1,
            observation_start_ms: 1,
            observation_end_ms: 2,
            finalize_after_ms: 3,
            evidence_deadline_ms: 4,
            liquidity_units: 100,
            inventory: vec![0, 0],
            version: 0,
            reserve_account_id: "r".into(),
            fee_charged: true,
            creator_account_id: None,
            series_id: Some("s-1".into()),
            bracket_start_ms: Some(1),
            result: None,
            evidence_id: None,
        }
    }

    #[test]
    fn responses_must_name_one_published_option() {
        let instance = instance_with_outcomes();
        assert_eq!(
            parse_response(br#"{"outcome_id":"yes"}"#, &instance),
            ResolverAnswer::Outcome("yes".into())
        );
        assert_eq!(
            parse_response(br#"{"pending":true}"#, &instance),
            ResolverAnswer::Pending
        );
        assert_eq!(
            parse_response(br#"{"outcome_id":"maybe"}"#, &instance),
            ResolverAnswer::Invalid
        );
        assert_eq!(
            parse_response(br#"{"outcome_id":42}"#, &instance),
            ResolverAnswer::Invalid
        );
        assert_eq!(
            parse_response(br#"not json"#, &instance),
            ResolverAnswer::Invalid
        );
        assert_eq!(parse_response(br#"{}"#, &instance), ResolverAnswer::Invalid);
    }
}
