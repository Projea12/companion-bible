mod connection;
pub mod migration;
pub mod models;

pub use connection::{close, connect, DbPool, PoolConfig};
pub use migration::AppliedMigration;
pub use models::{
    CalibrationThresholds, Church, ChurchSettings, DetectionEvent, Sermon, ServiceRecord,
    SubPoint, Verse,
};

#[cfg(test)]
mod tests;
