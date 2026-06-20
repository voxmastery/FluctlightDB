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
    #[error("serialization error: {0}")]
    Serde(String),
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
