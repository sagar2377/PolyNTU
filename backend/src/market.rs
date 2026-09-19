//! Category rules transform observed evidence into one outcome. No category prices trades.
use crate::{
    amm,
    error::{Result, invalid},
};
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
    /// Participant account credited with the creator's half of the settled fee
    /// pot. Absent for platform-created instances, whose fees go to the treasury.
    #[serde(default)]
    pub creator_account_id: Option<String>,
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
    pub creator_account_id: Option<String>,
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

pub fn demo_specs(now: i64) -> Vec<NewInstance> {
    let close = (now / 900000 + 1) * 900000;
    let definitions = vec![
        (
            "bus-blue",
            "Blue line · arrival at North Spine",
            Rule::Bus {
                route_id: "NTU-blue".into(),
                direction: "clockwise".into(),
                stop_id: "north-spine".into(),
            },
            600000,
        ),
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
            evidence_deadline_ms: close + duration + 60000, liquidity_units: 100, creator_account_id: None }
    }).collect()
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
        let categories: std::collections::HashSet<_> =
            specs.iter().map(|s| s.rule.category()).collect();
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
