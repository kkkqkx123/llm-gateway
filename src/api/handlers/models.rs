use crate::api::{handlers::AppState, ApiResult};
use crate::types::{Model, ModelDetail, ModelsResponse};
use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ListModelsQuery {
    pub provider: Option<String>,
    pub capability: Option<String>,
}

pub async fn list_models(
    State(state): State<AppState>,
    Query(query): Query<ListModelsQuery>,
) -> Json<ModelsResponse> {
    let models = if let Some(provider) = query.provider {
        state.registry.get_models_by_provider(&provider).await
    } else {
        state.registry.list_models().await
    };

    let models: Vec<Model> = models
        .into_iter()
        .filter(|model| {
            if let Some(capability) = &query.capability {
                model.capabilities.supports(capability)
            } else {
                true
            }
        })
        .map(|model| Model {
            id: model.id,
            object: "model".to_string(),
            created: model.updated_at.timestamp(),
            owned_by: model.provider,
        })
        .collect();

    Json(ModelsResponse {
        object: "list".to_string(),
        data: models,
    })
}

pub async fn get_model(
    State(state): State<AppState>,
    Path(model_id): Path<String>,
) -> ApiResult<Json<ModelDetail>> {
    let model = state.registry.get_model(&model_id).await?;

    let detail = ModelDetail {
        id: model.id,
        object: "model".to_string(),
        created: model.updated_at.timestamp(),
        owned_by: model.provider,
        capabilities: Some(model.capabilities),
        pricing: model.pricing,
    };

    Ok(Json(detail))
}
