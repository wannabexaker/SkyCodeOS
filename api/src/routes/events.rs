use std::collections::VecDeque;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::stream;
use rusqlite::{params, Connection};
use serde::Deserialize;
use serde_json::Value;
use tokio::time::{interval, Interval};

use skycode_contracts::sky_event::{compute_event_id, SkyEvent};
use skycode_contracts::sky_redact::redact_payload;

use crate::state::AppState;

const POLL_INTERVAL: Duration = Duration::from_millis(500);
const KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(15);
const INACTIVITY_TIMEOUT: Duration = Duration::from_secs(60);
const POLL_LIMIT: i64 = 100;

#[derive(Deserialize)]
pub struct EventsQuery {
    #[serde(default)]
    pub after: i64,
    pub task_id: Option<String>,
}

#[derive(Debug)]
struct SkyEventRow {
    rowid: i64,
    task_id: String,
    agent_id: String,
    event_type: String,
    output_json: Option<String>,
    created_at: i64,
}

struct EventStreamState {
    conn: Arc<Mutex<Connection>>,
    task_id: Option<String>,
    cursor: i64,
    pending: VecDeque<Event>,
    interval: Interval,
    last_event_at: Instant,
}

pub async fn handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<EventsQuery>,
) -> Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>> {
    let after = last_event_id(&headers).unwrap_or(params.after);
    let stream_state = EventStreamState {
        conn: state.conn.clone(),
        task_id: params.task_id,
        cursor: after,
        pending: VecDeque::new(),
        interval: interval(POLL_INTERVAL),
        last_event_at: Instant::now(),
    };

    let event_stream = stream::unfold(stream_state, |mut state| async move {
        loop {
            if let Some(event) = state.pending.pop_front() {
                return Some((Ok(event), state));
            }

            if state.last_event_at.elapsed() >= INACTIVITY_TIMEOUT {
                return None;
            }

            state.interval.tick().await;

            let conn = state.conn.clone();
            let task_id = state.task_id.clone();
            let after = state.cursor;
            let query_result =
                tokio::task::spawn_blocking(move || query_event_rows(conn, after, task_id)).await;

            let rows = match query_result {
                Ok(Ok(rows)) => rows,
                Ok(Err(_)) | Err(_) => return None,
            };

            if rows.is_empty() {
                continue;
            }

            state.last_event_at = Instant::now();
            for row in rows {
                state.cursor = row.rowid;
                state.pending.push_back(row.into_event());
            }
        }
    });

    Sse::new(event_stream).keep_alive(KeepAlive::new().interval(KEEP_ALIVE_INTERVAL))
}

fn last_event_id(headers: &HeaderMap) -> Option<i64> {
    headers
        .get("last-event-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<i64>().ok())
}

fn query_event_rows(
    conn: Arc<Mutex<Connection>>,
    after: i64,
    task_id: Option<String>,
) -> Result<Vec<SkyEventRow>, String> {
    let conn_guard = conn
        .lock()
        .map_err(|_| "database lock poisoned".to_string())?;

    if let Some(task_id) = task_id {
        let mut stmt = conn_guard
            .prepare(
                "SELECT rowid, task_id, agent_id, event_type, output_json, created_at
                 FROM tool_events
                 WHERE rowid > ?1 AND task_id = ?2
                 ORDER BY rowid ASC
                 LIMIT ?3",
            )
            .map_err(|e| format!("prepare error: {e}"))?;

        let rows = stmt
            .query_map(params![after, task_id, POLL_LIMIT], sky_event_row)
            .map_err(|e| format!("query error: {e}"))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("row error: {e}"))
    } else {
        let mut stmt = conn_guard
            .prepare(
                "SELECT rowid, task_id, agent_id, event_type, output_json, created_at
                 FROM tool_events
                 WHERE rowid > ?1
                 ORDER BY rowid ASC
                 LIMIT ?2",
            )
            .map_err(|e| format!("prepare error: {e}"))?;

        let rows = stmt
            .query_map(params![after, POLL_LIMIT], sky_event_row)
            .map_err(|e| format!("query error: {e}"))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("row error: {e}"))
    }
}

fn sky_event_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SkyEventRow> {
    Ok(SkyEventRow {
        rowid: row.get(0)?,
        task_id: row.get(1)?,
        agent_id: row.get(2)?,
        event_type: row.get(3)?,
        output_json: row.get(4)?,
        created_at: row.get(5)?,
    })
}

impl SkyEventRow {
    fn into_event(self) -> Event {
        let mut event = SkyEvent {
            event_id: compute_event_id(&self.task_id, self.rowid),
            source: "skycode-api".to_string(),
            cursor: self.rowid,
            task_id: self.task_id,
            agent_id: self.agent_id,
            project_id: "default".to_string(),
            quest_id: None,
            event_type: self.event_type,
            payload: parse_payload(self.output_json),
            created_at: self.created_at.to_string(),
        };

        redact_payload(&mut event.payload);

        let cursor = event.cursor.to_string();
        let data = match serde_json::to_string(&event) {
            Ok(data) => data,
            Err(_) => "{}".to_string(),
        };

        Event::default().id(cursor).data(data)
    }
}

fn parse_payload(raw: Option<String>) -> Value {
    match raw {
        Some(value) => match serde_json::from_str(&value) {
            Ok(parsed) => parsed,
            Err(_) => Value::String(value),
        },
        None => Value::Null,
    }
}
