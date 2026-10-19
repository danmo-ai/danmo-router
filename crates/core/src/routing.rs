use crate::db::DbPool;
use crate::dialect::{self, Dialect};
use crate::error::{AppError, AppResult};
use crate::models::{Channel, ChannelModel};
use crate::protocol::Protocol;
use rand::Rng;

#[derive(Debug, Clone)]
pub struct ChannelCandidate {
    pub channel: Channel,
    pub row: ChannelModel,
}

impl ChannelCandidate {
    pub fn upstream_model(&self) -> &str {
        self.row.upstream_name()
    }

    pub fn dialect(&self) -> &'static Dialect {
        dialect::resolve(self.channel.protocol(), &self.row.upstream_dialect)
    }

    /// Opaque rows are passthrough-only, so they cannot take another protocol.
    fn serves(&self, inbound: Protocol) -> bool {
        self.channel.serves(inbound)
            && !(self.dialect().opaque && self.channel.protocol() != inbound)
    }
}

/// Select channels for a group+model that can serve the inbound protocol, ordered by priority
/// (desc), weight-random within same priority.
pub async fn select_channels(
    pool: &DbPool,
    group_name: &str,
    model: &str,
    inbound: Protocol,
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

    let mut model_rows: Vec<ChannelModel> =
        sqlx::query_as("SELECT * FROM channel_models WHERE model = ?")
            .bind(model)
            .fetch_all(pool)
            .await?;

    let mut candidates: Vec<ChannelCandidate> = Vec::new();
    for channel in rows {
        if exclude_ids.contains(&channel.id) {
            continue;
        }
        let Some(i) = model_rows.iter().position(|r| r.channel_id == channel.id) else {
            continue;
        };
        let row = model_rows.swap_remove(i);
        let candidate = ChannelCandidate { channel, row };
        if candidate.serves(inbound) {
            candidates.push(candidate);
        }
    }

    if candidates.is_empty() {
        return Err(AppError::NotFound(format!(
            "no available {inbound} channel for model '{model}' in group '{group_name}'"
        )));
    }

    Ok(weighted_shuffle_by_priority(candidates))
}

fn weighted_shuffle_by_priority(mut channels: Vec<ChannelCandidate>) -> Vec<ChannelCandidate> {
    if channels.is_empty() {
        return channels;
    }

    channels.sort_by(|a, b| {
        b.channel
            .priority
            .cmp(&a.channel.priority)
            .then(a.channel.id.cmp(&b.channel.id))
    });

    let mut result = Vec::with_capacity(channels.len());
    let mut remaining = channels;
    let mut rng = rand::thread_rng();

    while !remaining.is_empty() {
        let top_priority = remaining[0].channel.priority;
        let mut same: Vec<ChannelCandidate> = Vec::new();
        let mut rest: Vec<ChannelCandidate> = Vec::new();
        for ch in remaining {
            if ch.channel.priority == top_priority {
                same.push(ch);
            } else {
                rest.push(ch);
            }
        }

        while !same.is_empty() {
            let total_weight: i64 = same.iter().map(|c| c.channel.weight.max(1)).sum();
            let mut pick = rng.gen_range(0..total_weight);
            let mut idx = 0;
            for (i, ch) in same.iter().enumerate() {
                pick -= ch.channel.weight.max(1);
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

    fn ch(id: i64, priority: i64, weight: i64) -> ChannelCandidate {
        let channel = Channel {
            id,
            name: format!("c{id}"),
            type_: "openai_compat".into(),
            key_enc: String::new(),
            base_url: "https://example.com".into(),
            group_name: "default".into(),
            priority,
            weight,
            status: "enabled".into(),
            created_at: String::new(),
            updated_at: String::new(),
            relay_mode: "passthrough".into(),
            upstream_protocol: "openai_chat".into(),
            auth_style: "bearer".into(),
            inject_stream_usage: 1,
            default_dialect: "standard".into(),
            vendor: String::new(),
        };
        let row = ChannelModel {
            id,
            channel_id: id,
            model: "gpt-4o-mini".into(),
            upstream_model: String::new(),
            upstream_dialect: "standard".into(),
        };
        ChannelCandidate { channel, row }
    }

    #[test]
    fn higher_priority_first() {
        let channels = vec![ch(1, 1, 1), ch(2, 10, 1), ch(3, 5, 1)];
        let ordered = weighted_shuffle_by_priority(channels);
        assert_eq!(ordered[0].channel.id, 2);
        assert_eq!(ordered[1].channel.id, 3);
        assert_eq!(ordered[2].channel.id, 1);
    }

    #[test]
    fn weight_respects_single() {
        let channels = vec![ch(1, 1, 1)];
        let ordered = weighted_shuffle_by_priority(channels);
        assert_eq!(ordered[0].channel.id, 1);
    }
}
