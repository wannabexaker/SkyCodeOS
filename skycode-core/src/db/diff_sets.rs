use std::borrow::Borrow;

use rusqlite::{params, Connection};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DiffSetError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("diff set membership is frozen: {0}")]
    MembershipFrozen(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffSetRecord {
    pub set_id: String,
    pub task_id: String,
    pub agent_id: String,
    pub project_id: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffSetMember {
    pub set_id: String,
    pub diff_id: String,
    pub ord: i64,
}

pub fn create_diff_set<R>(
    conn: &Connection,
    record: R,
    members: &[(String, i64)],
) -> Result<(), DiffSetError>
where
    R: Borrow<DiffSetRecord>,
{
    let record = record.borrow();
    let exists = conn
        .prepare("SELECT 1 FROM diff_sets WHERE set_id = ?1 LIMIT 1")?
        .exists(params![&record.set_id])?;

    if exists {
        return Err(DiffSetError::MembershipFrozen(record.set_id.clone()));
    }

    let tx = conn.unchecked_transaction()?;

    {
        let mut member_stmt = tx.prepare(
            "INSERT INTO diff_set_members (set_id, diff_id, ord)
             VALUES (?1, ?2, ?3)",
        )?;

        for (diff_id, ord) in members {
            member_stmt.execute(params![&record.set_id, diff_id, ord])?;
        }
    }

    tx.execute(
        "INSERT INTO diff_sets (set_id, task_id, agent_id, project_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            &record.set_id,
            &record.task_id,
            &record.agent_id,
            &record.project_id,
            record.created_at,
        ],
    )?;

    tx.commit()?;
    Ok(())
}

pub fn get_diff_set_members(
    conn: &Connection,
    set_id: &str,
) -> Result<Vec<DiffSetMember>, DiffSetError> {
    let mut stmt = conn.prepare(
        "SELECT set_id, diff_id, ord
         FROM diff_set_members
         WHERE set_id = ?1
         ORDER BY ord ASC",
    )?;

    let rows = stmt.query_map(params![set_id], |row| {
        Ok(DiffSetMember {
            set_id: row.get(0)?,
            diff_id: row.get(1)?,
            ord: row.get(2)?,
        })
    })?;

    let mut members = Vec::new();
    for row in rows {
        members.push(row?);
    }

    Ok(members)
}
