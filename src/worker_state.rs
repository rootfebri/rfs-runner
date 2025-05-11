use std::fmt;
use std::fmt::{Display, Formatter};

use colored::Colorize;

use crate::{limit_string, MainProgress, Uid, WorkerTemplate};

pub struct WorkerState<S> {
  pub(super) id: Uid,
  pub(super) status: WorkerStatus,
  pub(super) jobs_count: usize,
  pub(super) task: String,
  pub(super) main_progress: MainProgress<S>,
  pub(super) channel_active: usize,
}

impl<S> WorkerState<S>
where
  S: WorkerTemplate,
{
  pub fn new(id: Uid, main_progress: MainProgress<S>) -> Self {
    Self {
      id,
      status: WorkerStatus::Spawned,
      jobs_count: 0,
      task: "Idling..".to_string(),
      main_progress,
      channel_active: 0,
    }
  }

  pub fn update(
    &mut self,
    ch: impl Into<Option<usize>>,
    task: impl Into<Option<String>>,
    jobs: impl Into<Option<usize>>,
    status: impl Into<Option<WorkerStatus>>,
  ) {
    let (ch, task, jobs, status) = (ch.into(), task.into(), jobs.into(), status.into());
    if ch.is_none() && task.is_none() && jobs.is_none() && status.is_none() {
      return;
    }

    if let Some(ch) = ch {
      self.channel_active = ch;
    }
    if let Some(task) = task {
      self.task = limit_string(43, task, None);
    }
    if let Some(jobs) = jobs {
      self.jobs_count = jobs;
    }
    if let Some(status) = status {
      self.status = status;
    }

    self.main_progress.edit_worker(self.id, self.to_string());
  }

  fn id(&self) -> impl Display {
    limit_string(4, self.id, None)
  }

  pub fn channel_active(&self) -> impl Display {
    format!("{}: {}", "Channel".bright_magenta(), self.channel_active)
  }

  fn status(&self) -> impl Display {
    format!("{}: {}", "Status".bright_yellow(), self.status)
  }

  fn jobs(&self) -> impl Display {
    format!("{}: {}", "Jobs Count".bright_yellow(), self.jobs_count)
  }
  fn task(&self) -> impl Display {
    format!("{}: {}", "Task".bright_yellow(), self.task.bright_yellow())
  }

  pub fn set_channel(&mut self, count: usize) {
    self.channel_active = count;
    self.main_progress.edit_worker(self.id, self.to_string());
  }

  pub fn set_status(&mut self, status: WorkerStatus) {
    self.status = status;
    self.main_progress.edit_worker(self.id, format!("{self}"));
  }
  pub fn set_jobs(&mut self, count: usize) {
    self.jobs_count = count;
    self.main_progress.edit_worker(self.id, format!("{self}"));
  }
  pub fn set_task(&mut self, task: impl ToString) {
    self.task = limit_string(43, task, None);
    self.main_progress.edit_worker(self.id, format!("{self}"));
  }
}

#[derive(Clone)]
pub enum WorkerStatus {
  Running,
  Stopped,
  Waiting,
  Spawned,
}

impl Display for WorkerStatus {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    use WorkerStatus::*;
    match *self {
      Running => write!(f, " {{spinner}} {}", "RUNNING".bright_green()),
      Stopped => write!(f, "🛑 {}", "STOPPED".bright_red()),
      Waiting => write!(f, "⌛ {}", "WAITING".bright_magenta()),
      Spawned => write!(f, "⬆️ {}", "SPAWNED".bright_white()),
    }
  }
}

impl<S> Display for WorkerState<S>
where
  S: WorkerTemplate,
{
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    write!(
      f,
      "[ Worker #{} ] {} │ {} │ {} │ {}",
      self.id(),
      self.status(),
      self.jobs(),
      self.channel_active(),
      self.task(),
    )
  }
}

#[macro_export]
macro_rules! wsupdate {
  {
    $worker_state:expr,
    $receiver:expr,
    $task:expr,
    $status:expr$(,)?
  } => {
    $worker_state.update($receiver.sender_strong_count(), String::from($task), $receiver.len(), $status)
  };
}

#[macro_export]
macro_rules! wsupdate_async {
  ($worker_state:expr, $rx:expr$(,)?) => {
    async {
      loop {
        $crate::wsupdate! {
          $worker_state,
          $rx,
          String::from("Processing"),
          $crate::WorkerStatus::Running,
        }
        ::tokio::time::sleep(::core::time::Duration::from_millis(250)).await;
      }
    }
  };
}
