use axum::{extract::Extension, http::StatusCode, response::IntoResponse, Json };
use serde::Serialize;
use std::sync::Arc;
use crate::AppState;
use crate::healthcheck::monitor::check_health;

#[derive(Serialize)]
struct HealthCheckResponse {
    database: String,
    notifier: String,
    audit: String,
    bot: String
}

pub(crate) async fn health_check_handler(
    Extension(state): Extension<Arc<AppState>>,
) -> impl IntoResponse {
    // Check Current Health Status
    let current_status = check_health(&state).await;

    // Map CurrentHealthStatus to HealthCheckResponse
    let response = HealthCheckResponse {
        database: current_status.database,
        notifier: current_status.notifier,
        audit: current_status.audit,
        bot: current_status.bot
    };

    // Determine Overall HTTP Status Code
    let overall_status = if response.database == "ok" && response.notifier == "ok" && response.audit == "ok" && response.bot == "ok" {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    // Return JSON Response with Appropriate Status
    (overall_status, Json(response))
}