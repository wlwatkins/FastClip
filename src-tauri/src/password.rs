use lazy_static::lazy_static;
use std::sync::Arc;
use tokio::sync::Mutex;

lazy_static! {
    pub static ref PASSWORD: Arc<Mutex<Option<Vec<u8>>>> = Arc::new(Mutex::new(None));
}
