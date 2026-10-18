use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::Channel;
use rand::Rng;

#[derive(Debug, Clone)]
pub struct ChannelCandidate {
    pub channel: Channel,
}

/// Select channels for a group+model, ordered by priority (desc), weight-random within same priority.
pub async fn select_channels(
    pool: &DbPool,
    group_name: &str,
    model: &str,
    exclude_ids: &[i64],
) -> AppResult<Vec<ChannelCandidate>> {
    let rows: Vec<Channel> = sqlx::query_as(
        r#"
        SELECT c.*
        FROM channels c
        INNER JOIN abilities a ON a.channel_id = c.id
        WHERE a.group_name = ?
          AND a.model = ?
          AND c.status = 'enabled'
        ORDER BY c.priority DESC, c.id ASC
        "#,
    )
    .bind(group_name)
    .bind(model)
    .fetch_all(pool)
    .await?;

    let filtered: Vec<Channel> = rows
        .into_iter()
        .filter(|c| !exclude_ids.contains(&c.id))
        .collect();

    if filtered.is_empty() {
        return Err(AppError::NotFound(format!(
            "no available channel for model '{model}' in group '{group_name}'"
        )));
    }

    Ok(weighted_shuffle_by_priority(filtered)
        .into_iter()
        .map(|channel| ChannelCandidate { channel })
        .collect())
}

fn weighted_shuffle_by_priority(mut channels: Vec<Channel>) -> Vec<Channel> {
    if channels.is_empty() {
        return channels;
    }

    channels.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.id.cmp(&b.id)));

    let mut result = Vec::with_capacity(channels.len());
    let mut remaining = channels;
    let mut rng = rand::thread_rng();

    while !remaining.is_empty() {
        let top_priority = remaining[0].priority;
        let mut same: Vec<Channel> = Vec::new();
        let mut rest: Vec<Channel> = Vec::new();
        for ch in remaining {
            if ch.priority == top_priority {
                same.push(ch);
            } else {
                rest.push(ch);
            }
        }

        while !same.is_empty() {
            let total_weight: i64 = same.iter().map(|c| c.weight.max(1)).sum();
            let mut pick = rng.gen_range(0..total_weight);
            let mut idx = 0;
            for (i, ch) in same.iter().enumerate() {
                pick -= ch.weight.max(1);
                if pick < 0 {
                    idx = i;
                    break;
                }
            }
            result.push(same.remove(idx));
        }
        remaining = rest;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ch(id: i64, priority: i64, weight: i64) -> Channel {
        Channel {
            id,
            name: format!("c{id}"),
            type_: "openai_compat".into(),
            key_enc: String::new(),
            base_url: "https://example.com".into(),
            models: "gpt-4o-mini".into(),
            group_name: "default".into(),
            priority,
            weight,
            status: "enabled".into(),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn higher_priority_first() {
        let channels = vec![ch(1, 1, 1), ch(2, 10, 1), ch(3, 5, 1)];
        let ordered = weighted_shuffle_by_priority(channels);
        assert_eq!(ordered[0].id, 2);
        assert_eq!(ordered[1].id, 3);
        assert_eq!(ordered[2].id, 1);
    }

    #[test]
    fn weight_respects_single() {
        let channels = vec![ch(1, 1, 1)];
        let ordered = weighted_shuffle_by_priority(channels);
        assert_eq!(ordered[0].id, 1);
    }
}
