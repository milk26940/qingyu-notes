pub fn cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
    if left.len() != right.len() || left.is_empty() { return 0.0; }
    let mut dot = 0.0;
    let mut left_norm = 0.0;
    let mut right_norm = 0.0;
    for (a, b) in left.iter().zip(right) {
        dot += a * b;
        left_norm += a * a;
        right_norm += b * b;
    }
    if left_norm == 0.0 || right_norm == 0.0 { 0.0 } else { dot / (left_norm.sqrt() * right_norm.sqrt()) }
}

use rusqlite::Connection;

pub fn initialize(connection: &Connection) -> Result<(), String> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS embedding_models (id TEXT PRIMARY KEY, provider TEXT NOT NULL, model TEXT NOT NULL, dimension INTEGER NOT NULL, index_version INTEGER NOT NULL DEFAULT 1, enabled INTEGER NOT NULL DEFAULT 0, updated_at TEXT NOT NULL);\
         CREATE TABLE IF NOT EXISTS block_embeddings (block_id TEXT PRIMARY KEY, page_id TEXT NOT NULL, content_hash TEXT NOT NULL, modality TEXT NOT NULL, model_id TEXT NOT NULL, dimension INTEGER NOT NULL, vector BLOB NOT NULL, updated_at TEXT NOT NULL);\
         CREATE TABLE IF NOT EXISTS embedding_jobs (id TEXT PRIMARY KEY, block_id TEXT NOT NULL, status TEXT NOT NULL, attempts INTEGER NOT NULL DEFAULT 0, error TEXT, updated_at TEXT NOT NULL);\
         CREATE INDEX IF NOT EXISTS idx_block_embeddings_page ON block_embeddings(page_id);\
         CREATE INDEX IF NOT EXISTS idx_embedding_jobs_status ON embedding_jobs(status);"
    ).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::cosine_similarity;

    #[test]
    fn cosine_similarity_identifies_same_direction() {
        assert_eq!(cosine_similarity(&[1.0, 2.0], &[2.0, 4.0]), 1.0);
    }
}
