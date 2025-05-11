#[derive(thiserror::Error, Debug)]
pub enum Error {
  #[error("No channel available")]
  NoChannel,
  #[error("{0}")]
  Missing(&'static str),
}
