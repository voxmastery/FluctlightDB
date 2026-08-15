use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("region locked until development stage {required:?} (current: {current:?})")]
    RegionLocked {
        required: crate::development::DevStage,
        current: crate::development::DevStage,
    },
    #[error("embryonic stage: only reflex encoding allowed")]
    EmbryonicOnlyReflex,
    #[error("life has ended; start a new life with life_start()")]
    LifeEnded,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("store error: {0}")]
    Store(String),
    #[error(
        "not primary at placement generation {generation}; primary={primary:?}, api={api_addr:?}"
    )]
    NotPrimary {
        primary: Option<u64>,
        generation: u64,
        api_addr: Option<String>,
    },
    #[error("placement unavailable: {0}")]
    PlacementUnavailable(String),
    #[error("read consistency unavailable: {0}")]
    ReadConsistencyUnavailable(String),
    #[error(
        "durability unavailable for {policy} write at watermark {watermark}: required {required} durable copies, received {received}"
    )]
    DurabilityUnavailable {
        policy: String,
        watermark: u64,
        required: usize,
        received: usize,
    },
    #[error("durable mutation {operation} is disabled in distributed production")]
    DistributedMutationDisabled { operation: &'static str },
    #[error("serialization error: {0}")]
    Serde(String),
    #[error(transparent)]
    Swarm(#[from] crate::swarm::SwarmError),
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
