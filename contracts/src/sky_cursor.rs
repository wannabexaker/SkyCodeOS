use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct SkyCursor {
    pub after: i64,
    pub limit: usize,
}

impl Default for SkyCursor {
    fn default() -> Self {
        Self {
            after: 0,
            limit: 100,
        }
    }
}
