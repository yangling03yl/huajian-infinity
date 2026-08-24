use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteMeta {
    pub id: String,
    pub title: String,
    pub color: String,
    pub file: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoxIndex {
    pub version: u32,
    pub name: String,
    pub created_at: String,
    pub notes: Vec<NoteMeta>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BoxInfo {
    pub id: String,
    pub path: String,
    pub name: String,
    pub note_count: usize,
}
