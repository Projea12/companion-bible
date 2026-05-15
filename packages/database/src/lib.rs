mod connection;

pub use connection::{close, connect, DbPool, PoolConfig};

#[cfg(test)]
mod tests;
