//! Platform-neutral login-session contract.

use serde::{Deserialize, Serialize};

use super::target::SessionId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SessionControlAction {
    Disconnect,
    Lock,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SessionItem {
    pub id: SessionId,
    pub uid: u32,
    pub user: String,
    pub seat: Option<String>,
    pub tty: Option<String>,
    pub remote: bool,
    pub timestamp: Option<String>,
}
