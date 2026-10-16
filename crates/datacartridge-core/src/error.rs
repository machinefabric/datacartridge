use serde::{Deserialize, Serialize};

pub type Result<T> = std::result::Result<T, Error>;

/// A candidate offered when an identity query cannot be resolved to exactly one entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AmbiguityCandidate {
    pub id: String,
    pub label: String,
    pub detail: String,
}

/// The single error type of the platform. Every variant names a state the caller must
/// handle deliberately; nothing here is a catch-all for "something went wrong".
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{kind} not found: {id}")]
    NotFound { kind: &'static str, id: String },

    #[error("invalid {what}: {reason}")]
    Invalid { what: &'static str, reason: String },

    #[error("'{query}' is ambiguous between {} candidates", candidates.len())]
    Ambiguous { query: String, candidates: Vec<AmbiguityCandidate> },

    #[error("conflict on {what}: {reason}")]
    Conflict { what: &'static str, reason: String },

    #[error("illegal state transition for {what}: {reason}")]
    IllegalState { what: &'static str, reason: String },

    #[error("action '{action}' forbidden for {principal}: {reason}")]
    Forbidden { principal: String, action: String, reason: String },

    #[error("action '{action}' by {principal} requires {level} approval (policy '{policy}')")]
    ApprovalRequired { principal: String, action: String, level: String, policy: String },

    #[error("determinism violation in {run}: {reason}")]
    Determinism { run: String, reason: String },

    #[error("{what} unavailable: {reason}")]
    Unavailable { what: String, reason: String },

    #[error("risk check rejected: {}", reasons.join("; "))]
    RiskRejected { reasons: Vec<String> },

    #[error("storage failure: {0}")]
    Storage(String),

    #[error("integrity violation: {0}")]
    Integrity(String),

    /// The host's network policy does not allow reaching a service now (capdag 12.2
    /// §Policy). Nothing was sent; it holds until the operator changes the setting.
    #[error("{0}")]
    NetworkRefused(String),
}

impl Error {
    pub fn invalid(what: &'static str, reason: impl Into<String>) -> Self {
        Error::Invalid { what, reason: reason.into() }
    }

    pub fn not_found(kind: &'static str, id: impl Into<String>) -> Self {
        Error::NotFound { kind, id: id.into() }
    }

    pub fn conflict(what: &'static str, reason: impl Into<String>) -> Self {
        Error::Conflict { what, reason: reason.into() }
    }

    pub fn illegal(what: &'static str, reason: impl Into<String>) -> Self {
        Error::IllegalState { what, reason: reason.into() }
    }

    pub fn unavailable(what: impl Into<String>, reason: impl Into<String>) -> Self {
        Error::Unavailable { what: what.into(), reason: reason.into() }
    }

    /// Stable machine-readable code, used by the API and CLI layers.
    pub fn code(&self) -> &'static str {
        match self {
            Error::NotFound { .. } => "not_found",
            Error::Invalid { .. } => "invalid",
            Error::Ambiguous { .. } => "ambiguous",
            Error::Conflict { .. } => "conflict",
            Error::IllegalState { .. } => "illegal_state",
            Error::Forbidden { .. } => "forbidden",
            Error::ApprovalRequired { .. } => "approval_required",
            Error::Determinism { .. } => "determinism_violation",
            Error::Unavailable { .. } => "unavailable",
            Error::RiskRejected { .. } => "risk_rejected",
            Error::Storage(_) => "storage",
            Error::Integrity(_) => "integrity",
            Error::NetworkRefused(_) => "network_refused",
        }
    }
}
