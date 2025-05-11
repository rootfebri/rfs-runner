use std::collections::{HashMap, VecDeque};
use std::process::abort;
use std::time::Duration;
use std::{fmt, io};

use colored::Colorize;
#[cfg(feature = "futures-util")]
use futures_util::future::join_all;
use indicatif::{MultiProgress, ProgressDrawTarget, ProgressState, ProgressStyle};
use tokio::sync::mpsc::error::SendTimeoutError;
use tokio::sync::mpsc::{self, Sender};
use tokio::task::JoinHandle;

use crate::helper::line_err;
use crate::{limit_string, MainProgress, Result, Uid, WorkerTemplate};

type Handle = JoinHandle<()>;
type Handles = HashMap<Uid, Handle>;

pub struct WorkerPool<D, S>
where
  S: WorkerTemplate,
{
  ui: MultiProgress,
  channels: VecDeque<(Uid, Sender<D>)>,
  handles: Handles,
  main_progress: MainProgress<S>,
}

impl<D: Send + 'static, S: WorkerTemplate> WorkerPool<D, S> {
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
    }
  }

  pub fn new_task_id(&self) -> Uid {
    Uid::new(self.handles.len() as u32 + 1).unwrap()
  }

  pub fn spawn_worker<F, Fut>(&mut self, f: F) -> Uid
  where
    Fut: Future<Output = Result<()>> + Send + 'static,
    F: Fn(D, MainProgress<S>) -> Fut + Send + 'static,
  {
    let task_id = self.new_task_id();
    let (tx, rx) = mpsc::channel(1);
    let handle = match super::WorkerHandleBuilder::default()
      .fn_ptr(f)
      .main_ui(self.main_ui())
      .receiver(rx)
      .build()
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

  pub fn main_ui(&self) -> MainProgress<S> {
    self.main_progress.clone()
  }

  pub async fn sigint(&mut self) {
    let prefix = "<C-c> Received, Waiting all background processes to finished";
    self.main_progress.set_prefix(limit_string(116, prefix.bright_red().to_string(), None));
    self.stop_all_workers().await;
  }

  pub async fn stop_all_workers(&mut self) {
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

  pub fn close_all(&mut self) {
    while self.channels.pop_front().is_some() {
      // Empty
    }
  }

  pub async fn send_seqcst(&mut self, mut data: D) -> Result<()> {
    'entry: while !self.channels.is_empty() {
      let Some(channel) = self.channels.pop_front() else { continue };

      match channel.1.send_timeout(data, Duration::from_millis(1)).await {
        Ok(_) => {
          self.channels.push_back(channel);
          return Ok(());
        }
        Err(error) => match error {
          SendTimeoutError::Timeout(d) => {
            self.channels.push_back(channel);
            data = d;
            continue 'entry;
          }
          SendTimeoutError::Closed(d) => {
            data = d;
            continue 'entry;
          }
        },
      }
    }

    Err(crate::Error::NoChannel)
  }

  pub async fn send_to(&mut self, id: Uid, data: D) -> bool {
    if self.handles.is_empty() {
      panic!("No worker have ever been spawned!");
    }
    let mut temp_ch = VecDeque::new();

    while let Some((_id, ch)) = self.channels.pop_front() {
      if _id == id {
        if let Err(err) = ch.send(data).await {
          _ = self.ui.println(err.to_string().bright_red().to_string());
          return false;
        }
        return true;
      }

      temp_ch.push_back((_id, ch));
    }

    temp_ch.into_iter().for_each(|x| self.channels.push_back(x));
    false
  }

  pub fn thead_count(&self) -> usize {
    self.handles.len()
  }

  pub async fn join_all(mut self) {
    self.close_all();

    #[cfg(feature = "futures-util")]
    join_all(self.handles.into_values()).await;
    #[cfg(not(feature = "futures-util"))]
    for handle in self.handles.into_values() {
      _ = handle.await;
    }
  }
}

impl<D: Send + 'static, S: WorkerTemplate> WorkerPool<D, S> {
  pub fn get_style(template: impl AsRef<str>) -> ProgressStyle {
    type PS = ProgressState;
    ProgressStyle::with_template(template.as_ref())
      .unwrap()
      .progress_chars("──")
      .tick_strings(&["◜", "◠", "◝", "◞", "◡", "◟"]) // Progress Chars
      .with_key("date", |_: &PS, w: &mut dyn fmt::Write| _ = write!(w, "[{}]", crate::dt_now_rfc2822()))
      .with_key("|", |_: &PS, w: &mut dyn fmt::Write| _ = w.write_str("│"))
      .with_key("-", |_: &PS, w: &mut dyn fmt::Write| _ = w.write_str("─"))
      .with_key("l", |_: &PS, w: &mut dyn fmt::Write| _ = w.write_str("╰"))
      .with_key("status", |ps: &PS, w: &mut dyn fmt::Write| {
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

  pub fn println(&self, line: impl AsRef<str>) -> Result<(), io::Error> {
    self.ui.println(line)
  }

  pub fn eprintln(&self, line: impl AsRef<str>) -> Result<(), io::Error> {
    self.ui.println(line_err(line.as_ref()))
  }

  pub fn horizontal_line(len: usize) -> String {
    "─".repeat(len)
  }

  pub fn vertical_line(len: usize) -> String {
    "│".repeat(len)
  }
}
