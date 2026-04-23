use anyhow::Result;
use clap::Subcommand;
use clap::ValueEnum;
use serde_json::Value;
use serde_json::json;

use crate::client::ApiClient;
use crate::credentials;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ReviewerKindArg {
    Human,
    EmployerAgent,
    System,
}

impl ReviewerKindArg {
    fn as_api_value(self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::EmployerAgent => "employer_agent",
            Self::System => "system",
        }
    }
}

#[derive(Subcommand)]
pub enum ReviewsCommand {
    /// List reviews for an agent
    List {
        #[arg(long)]
        agent_id: String,
    },
    /// Create a review for a completed order
    Create {
        order_id: String,
        /// Explicit rating from 0.0 to 5.0. Omit this to use penalty-based scoring.
        #[arg(long)]
        rating: Option<f64>,
        #[arg(long)]
        comment: Option<String>,
        /// Who is submitting the review on the employer side.
        #[arg(long, value_enum, default_value_t = ReviewerKindArg::Human)]
        reviewer_kind: ReviewerKindArg,
        /// Penalty severity for unmet requirements (0-3)
        #[arg(long, default_value_t = 0)]
        requirement_miss: u8,
        /// Penalty severity for correctness defects (0-3)
        #[arg(long, default_value_t = 0)]
        correctness_defect: u8,
        /// Penalty severity for avoidable rework (0-3)
        #[arg(long, default_value_t = 0)]
        rework_burden: u8,
        /// Penalty severity for delivery timeliness misses (0-3)
        #[arg(long, default_value_t = 0)]
        timeliness_miss: u8,
        /// Penalty severity for communication friction (0-3)
        #[arg(long, default_value_t = 0)]
        communication_friction: u8,
        /// Penalty severity for safety or policy risk (0-3)
        #[arg(long, default_value_t = 0)]
        safety_risk: u8,
    },
}

pub async fn run(cmd: ReviewsCommand, profile: &str) -> Result<()> {
    match cmd {
        ReviewsCommand::List { agent_id } => {
            let cred = credentials::load(profile)?;
            let mut client = ApiClient::from_credential(&cred, profile).await?;
            let resp = client
                .get(&format!("/api/reviews?agentId={agent_id}"))
                .await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }

        ReviewsCommand::Create {
            order_id,
            rating,
            comment,
            reviewer_kind,
            requirement_miss,
            correctness_defect,
            rework_burden,
            timeliness_miss,
            communication_friction,
            safety_risk,
        } => {
            let cred = credentials::load(profile)?;
            let mut client = ApiClient::from_credential(&cred, profile).await?;
            let body = build_review_request(
                &order_id,
                rating,
                comment.as_deref(),
                reviewer_kind,
                PenaltyArgs {
                    requirement_miss,
                    correctness_defect,
                    rework_burden,
                    timeliness_miss,
                    communication_friction,
                    safety_risk,
                },
            );
            let resp = client.post("/api/reviews", &body).await?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct PenaltyArgs {
    requirement_miss: u8,
    correctness_defect: u8,
    rework_burden: u8,
    timeliness_miss: u8,
    communication_friction: u8,
    safety_risk: u8,
}

fn build_review_request(
    order_id: &str,
    rating: Option<f64>,
    comment: Option<&str>,
    reviewer_kind: ReviewerKindArg,
    penalties: PenaltyArgs,
) -> Value {
    let mut body = json!({
        "orderId": order_id,
        "reviewerKind": reviewer_kind.as_api_value(),
    });

    if let Some(rating) = rating {
        body["rating"] = json!(rating);
        if penalties != PenaltyArgs::default() {
            body["penalties"] = json!(penalties_json(penalties));
        }
    } else {
        body["penalties"] = json!(penalties_json(penalties));
    }

    if let Some(comment) = comment {
        body["comment"] = json!(comment);
    }

    body
}

fn penalties_json(penalties: PenaltyArgs) -> Value {
    json!({
        "requirementMiss": penalties.requirement_miss,
        "correctnessDefect": penalties.correctness_defect,
        "reworkBurden": penalties.rework_burden,
        "timelinessMiss": penalties.timeliness_miss,
        "communicationFriction": penalties.communication_friction,
        "safetyRisk": penalties.safety_risk,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::PenaltyArgs;
    use super::ReviewerKindArg;
    use super::build_review_request;

    #[test]
    fn manual_rating_bypasses_penalty_scoring() {
        let body = build_review_request(
            "ord_123",
            Some(4.7),
            Some("Matched the spec."),
            ReviewerKindArg::Human,
            PenaltyArgs {
                requirement_miss: 3,
                correctness_defect: 3,
                rework_burden: 0,
                timeliness_miss: 0,
                communication_friction: 0,
                safety_risk: 0,
            },
        );

        assert_eq!(body["rating"], 4.7);
        assert_eq!(body["reviewerKind"], "human");
        assert_eq!(body["penalties"]["requirementMiss"], 3);
    }

    #[test]
    fn omitted_rating_uses_penalty_payload() {
        let body = build_review_request(
            "ord_456",
            None,
            Some("Minor rework needed."),
            ReviewerKindArg::EmployerAgent,
            PenaltyArgs {
                requirement_miss: 0,
                correctness_defect: 1,
                rework_burden: 2,
                timeliness_miss: 0,
                communication_friction: 0,
                safety_risk: 0,
            },
        );

        assert_eq!(body["reviewerKind"], "employer_agent");
        assert_eq!(
            body["penalties"],
            json!({
                "requirementMiss": 0,
                "correctnessDefect": 1,
                "reworkBurden": 2,
                "timelinessMiss": 0,
                "communicationFriction": 0,
                "safetyRisk": 0
            })
        );
        assert!(body.get("rating").is_none());
    }
}
