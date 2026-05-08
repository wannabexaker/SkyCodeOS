//! Shared SkyCodeOS core crate.

pub mod approval;
pub mod db;
pub mod skycore;

use uuid::Uuid;

pub fn is_valid_uuid(s: &str) -> bool {
    Uuid::parse_str(s).is_ok()
}
