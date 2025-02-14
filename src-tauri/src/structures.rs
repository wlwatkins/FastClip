use serde::{Serialize, Deserialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Clip {
    pub id: Uuid,
    value: String,
    label: String,
    icon: String,
    visible: bool,
    clear_time: u64,  // using u64 for clear_time, assuming it's a number like TypeScript's `number`
}