// SPDX-License-Identifier: GPL-3.0-or-later
//! The effect outbox as the indexer's [`EffectLog`] (docs/specs/catalogo/busca.md).

use elzevir_app::search::{Effect, EffectLog, EffectLogError, LoggedEffect, REINDEX_INSTANCE};
use elzevir_domain::catalog::InstanceId;
use rusqlite::params;

use crate::{CommandHandler, Store};

/// Payload of [`REINDEX_INSTANCE`].
#[derive(serde::Deserialize)]
struct ReindexPayload {
    instance: i64,
}

impl<H: CommandHandler> EffectLog for Store<H>
where
    Store<H>: Send + Sync,
{
    fn after(&self, cursor: i64, limit: usize) -> Result<Vec<LoggedEffect>, EffectLogError> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let rows = self
            .readers()
            .read(|c| {
                let mut statement = c.prepare(
                    "SELECT id, kind, payload FROM effect_outbox WHERE id > ?1 ORDER BY id LIMIT ?2",
                )?;
                statement
                    .query_map(params![cursor, limit], |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    })?
                    .collect::<Result<Vec<_>, _>>()
            })
            .map_err(|e: rusqlite::Error| EffectLogError(e.to_string()))?;
        rows.into_iter()
            .map(|(id, kind, payload)| {
                let effect = if kind == REINDEX_INSTANCE {
                    let payload: ReindexPayload = serde_json::from_str(&payload)
                        .map_err(|e| EffectLogError(format!("effect {id} ({kind}): {e}")))?;
                    Effect::ReindexInstance(InstanceId(payload.instance))
                } else {
                    Effect::Other
                };
                Ok(LoggedEffect { id, effect })
            })
            .collect()
    }
}
