use crate::billing;
use crate::db::DbPool;
use crate::error::AppResult;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
pub struct CatalogModel {
    pub model: String,
    pub model_ratio: Option<f64>,
    pub completion_ratio: Option<f64>,
    pub effective_ratio: Option<f64>,
    pub protocols: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GroupCatalog {
    pub group_name: String,
    pub group_ratio: f64,
    pub models: Vec<CatalogModel>,
}

/// Models the group can route to, with the price the user actually pays.
pub async fn list_for_group(pool: &DbPool, group_name: &str) -> AppResult<GroupCatalog> {
    let names: Vec<(String,)> =
        sqlx::query_as("SELECT DISTINCT model FROM abilities WHERE group_name = ? ORDER BY model")
            .bind(group_name)
            .fetch_all(pool)
            .await?;
    let ratio_row: Option<(f64,)> =
        sqlx::query_as("SELECT ratio FROM group_ratios WHERE group_name = ?")
            .bind(group_name)
            .fetch_optional(pool)
            .await?;
    let group_ratio = ratio_row.map(|r| r.0).unwrap_or(1.0);
    let prices = billing::list_model_prices(pool).await?;
    let by_model: HashMap<String, _> = prices.into_iter().map(|p| (p.model.clone(), p)).collect();

    let models = names
        .into_iter()
        .map(|(model,)| match by_model.get(&model) {
            Some(price) => CatalogModel {
                model,
                model_ratio: Some(price.model_ratio),
                completion_ratio: Some(price.completion_ratio),
                effective_ratio: Some(price.model_ratio * group_ratio),
                protocols: protocol_keys(&price.native_dialects),
            },
            None => CatalogModel {
                model,
                model_ratio: None,
                completion_ratio: None,
                effective_ratio: None,
                protocols: Vec::new(),
            },
        })
        .collect();

    Ok(GroupCatalog {
        group_name: group_name.to_string(),
        group_ratio,
        models,
    })
}

fn protocol_keys(native_dialects: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(native_dialects) else {
        return Vec::new();
    };
    let Some(obj) = value.as_object() else {
        return Vec::new();
    };
    let mut keys: Vec<String> = obj.keys().cloned().collect();
    keys.sort();
    keys
}
