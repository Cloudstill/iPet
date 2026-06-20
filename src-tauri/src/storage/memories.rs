//! `memories` table — long-term, cross-session memory the model can read/write.
//!
//! Memories persist facts and preferences the model chose to remember (via the
//! `memory_save` tool) and are surfaced two ways (ref-plan §memory, Tier 1):
//!   1. A small "recently relevant" slice is appended to the system prompt on
//!      every turn (`recent_memories`) — stable, always-on context.
//!   2. The `memory_search` tool lets the model pull additional memories on
//!      demand when a query seems related.
//! Both paths bump `last_used_at` / `use_count` so stale memories are visible
//! in the management UI and can be pruned.

use super::{AppResult, Memory, Storage};
use chrono::Utc;
use rusqlite::params;

impl Storage {
    /// Insert or update a memory keyed by `key`. Same-key writes update the
    /// content/category and bump `updated_at` (a memory the model re-saves is
    /// treated as freshly relevant). Returns the upserted row.
    pub fn save_memory(&self, key: &str, content: &str, category: &str) -> AppResult<Memory> {
        let key = key.trim();
        if key.is_empty() {
            return Err(crate::app_error::AppError::InvalidInput(
                "memory key must not be empty".to_string(),
            ));
        }
        let category = if category.trim().is_empty() {
            "general"
        } else {
            category.trim()
        };
        let now = Utc::now().to_rfc3339();
        let conn = self.lock()?;

        // upsert: if the key exists, refresh content/category/updated_at and
        // keep use_count + last_used_at (a re-save shouldn't fake a use).
        let changed = conn.execute(
            "UPDATE memories
             SET content = ?1, category = ?2, updated_at = ?3
             WHERE key = ?4",
            params![content, category, now, key],
        )?;
        if changed == 0 {
            conn.execute(
                "INSERT INTO memories (key, content, category, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?4)",
                params![key, content, category, now],
            )?;
        }
        drop(conn);
        self.get_memory_by_key(key)?
            .ok_or_else(|| crate::app_error::AppError::Config("memory vanished after upsert".to_string()))
    }

    pub fn get_memory_by_key(&self, key: &str) -> AppResult<Option<Memory>> {
        let conn = self.lock()?;
        let memory = conn
            .query_row(
                "SELECT id, key, content, category, created_at, updated_at, last_used_at, use_count
                 FROM memories WHERE key = ?1",
                params![key],
                read_memory_row,
            )
            .ok();
        Ok(memory)
    }

    /// All memories, newest-updated first. Backs the management UI.
    pub fn list_memories(&self) -> AppResult<Vec<Memory>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT id, key, content, category, created_at, updated_at, last_used_at, use_count
             FROM memories
             ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], read_memory_row)?;
        let mut memories = Vec::new();
        for row in rows {
            memories.push(row?);
        }
        Ok(memories)
    }

    /// The `limit` most-recently-updated memories for stable system-prompt
    /// injection. Bounded so the prompt can't grow unbounded over time.
    pub fn recent_memories(&self, limit: usize) -> AppResult<Vec<Memory>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT id, key, content, category, created_at, updated_at, last_used_at, use_count
             FROM memories
             ORDER BY updated_at DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], read_memory_row)?;
        let mut memories = Vec::new();
        for row in rows {
            memories.push(row?);
        }
        Ok(memories)
    }

    /// Substring search across key + content + category (case-insensitive).
    /// Lightweight — no embeddings; this is Tier 1, not full RAG. Each hit has
    /// its `last_used_at` / `use_count` bumped so usage-driven pruning works.
    pub fn search_memories(&self, query: &str, limit: usize) -> AppResult<Vec<Memory>> {
        let query = query.trim();
        if query.is_empty() {
            return self.recent_memories(limit);
        }
        let like = format!("%{}%", query.to_lowercase());
        let now = Utc::now().to_rfc3339();
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT id, key, content, category, created_at, updated_at, last_used_at, use_count
             FROM memories
             WHERE LOWER(key) LIKE ?1
                OR LOWER(content) LIKE ?1
                OR LOWER(category) LIKE ?1
             ORDER BY updated_at DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![like, limit as i64], read_memory_row)?;
        let mut hits = Vec::new();
        let mut ids = Vec::new();
        for row in rows {
            let m = row?;
            ids.push(m.id);
            hits.push(m);
        }
        drop(stmt);
        // Record usage for matched memories so the management UI can surface
        // what the model actually consults.
        for id in &ids {
            let _ = conn.execute(
                "UPDATE memories SET last_used_at = ?1, use_count = use_count + 1 WHERE id = ?2",
                params![now, id],
            );
        }
        Ok(hits)
    }

    /// Update a memory's content/category from the management UI. Returns the
    /// updated row, or None if no memory has `id`.
    pub fn update_memory(
        &self,
        id: i64,
        content: &str,
        category: Option<&str>,
    ) -> AppResult<Option<Memory>> {
        let now = Utc::now().to_rfc3339();
        let conn = self.lock()?;
        let touched = if let Some(category) = category {
            let category = if category.trim().is_empty() {
                "general"
            } else {
                category.trim()
            };
            conn.execute(
                "UPDATE memories SET content = ?1, category = ?2, updated_at = ?3 WHERE id = ?4",
                params![content, category, now, id],
            )?
        } else {
            conn.execute(
                "UPDATE memories SET content = ?1, updated_at = ?2 WHERE id = ?3",
                params![content, now, id],
            )?
        };
        if touched == 0 {
            return Ok(None);
        }
        drop(conn);
        let conn = self.lock()?;
        let memory = conn
            .query_row(
                "SELECT id, key, content, category, created_at, updated_at, last_used_at, use_count
                 FROM memories WHERE id = ?1",
                params![id],
                read_memory_row,
            )
            .ok();
        Ok(memory)
    }

    pub fn delete_memory(&self, id: i64) -> AppResult<bool> {
        let conn = self.lock()?;
        let removed = conn.execute("DELETE FROM memories WHERE id = ?1", params![id])?;
        Ok(removed > 0)
    }
}

fn read_memory_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Memory> {
    Ok(Memory {
        id: row.get(0)?,
        key: row.get(1)?,
        content: row.get(2)?,
        category: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        last_used_at: row.get(6)?,
        use_count: row.get(7)?,
    })
}
