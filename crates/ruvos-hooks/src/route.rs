//! Task routing and model/archetype recommendations.

use serde::{Deserialize, Serialize};

/// Model + archetype recommendation from ruvector-router-core.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteRecommendation {
    pub model: String,
    pub archetype: String,
    pub confidence: f32,
}

/// Get model + archetype recommendation for a task.
pub async fn route_task(_task: &str) -> anyhow::Result<RouteRecommendation> {
    // TODO: Query ruvector-router-core with task description
    // Returns best-fit (model, archetype) pair with confidence score
    Ok(RouteRecommendation {
        model: String::new(),
        archetype: String::new(),
        confidence: 0.0,
    })
}
