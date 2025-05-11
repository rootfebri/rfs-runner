use std::fmt::Display;
use std::ops::Deref;
use std::time::Duration;

use indicatif::{MultiProgress, ProgressBar, ProgressFinish};

use crate::templates::WorkerTemplate;
use crate::{line_err, line_ok, Uid};

pub struct MainProgress<S> {
  style: S,
  progress: ProgressBar,
  multi_progress: MultiProgress,
}

impl<S> MainProgress<S>
where
  S: WorkerTemplate,
{
  pub fn new(length: u64, multi_progress: MultiProgress, style: S) -> Self {
    let progress = ProgressBar::new(length)
      .with_finish(ProgressFinish::AndLeave)
      .with_style(style.as_progress_style());

    progress.enable_steady_tick(Duration::from_millis(250));
    multi_progress.insert(0, progress.clone());

    Self {
      style,
      progress,
      multi_progress,
    }
  }

  pub fn add_worker(&self, id: Uid, worker: impl Display) {
    self.style.add_worker(id, worker.to_string());
    self.progress.set_style(self.style.as_progress_style());
  }

  pub fn remove_worker(&self, id: &Uid) {
    self.style.remove_worker(id);
    self.progress.set_style(self.style.as_progress_style());
  }

  pub fn edit_worker(&self, id: Uid, worker: impl Display) {
    self.style.edit_worker(&id, worker.to_string());
    self.progress.set_style(self.style.as_progress_style());
  }

  pub fn println(&self, line: impl AsRef<str>) {
    _ = self.multi_progress.println(line_ok(line.as_ref()));
  }

  pub fn eprintln(&self, error: impl AsRef<str>) {
    _ = self.multi_progress.println(line_err(error.as_ref()));
  }

  pub fn increment(&self, count: usize) {
    self.style.increment(count);
  }
}

impl<S: Clone> Clone for MainProgress<S> {
  fn clone(&self) -> Self {
    Self {
      style: self.style.clone(),
      progress: self.progress.clone(),
      multi_progress: self.multi_progress.clone(),
    }
  }
}

impl<S> Deref for MainProgress<S> {
  type Target = ProgressBar;

  fn deref(&self) -> &Self::Target {
    &self.progress
  }
}
