//! Category rules transform observed evidence into one outcome. No category prices trades.
use crate::{
    amm,
    error::{Result, invalid},
};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, types::Json};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Outcome {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    QueueLength,
    CrowdOccupancy,
    UniqueAttendance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Rule {
    Weather {
        station_id: String,
        threshold_milli_mm: i64,
    },
    Bus {
        route_id: String,
        direction: String,
        stop_id: String,
    },
    Election {
        candidates: Vec<String>,
        is_fictional: bool,
    },
    Count {
        metric: Metric,
        location_id: String,
        threshold: i64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Observation {
    Weather {
        station_id: String,
        total_milli_mm: i64,
        complete: bool,
    },
    Bus {
        route_id: String,
        direction: String,
        stop_id: String,
        arrivals_ms: Vec<i64>,
        complete: bool,
    },
    Election {
        winner: Option<String>,
        is_final: bool,
        is_fictional: bool,
    },
    Count {
        metric: Metric,
        location_id: String,
        value: i64,
        complete: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Resolution {
    Winner { outcome: usize },
    Void { reason: String },
}

impl Rule {
    pub fn category(&self) -> &'static str {
        match self {
            Self::Weather { .. } => "weather",
            Self::Bus { .. } => "bus",
            Self::Election { .. } => "elections",
            Self::Count {
                metric: Metric::UniqueAttendance,
                ..
            } => "attendance",
            Self::Count { .. } => "queue_crowd",
        }
    }

    pub fn validate(&self) -> Result<()> {
        let valid_id = |s: &str| !s.trim().is_empty() && s.len() <= 120;
        let valid = match self {
            Self::Weather {
                station_id,
                threshold_milli_mm,
            } => valid_id(station_id) && (1..=1_000_000).contains(threshold_milli_mm),
            Self::Bus {
                route_id,
                direction,
                stop_id,
            } => valid_id(route_id) && valid_id(direction) && valid_id(stop_id),
            Self::Election {
                candidates,
                is_fictional,
            } => {
                *is_fictional
                    && (2..=8).contains(&candidates.len())
                    && candidates.iter().all(|x| valid_id(x))
                    && candidates
                        .iter()
                        .enumerate()
                        .all(|(i, c)| !candidates[..i].contains(c))
            }
            Self::Count {
                location_id,
                threshold,
                ..
            } => valid_id(location_id) && (1..=1_000_000).contains(threshold),
        };
        if valid {
            Ok(())
        } else {
            Err(invalid(
                "Invalid rule: check identifiers, positive thresholds, or distinct fictional election candidates",
            ))
        }
    }

    pub fn outcomes(&self) -> Vec<Outcome> {
        if let Self::Election { candidates, .. } = self {
            candidates
                .iter()
                .enumerate()
                .map(|(i, name)| Outcome {
                    id: format!("candidate-{i}"),
                    label: name.clone(),
                })
                .collect()
        } else {
            vec![
                Outcome {
                    id: "yes".into(),
                    label: "Yes".into(),
                },
                Outcome {
                    id: "no".into(),
                    label: "No".into(),
                },
            ]
        }
    }

    /// None means insufficient/non-final evidence, never a false outcome.
    pub fn evaluate(
        &self,
        observation: &Observation,
        start: i64,
        end: i64,
    ) -> Result<Option<Resolution>> {
        let binary = |yes: bool| {
            Some(Resolution::Winner {
                outcome: if yes { 0 } else { 1 },
            })
        };
        match (self, observation) {
            (
                Self::Weather {
                    station_id,
                    threshold_milli_mm,
                },
                Observation::Weather {
                    station_id: observed_station,
                    total_milli_mm,
                    complete,
                },
            ) if station_id == observed_station && *total_milli_mm >= 0 => Ok(if *complete {
                binary(total_milli_mm >= threshold_milli_mm)
            } else {
                None
            }),
            (
                Self::Bus {
                    route_id,
                    direction,
                    stop_id,
                },
                Observation::Bus {
                    route_id: r,
                    direction: d,
                    stop_id: s,
                    arrivals_ms,
                    complete,
                },
            ) if route_id == r && direction == d && stop_id == s && arrivals_ms.len() <= 10000 => {
                Ok(if *complete {
                    binary(arrivals_ms.iter().any(|t| *t >= start && *t < end))
                } else {
                    None
                })
            }
            (
                Self::Election { candidates, .. },
                Observation::Election {
                    winner,
                    is_final,
                    is_fictional,
                },
            ) if *is_fictional => {
                if !is_final {
                    return Ok(None);
                }
                match winner {
                    Some(w) => candidates
                        .iter()
                        .position(|c| c == w)
                        .map(|outcome| Some(Resolution::Winner { outcome }))
                        .ok_or_else(|| invalid("Evidence names an unknown candidate")),
                    None => Ok(Some(Resolution::Void {
                        reason:
                            "No sole final winner; ties, runoff and cancellation void this instance"
                                .into(),
                    })),
                }
            }
            (
                Self::Count {
                    metric,
                    location_id,
                    threshold,
                },
                Observation::Count {
                    metric: m,
                    location_id: l,
                    value,
                    complete,
                },
            ) if metric == m && location_id == l && *value >= 0 => Ok(if *complete {
                binary(value >= threshold)
            } else {
                None
            }),
            _ => Err(invalid("Evidence does not match the published market rule")),
        }
    }

    pub fn simulated(&self, instance_id: &str, start: i64, end: i64) -> Observation {
        let sample = Sha256::digest(instance_id.as_bytes())[0] as i64;
        match self {
            Self::Weather {
                station_id,
                threshold_milli_mm,
            } => Observation::Weather {
                station_id: station_id.clone(),
                total_milli_mm: if sample % 2 == 0 {
                    *threshold_milli_mm + 100
                } else {
                    0
                },
                complete: true,
            },
            Self::Bus {
                route_id,
                direction,
                stop_id,
            } => Observation::Bus {
                route_id: route_id.clone(),
                direction: direction.clone(),
                stop_id: stop_id.clone(),
                arrivals_ms: if sample % 2 == 0 {
                    vec![start + (end - start) / 2]
                } else {
                    vec![]
                },
                complete: true,
            },
            Self::Election { candidates, .. } => Observation::Election {
                winner: Some(candidates[sample as usize % candidates.len()].clone()),
                is_final: true,
                is_fictional: true,
            },
            Self::Count {
                metric,
                location_id,
                threshold,
            } => Observation::Count {
                metric: metric.clone(),
                location_id: location_id.clone(),
                value: threshold + sample % 21 - 10.min(*threshold),
                complete: true,
            },
        }
    }
}

fn default_fee_charged() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewInstance {
    pub template_id: String,
    pub title: String,
    pub resolution_criterion: String,
    pub rule: Rule,
    pub source_id: String,
    pub data_mode: String,
    pub close_ms: i64,
    pub observation_start_ms: i64,
    pub observation_end_ms: i64,
    pub finalize_after_ms: i64,
    pub evidence_deadline_ms: i64,
    pub liquidity_units: i64,
    /// Whether trades on this instance pay the 25-basis-point fee. Fee-free
    /// markets exist for student welfare, like the crowd-sourced bus series;
    /// they collect no fee and pay no creator share.
    #[serde(default = "default_fee_charged")]
    pub fee_charged: bool,
    /// Participant account credited with the creator's half of the settled fee
    /// pot. Absent for platform-created instances, whose fees go to the treasury.
    #[serde(default)]
    pub creator_account_id: Option<String>,
    /// Series this instance is a bracket of, with its slot start. Absent for
    /// direct administrator-created instances.
    #[serde(default)]
    pub series_id: Option<String>,
    #[serde(default)]
    pub bracket_start_ms: Option<i64>,
}

impl NewInstance {
    pub fn validate(&self, now: i64, demo: bool) -> Result<()> {
        self.rule.validate()?;
        amm::funding(self.liquidity_units, self.rule.outcomes().len())?;
        if self.title.trim().len() < 5
            || self.title.len() > 240
            || self.resolution_criterion.trim().len() < 30
            || self.resolution_criterion.len() > 4000
            || self.template_id.is_empty()
            || self.template_id.len() > 80
            || self.source_id.trim().is_empty()
            || self.source_id.len() > 120
        {
            return Err(invalid(
                "Publish a title, template, source and a specific resolution criterion",
            ));
        }
        if !matches!(self.data_mode.as_str(), "manual" | "simulated")
            || (self.data_mode == "simulated" && !demo)
            || (self.data_mode == "simulated" && self.source_id != "polyntu-simulator-v1")
        {
            return Err(invalid(
                "Simulated markets require demo mode and source polyntu-simulator-v1; other evidence uses authenticated manual ingestion",
            ));
        }
        if self.close_ms <= now
            || self.observation_start_ms < self.close_ms
            || self.observation_end_ms <= self.observation_start_ms
            || self.finalize_after_ms < self.observation_end_ms
            || self.evidence_deadline_ms <= self.finalize_after_ms
            || self.evidence_deadline_ms > now + 366 * 86400000
        {
            return Err(invalid(
                "Use future close <= observation start < end <= finalization < evidence deadline, within one year",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct Instance {
    pub id: String,
    pub template_id: String,
    pub category: String,
    pub title: String,
    pub resolution_criterion: String,
    pub rule: Json<Rule>,
    pub outcomes: Json<Vec<Outcome>>,
    pub source_id: String,
    pub data_mode: String,
    pub state: String,
    pub suspended: bool,
    pub close_ms: i64,
    pub observation_start_ms: i64,
    pub observation_end_ms: i64,
    pub finalize_after_ms: i64,
    pub evidence_deadline_ms: i64,
    pub liquidity_units: i64,
    pub inventory: Vec<i64>,
    pub version: i64,
    pub reserve_account_id: String,
    pub fee_charged: bool,
    pub creator_account_id: Option<String>,
    pub series_id: Option<String>,
    pub bracket_start_ms: Option<i64>,
    pub result: Option<Json<Resolution>>,
    pub evidence_id: Option<String>,
}

impl Instance {
    pub fn tradable(&self, now: i64) -> bool {
        self.state == "open" && !self.suspended && now < self.close_ms
    }
    pub fn outcome_index(&self, id: &str) -> Result<usize> {
        self.outcomes
            .iter()
            .position(|o| o.id == id)
            .ok_or_else(|| invalid("Unknown outcome"))
    }
}

/// A published market series (ADR 0006).
#[derive(Debug, Clone, FromRow)]
pub struct Series {
    pub id: String,
    pub creator_account_id: Option<String>,
    pub title: String,
    pub resolution_criterion: String,
    pub rule: Json<Rule>,
    pub source_id: String,
    pub data_mode: String,
    pub liquidity_units: i64,
    pub fee_charged: bool,
    pub recurrence: String,
    pub interval_ms: Option<i64>,
    pub active_start_minute: Option<i32>,
    pub active_end_minute: Option<i32>,
    pub max_concurrency: i64,
    pub end_ms: Option<i64>,
    pub anchor_ms: i64,
    pub resolution_authority: String,
    pub resolution_public_key: Option<String>,
    pub resolver_endpoint: Option<String>,
    pub state: String,
    pub created_ms: i64,
}

impl Series {
    /// The instance spec for the bracket occupying [slot, slot + interval).
    pub fn bracket_spec(&self, slot_start_ms: i64) -> NewInstance {
        let interval = self.interval_ms.unwrap_or(60_000);
        NewInstance {
            template_id: self.id.clone(),
            title: bracket_title(&self.title, slot_start_ms, interval),
            resolution_criterion: self.resolution_criterion.clone(),
            rule: self.rule.0.clone(),
            source_id: self.source_id.clone(),
            data_mode: self.data_mode.clone(),
            close_ms: slot_start_ms,
            observation_start_ms: slot_start_ms,
            observation_end_ms: slot_start_ms + interval,
            finalize_after_ms: slot_start_ms + interval + BRACKET_FINALIZE_MARGIN_MS,
            evidence_deadline_ms: slot_start_ms + interval + BRACKET_DEADLINE_MARGIN_MS,
            liquidity_units: self.liquidity_units,
            fee_charged: self.fee_charged,
            creator_account_id: self.creator_account_id.clone(),
            series_id: Some(self.id.clone()),
            bracket_start_ms: Some(slot_start_ms),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceInput {
    pub source_id: String,
    pub event_id: String,
    pub source_revision: i64,
    pub window_start_ms: i64,
    pub window_end_ms: i64,
    pub observation: Observation,
    pub reference: String,
}

/// Series brackets finalize shortly after their observation window so the
/// rolling horizon does not pile up unsettled instances.
pub const BRACKET_FINALIZE_MARGIN_MS: i64 = 1_000;

/// The SGT (UTC+8) wall-clock time of an instant as HH:MM, matching the
/// active-window interpretation.
fn sgt_hhmm(epoch_ms: i64) -> String {
    let minute_of_day = (epoch_ms.div_euclid(60_000) + 8 * 60).rem_euclid(1440);
    format!("{:02}:{:02}", minute_of_day / 60, minute_of_day % 60)
}

/// Recurring brackets all share the series definition, so each bracket title
/// carries its time window to stay distinguishable, e.g.
/// "Blue line · arrival at North Spine · 14:20 to 14:22". The base title is
/// truncated on a character boundary so the result fits the instance bound.
fn bracket_title(base: &str, slot_start_ms: i64, interval_ms: i64) -> String {
    let suffix = format!(
        " · {} to {}",
        sgt_hhmm(slot_start_ms),
        sgt_hhmm(slot_start_ms + interval_ms)
    );
    let room = 240usize.saturating_sub(suffix.len());
    let mut cut = base.len().min(room);
    while !base.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}{}", &base[..cut], suffix)
}
pub const BRACKET_DEADLINE_MARGIN_MS: i64 = 60_000;

/// A series schedule: one explicit window, or a recurrence rule whose slots
/// the scheduler keeps filled on a rolling grid (ADR 0006).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Schedule {
    Once {
        close_ms: i64,
        observation_start_ms: i64,
        observation_end_ms: i64,
        finalize_after_ms: i64,
        evidence_deadline_ms: i64,
    },
    Recurring {
        /// Slot spacing, one minute to one day.
        interval_ms: i64,
        /// Daily operating window in Singapore time (UTC+8, no daylight
        /// saving), as minutes of day; slots never start outside it.
        active_start_minute: i32,
        active_end_minute: i32,
        /// How many upcoming slots stay live; the covered horizon is
        /// max_concurrency multiplied by interval_ms. Capped at 50.
        max_concurrency: i64,
        /// Optional series end; absent means perpetual.
        #[serde(default)]
        end_ms: Option<i64>,
    },
}

/// How a series resolves (ADR 0007), fixed at creation. Absent means the
/// platform administrator resolves it, as before.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ResolutionSpec {
    /// Human authority: the creator signs every resolution with an ed25519
    /// key this platform never holds; only the public key is published.
    Creator { public_key: String },
    /// Automatic authority: an external API that must answer with exactly
    /// one of the published outcome identifiers.
    Resolver { endpoint: String },
}

impl ResolutionSpec {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Creator { public_key } => {
                let decoded = base64::engine::general_purpose::STANDARD
                    .decode(public_key.trim())
                    .map_err(|_| invalid("The resolution public key must be base64 encoded"))?;
                if decoded.len() != 32 {
                    return Err(invalid(
                        "The resolution public key must be a 32-byte ed25519 key in base64",
                    ));
                }
            }
            Self::Resolver { endpoint } => {
                let endpoint = endpoint.trim();
                // https for real services; plain http only on loopback, where
                // local adapters run during development and tests.
                let allowed = endpoint.starts_with("https://")
                    || endpoint.starts_with("http://127.0.0.1")
                    || endpoint.starts_with("http://localhost");
                if !allowed
                    || endpoint.len() < 12
                    || endpoint.len() > 500
                    || endpoint.chars().any(char::is_whitespace)
                {
                    return Err(invalid(
                        "The resolver endpoint must be an https URL (http allowed on loopback only)",
                    ));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewSeries {
    pub title: String,
    pub resolution_criterion: String,
    pub rule: Rule,
    pub source_id: String,
    pub liquidity_units: i64,
    /// Whether trades pay the 25-basis-point fee. Fee-free series collect no
    /// fee and pay no creator share (welfare markets like bus timing).
    #[serde(default = "default_fee_charged")]
    pub fee_charged: bool,
    /// Resolution authority fixed at creation (ADR 0007); absent means the
    /// platform administrator resolves.
    #[serde(default)]
    pub resolution: Option<ResolutionSpec>,
    pub schedule: Schedule,
}

/// Slot starts inside the creator-set daily operating window, interpreted in
/// Singapore time (UTC+8, no daylight saving).
pub fn inside_active_window(slot_start_ms: i64, start_minute: i32, end_minute: i32) -> bool {
    let minute_of_day = ((slot_start_ms / 60_000 + 8 * 60) % 1440) as i32;
    (start_minute..end_minute).contains(&minute_of_day)
}

impl Schedule {
    pub fn validate(&self, now: i64) -> Result<()> {
        match self {
            Self::Once {
                close_ms,
                observation_start_ms,
                observation_end_ms,
                finalize_after_ms,
                evidence_deadline_ms,
            } => {
                if *close_ms <= now
                    || *observation_start_ms < *close_ms
                    || *observation_end_ms <= *observation_start_ms
                    || *finalize_after_ms < *observation_end_ms
                    || *evidence_deadline_ms <= *finalize_after_ms
                    || *evidence_deadline_ms > now + 366 * 86_400_000
                {
                    return Err(invalid(
                        "Use future close <= observation start < end <= finalization < evidence deadline, within one year",
                    ));
                }
            }
            Self::Recurring {
                interval_ms,
                active_start_minute,
                active_end_minute,
                max_concurrency,
                end_ms,
            } => {
                if !(60_000..=86_400_000).contains(interval_ms) {
                    return Err(invalid(
                        "Recurrence interval must be between 1 minute and 1 day",
                    ));
                }
                if !(1..=50).contains(max_concurrency) {
                    return Err(invalid("Maximum concurrency must be between 1 and 50"));
                }
                if !(0..=1438).contains(active_start_minute)
                    || !(1..=1439).contains(active_end_minute)
                    || active_start_minute >= active_end_minute
                {
                    return Err(invalid(
                        "Active period must be a daily window like 06:00 to 23:59 Singapore time",
                    ));
                }
                if end_ms.is_some_and(|end| end <= now + interval_ms) {
                    return Err(invalid(
                        "Series end must leave room for at least one more slot",
                    ));
                }
            }
        }
        Ok(())
    }

    /// The instance timing for one bracket slot starting at `slot_start_ms`.
    pub fn bracket_timing(&self, slot_start_ms: i64) -> (i64, i64, i64, i64, i64) {
        match self {
            Self::Once {
                close_ms,
                observation_start_ms,
                observation_end_ms,
                finalize_after_ms,
                evidence_deadline_ms,
            } => (
                *close_ms,
                *observation_start_ms,
                *observation_end_ms,
                *finalize_after_ms,
                *evidence_deadline_ms,
            ),
            Self::Recurring { interval_ms, .. } => (
                slot_start_ms,
                slot_start_ms,
                slot_start_ms + interval_ms,
                slot_start_ms + interval_ms + BRACKET_FINALIZE_MARGIN_MS,
                slot_start_ms + interval_ms + BRACKET_DEADLINE_MARGIN_MS,
            ),
        }
    }
}

impl NewSeries {
    pub fn validate(&self, now: i64) -> Result<()> {
        self.rule.validate()?;
        amm::funding(self.liquidity_units, self.rule.outcomes().len())?;
        if self.title.trim().len() < 5
            || self.title.len() > 240
            || self.resolution_criterion.trim().len() < 30
            || self.resolution_criterion.len() > 4000
            || self.source_id.trim().is_empty()
            || self.source_id.len() > 120
        {
            return Err(invalid(
                "Publish a title, source and a specific resolution criterion",
            ));
        }
        if let Some(resolution) = &self.resolution {
            resolution.validate()?;
        }
        self.schedule.validate(now)
    }

    /// Build the instance spec for one bracket of this series.
    pub fn instance_spec(
        &self,
        series_id: &str,
        slot_start_ms: i64,
        creator_account_id: Option<String>,
        data_mode: &str,
    ) -> NewInstance {
        let (close, start, end, finalize, deadline) = self.schedule.bracket_timing(slot_start_ms);
        NewInstance {
            template_id: series_id.into(),
            title: self.title.clone(),
            resolution_criterion: self.resolution_criterion.clone(),
            rule: self.rule.clone(),
            source_id: self.source_id.clone(),
            data_mode: data_mode.into(),
            close_ms: close,
            observation_start_ms: start,
            observation_end_ms: end,
            finalize_after_ms: finalize,
            evidence_deadline_ms: deadline,
            liquidity_units: self.liquidity_units,
            fee_charged: self.fee_charged,
            creator_account_id,
            series_id: Some(series_id.into()),
            bracket_start_ms: if matches!(self.schedule, Schedule::Recurring { .. }) {
                Some(slot_start_ms)
            } else {
                None
            },
        }
    }
}

pub fn demo_specs(now: i64) -> Vec<NewInstance> {
    let close = (now / 900000 + 1) * 900000;
    let definitions = vec![
        (
            "weather-rain",
            "Rainfall · campus weather station",
            Rule::Weather {
                station_id: "demo-campus".into(),
                threshold_milli_mm: 200,
            },
            3600000,
        ),
        (
            "election-demo",
            "Fictional hall committee election",
            Rule::Election {
                candidates: vec![
                    "Candidate A".into(),
                    "Candidate B".into(),
                    "Candidate C".into(),
                ],
                is_fictional: true,
            },
            86400000,
        ),
        (
            "queue-north",
            "North Spine · at least 20 people in the queue",
            Rule::Count {
                metric: Metric::QueueLength,
                location_id: "north-spine-stall".into(),
                threshold: 20,
            },
            300000,
        ),
        (
            "crowd-library",
            "Library · occupancy of at least 100",
            Rule::Count {
                metric: Metric::CrowdOccupancy,
                location_id: "library".into(),
                threshold: 100,
            },
            300000,
        ),
        (
            "attendance-event",
            "Campus event · at least 80 attendees",
            Rule::Count {
                metric: Metric::UniqueAttendance,
                location_id: "demo-campus-event".into(),
                threshold: 80,
            },
            7200000,
        ),
        (
            "attendance-lecture",
            "Lecture · at least 50 attendees",
            Rule::Count {
                metric: Metric::UniqueAttendance,
                location_id: "demo-lecture".into(),
                threshold: 50,
            },
            3600000,
        ),
    ];
    definitions.into_iter().map(|(id, title, rule, duration)| {
        let criterion = match &rule {
            Rule::Bus { .. } => "Yes if at least one matching bus arrives in the published [start, end) window. No requires complete observation coverage with no matching arrival.",
            Rule::Weather { .. } => "Yes if total observed rainfall at the named station reaches 0.2 mm in the published window. Missing samples do not count as zero rainfall.",
            Rule::Election { .. } => "The sole final winner of the fictional demo election resolves to one unit. A tie, runoff or cancellation voids the instance.",
            Rule::Count { metric: Metric::UniqueAttendance, .. } => "Yes if the trusted source reports at least the threshold number of unique check-ins during the published window. Repeat check-ins are counted once by the source.",
            Rule::Count { .. } => "Yes if the trusted source's count at the end of the published sampling window reaches the threshold. Queue length and occupancy use distinct measurements.",
        };
        NewInstance { template_id: id.into(), title: title.into(), resolution_criterion: format!("{criterion} Demo: deterministic simulated evidence. Missing final evidence voids shares at 1/n units each."),
            rule, source_id: "polyntu-simulator-v1".into(), data_mode: "simulated".into(), close_ms: close,
            observation_start_ms: close, observation_end_ms: close + duration, finalize_after_ms: close + duration + 1000,
            evidence_deadline_ms: close + duration + 60000, liquidity_units: 100, fee_charged: true, creator_account_id: None,
            series_id: None, bracket_start_ms: None }
    }).collect()
}

/// The demo bus market (ADR 0006): a rolling, fee-free welfare series. A new
/// bracket every 2 minutes, at most 5 live at once, covering a rolling
/// 10-minute horizon, only during operating hours.
pub fn demo_series_spec() -> NewSeries {
    NewSeries {
        title: "Blue line · arrival at North Spine".into(),
        resolution_criterion: "Yes if at least one matching bus arrives in the published [start, end) window. No requires complete observation coverage with no matching arrival. This market exists for student welfare: crowd-sourced arrival estimation, so no trading fee is charged.".into(),
        rule: Rule::Bus {
            route_id: "NTU-blue".into(),
            direction: "clockwise".into(),
            stop_id: "north-spine".into(),
        },
        source_id: "polyntu-simulator-v1".into(),
        liquidity_units: 100,
        fee_charged: false,
        resolution: None,
        schedule: Schedule::Recurring {
            interval_ms: 120000,
            active_start_minute: 360,
            active_end_minute: 1439,
            max_concurrency: 5,
            end_ms: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bus_boundaries_and_missing_coverage() {
        let rule = Rule::Bus {
            route_id: "r".into(),
            direction: "d".into(),
            stop_id: "s".into(),
        };
        for (arrivals, complete, expected) in [
            (vec![10], true, Some(0)),
            (vec![20], true, Some(1)),
            (vec![], false, None),
        ] {
            let evidence = Observation::Bus {
                route_id: "r".into(),
                direction: "d".into(),
                stop_id: "s".into(),
                arrivals_ms: arrivals,
                complete,
            };
            assert_eq!(
                rule.evaluate(&evidence, 10, 20).unwrap(),
                expected.map(|outcome| Resolution::Winner { outcome })
            );
        }
    }
    #[test]
    fn all_categories_have_deterministic_matching_evidence() {
        let specs = demo_specs(1000);
        let mut categories: std::collections::HashSet<_> =
            specs.iter().map(|s| s.rule.category()).collect();
        let series = demo_series_spec();
        categories.insert(series.rule.category());
        assert_eq!(categories.len(), 5);
        for spec in specs {
            spec.validate(1000, true).unwrap();
            let observation = spec.rule.simulated(
                "fixed-id",
                spec.observation_start_ms,
                spec.observation_end_ms,
            );
            assert!(
                spec.rule
                    .evaluate(
                        &observation,
                        spec.observation_start_ms,
                        spec.observation_end_ms
                    )
                    .unwrap()
                    .is_some()
            );
        }
        series.validate(1000).unwrap();
        let observation = series.rule.simulated("fixed-id", 0, 120000);
        assert!(
            series
                .rule
                .evaluate(&observation, 0, 120000)
                .unwrap()
                .is_some()
        );
    }
    #[test]
    fn active_window_is_interpreted_in_singapore_time() {
        // The Unix epoch is 08:00 Singapore time: minute of day 480.
        assert!(inside_active_window(0, 480, 496));
        assert!(!inside_active_window(0, 0, 480));
        assert!(!inside_active_window(0, 496, 600));
        // 15 minutes past the epoch is 08:15 SGT.
        assert!(inside_active_window(900_000, 480, 496));
        assert!(!inside_active_window(900_000, 480, 495));
        // One day later wraps to the same minute of day.
        assert!(inside_active_window(86_400_000, 480, 496));
    }
    #[test]
    fn bracket_titles_carry_their_time_window() {
        // The Unix epoch is 08:00 SGT, so the epoch slot is 08:00 to 08:02.
        assert_eq!(sgt_hhmm(0), "08:00");
        assert_eq!(
            bracket_title("Blue line · arrival at North Spine", 0, 120_000),
            "Blue line · arrival at North Spine · 08:00 to 08:02"
        );
        // A window crossing midnight wraps the clock: 23:59 SGT is
        // 57,540,000 ms past the epoch.
        assert_eq!(
            bracket_title("Bus", 57_540_000, 120_000),
            "Bus · 23:59 to 00:01"
        );
        // An over-long base title is truncated on a character boundary so
        // the suffixed title still fits the instance bound.
        let long = "ä".repeat(240);
        let title = bracket_title(&long, 0, 120_000);
        let suffix = " · 08:00 to 08:02";
        assert!(title.len() <= 240);
        assert!(title.ends_with(suffix));
        assert!(title.is_char_boundary(title.len() - suffix.len()));
    }
    #[test]
    fn elections_require_distinct_fictional_candidates() {
        assert!(
            Rule::Election {
                candidates: vec!["A".into(), "A".into()],
                is_fictional: true
            }
            .validate()
            .is_err()
        );
        assert!(
            Rule::Election {
                candidates: vec!["A".into(), "B".into()],
                is_fictional: false
            }
            .validate()
            .is_err()
        );
    }
}
