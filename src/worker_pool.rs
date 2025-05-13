//! `WorkerPool` manages a pool of worker threads for concurrent task execution.
//!
//! It provides functionalities to spawn workers, send data to them,
//! and manage their lifecycle, including graceful shutdown on receiving signals.
//!
//! # Examples
//!
//! ```no_run
//! use std::sync::Arc;
//!
//! use anyhow::{anyhow, Result};
//! use lettre::message::{Mailbox, MessageBuilder};
//! use lettre::transport::smtp::authentication::{Credentials, Mechanism};
//! use lettre::transport::smtp::PoolConfig;
//! use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};
//! use rfs_runner::{DefaultTemplate, MainProgress, WorkerPool, WorkerTemplate};
//! use tokio::io::{AsyncBufReadExt, AsyncSeekExt};
//! const MAX_WORKER: u32 = 8;
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!   use std::env::temp_dir;
//!
//!   let smtp_credentials = Credentials::new("example@gmail.com".to_owned(), "KeepSecretPasswordFromCommit".to_owned());
//!   let pool_config = PoolConfig::new().max_size(MAX_WORKER);
//!   let smtp_server: Arc<AsyncSmtpTransport<Tokio1Executor>> = Arc::new(
//!     AsyncSmtpTransport::<Tokio1Executor>::starttls_relay("smtp-relay.gmail.com")?
//!         .authentication(vec![Mechanism::Login])
//!         .credentials(smtp_credentials)
//!         .pool_config(pool_config)
//!         .build(),
//!   );
//!
//!   let tmp = temp_dir().join("maillist.txt");
//!   let file = tokio::fs::File::options().create(true).append(true).open(tmp).await?;
//!   let mut reader = tokio::io::BufReader::new(file);
//!
//!   let mut total_lines = 0;
//!   loop {
//!     match reader.read_line(&mut String::new()).await? {
//!       0 => break,
//!       _ => total_lines += 1,
//!     }
//!   }
//!
//!   reader.rewind().await?;
//!   let mut lines = reader.lines();
//!
//!   // <-- Start of Main Example -->
//!   let template: DefaultTemplate = WorkerTemplate::new("Email Sent");
//!   let mut worker_pool: WorkerPool<String, DefaultTemplate> = WorkerPool::new(total_lines, template, 20);
//!
//!   // Spawn all worker and hold the connections
//!   (0..MAX_WORKER).for_each(|_| {
//!     let smtp = smtp_server.clone();
//!     worker_pool.spawn_worker(move |line, progress| send_email(line, progress, smtp.clone()));
//!   });
//!
//!   while let Some(line) = lines.next_line().await? {
//!     _ = worker_pool.send_seqcst(line).await;
//!   }
//!
//!   // Wait for all workers to complete
//!   worker_pool.join_all().await;
//!   // <-- End of Main Example -->
//!
//!   Ok(())
//! }
//!
//! async fn send_email(line: String, progress: MainProgress<DefaultTemplate>, smtp_server: Arc<AsyncSmtpTransport<Tokio1Executor>>) -> Result<()> {
//!   let recipients = line.parse::<Mailbox>()?;
//!   let message = MessageBuilder::new()
//!       .to(recipients)
//!       .from("example@gmail.com".parse()?)
//!       .subject(format!("This is an email sent at #{} progress", progress.length().unwrap()))
//!       .body(String::from("Test mail body"))?;
//!
//!   smtp_server
//!       .send(message)
//!       .await
//!       .map(|_| progress.increment(1))
//!       .map_err(|err| anyhow!("{err}"))
//! }
//! ```

use std::cell::UnsafeCell;
use std::collections::{HashMap, VecDeque};
use std::marker::PhantomData;
use std::process::abort;
use std::time::Duration;

use colored::Colorize;
#[cfg(feature = "futures-util")]
use futures_util::future::join_all;
use indicatif::{MultiProgress, ProgressDrawTarget, ProgressState, ProgressStyle};
use tokio::sync::mpsc::{self, Sender};
use tokio::task::JoinHandle;

use crate::helper::line_err;
use crate::{limit_string, MainProgress, Result, Uid, WorkerTemplate};

type Handle = JoinHandle<()>;
type Handles = HashMap<Uid, Handle>;

/// [`WorkerPool`] manages a pool of worker threads for parallel task execution.
///
/// It allows spawning workers, sending data to them, and managing their lifecycle.
pub struct WorkerPool<D, S>
where
  S: WorkerTemplate,
{
  ui: MultiProgress,
  channels: VecDeque<(Uid, Sender<D>)>,
  handles: Handles,
  main_progress: MainProgress<S>,
  _unsafe_sync: PhantomData<UnsafeCell<()>>,
}

impl<D: Send + 'static, S: WorkerTemplate> WorkerPool<D, S> {
  /// Creates a new `WorkerPool` with a specified number of workers, a worker template, and a draw frequency.
  ///
  /// # Arguments
  ///
  /// * `len` - The number of workers in the pool.
  /// * `template` - The template for creating progress bars for each worker.
  /// * `draw_hz` - The frequency at which the progress bars are drawn (frames per second).
  pub fn new(len: u64, template: S, draw_hz: u8) -> Self {
    let target = ProgressDrawTarget::stderr_with_hz(draw_hz);
    let ui = MultiProgress::with_draw_target(target);
    let main_ui = MainProgress::new(len, ui.clone(), template);
    ui.set_move_cursor(false);

    Self {
      ui,
      channels: Default::default(),
      handles: Default::default(),
      main_progress: main_ui,
      _unsafe_sync: PhantomData,
    }
  }

  /// Generates a new unique task ID.
  pub fn new_task_id(&self) -> Uid {
    Uid::new(self.handles.len() as u32 + 1).unwrap()
  }

  /// Spawns a new worker in the pool.
  ///
  /// # Arguments
  ///
  /// * `f` - A closure that represents the worker's task. It takes data `D` and a `MainProgress` instance as input
  ///   and returns a `Result`.
  pub fn spawn_worker<F, Fut>(&mut self, f: F) -> Uid
  where
    Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
    Fut::Output: Send + 'static,
    F: Fn(D, MainProgress<S>) -> Fut + Send + 'static,
  {
    let task_id = self.new_task_id();
    let (tx, rx) = mpsc::channel(1);
    let handle = match super::WorkerHandleBuilder::default()
      .fn_ptr(f)
      .main_ui(self.main_ui())
      .receiver(rx)
      .build(task_id)
    {
      Ok(handle) => handle,
      Err(build_error) => {
        self.main_progress.println(build_error.to_string());
        abort();
      }
    };

    let handle: Handle = tokio::spawn(handle.run());
    self.channels.push_back((task_id, tx));
    self.handles.insert(task_id, handle);

    task_id
  }

  /// Returns a clone of the main progress instance.
  pub fn main_ui(&self) -> MainProgress<S> {
    self.main_progress.clone()
  }

  /// Handles the SIGINT signal, setting a prefix message and stopping all workers.
  pub async fn sigint(&mut self) {
    let prefix = "<C-c> Received, Waiting all background processes to finished";
    self.main_progress.set_prefix(limit_string(116, prefix.bright_red().to_string(), None));
    self.stop_all_workers().await;
  }

  /// Stops all workers in the pool and waits for them to finish.
  async fn stop_all_workers(&mut self) {
    self.close_all();

    loop {
      if self.handles.iter().filter(|(_, h)| h.is_finished()).count().ge(&self.handles.len()) {
        break;
      }

      tokio::time::sleep(Duration::from_millis(1)).await;
    }

    self
      .main_progress
      .println("All background process is finished!".bright_green().to_string())
  }

  /// Closes all channels in the pool.
  fn close_all(&mut self) {
    while self.channels.pop_back().is_some() {
      // Empty
    }
  }

  /// Sends data to the first available worker in the pool.
  ///
  /// # Arguments
  ///
  /// * `data`: The data to send to the worker.
  ///
  /// Returns: `Result<(), D>`
  ///
  pub async fn send_seqcst(&mut self, mut data: D) -> Result<(), D> {
    let timeout_1ms = Duration::from_millis(1);

    'sending: while !self.channels.is_empty() {
      // Manages closed channel
      self.channels.retain(|(_, tx)| !tx.is_closed());

      let Some(channel) = self.channels.pop_front() else { continue };
      let sent = channel.1.send_timeout(data, timeout_1ms).await;
      self.channels.push_back(channel);

      match sent {
        Ok(_) => return Ok(()),
        Err(error) => {
          data = error.into_inner();
          continue 'sending;
        }
      }
    }

    Err(data)
  }

  /// Sends data directly to the worker by its id.
  ///
  /// # Arguments
  ///
  /// * `id` - The ID of the worker to send data to.
  /// * `data` - The data to send.
  ///
  /// returns: Result<(), D>
  ///
  /// # Panics
  ///
  /// Panics if the pool naver been spawned a worker.
  pub async fn send_to(&mut self, id: Uid, data: D) -> Result<(), D> {
    if self.handles.is_empty() {
      panic!("No worker has ever been spawned!");
    }

    self.channels.retain(|(_, tx)| !tx.is_closed());

    for (worker_id, tx) in &self.channels {
      if worker_id == &id {
        if let Err(err) = tx.send(data).await {
          _ = self.ui.println(err.to_string().bright_red().to_string());
          return Err(err.0);
        }
        return Ok(());
      }
    }

    Err(data)
  }

  /// Returns the number of active threads in the pool.
  pub fn thead_count(&self) -> usize {
    self.handles.len()
  }

  /// Waits for all workers to complete their tasks.
  pub async fn join_all(mut self) {
    self.stop_all_workers().await;

    #[cfg(feature = "futures-util")]
    join_all(self.handles.into_values()).await;
    #[cfg(not(feature = "futures-util"))]
    for handle in self.handles.into_values() {
      _ = handle.await;
    }
  }
}

impl<D: Send, S: WorkerTemplate> WorkerPool<D, S> {
  /// Retrieves a `ProgressStyle` based on the provided template.
  ///
  /// This style includes customized progress characters, date formatting, and status indicators.
  pub fn get_style(template: impl AsRef<str>) -> ProgressStyle {
    type PS = ProgressState;
    ProgressStyle::with_template(template.as_ref())
      .unwrap()
      .progress_chars("──")
      .tick_strings(&["◜", "◠", "◝", "◞", "◡", "◟"]) // Progress Chars
      .with_key("date", |_: &PS, w: &mut dyn std::fmt::Write| {
        _ = write!(w, "[{}]", crate::dt_now_rfc2822())
      })
      .with_key("|", |_: &PS, w: &mut dyn std::fmt::Write| _ = w.write_str("│"))
      .with_key("-", |_: &PS, w: &mut dyn std::fmt::Write| _ = w.write_str("─"))
      .with_key("l", |_: &PS, w: &mut dyn std::fmt::Write| _ = w.write_str("╰"))
      .with_key("status", |ps: &PS, w: &mut dyn std::fmt::Write| {
        _ = write!(
          w,
          "{}",
          if ps.is_finished() {
            "FINISHED".bright_green()
          } else {
            "RUNNING".bright_yellow()
          }
        )
      })
  }

  /// Prints a line to the UI.
  pub fn println(&self, line: impl AsRef<str>) -> Result<(), std::io::Error> {
    self.ui.println(line)
  }

  /// Prints an error line to the UI.
  pub fn eprintln(&self, line: impl AsRef<str>) -> Result<(), std::io::Error> {
    self.ui.println(line_err(line.as_ref()))
  }

  /// Generates a horizontal line of a specified length.
  pub fn horizontal_line(len: usize) -> String {
    "─".repeat(len)
  }

  /// Generates a vertical line of a specified length.
  pub fn vertical_line(len: usize) -> String {
    "│".repeat(len)
  }
}

#[cfg(test)]
mod test {
  use static_assertions::assert_not_impl_any;

  use super::*;
  use crate::DefaultTemplate;

  #[test]
  fn worker_pool_should_not_be_sync() {
    assert_not_impl_any!(WorkerPool<String, DefaultTemplate>: Sync);
  }
}
