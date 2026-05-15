mod connection;
pub mod migration;
pub mod models;
pub mod repositories;

pub use connection::{close, connect, DbPool, PoolConfig};
pub use migration::AppliedMigration;
pub use models::{
    CalibrationThresholds, Church, ChurchSettings, DetectionEvent, Sermon, ServiceRecord,
    SubPoint, Verse,
};
pub use repositories::{
    CalibrationRepository, ChurchRepository, DetectionEventRepository, SermonRepository,
    VerseRepository,
};

#[cfg(test)]
mod tests;
