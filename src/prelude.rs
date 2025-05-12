use std::num::NonZeroU32;

pub use super::helper::limit_string;
pub(crate) use super::helper::*;
pub use super::templates::*;
pub use super::worker_handle::{WorkerHandle, WorkerHandleBuilder};
pub use super::worker_pool::WorkerPool;
pub use super::worker_progress::MainProgress;
pub use super::worker_state::{WorkerState, WorkerStatus};

pub type Uid = NonZeroU32;
pub type Result<T, E = crate::error::Error> = std::result::Result<T, E>;
