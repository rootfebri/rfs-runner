use std::collections::HashMap;
use std::fmt::{Debug, Display, Formatter, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};

use colored::Colorize;
use indicatif::{HumanCount, HumanDuration, ProgressState, ProgressStyle};

use crate::Uid;

pub trait WorkerTemplate: Send + Sync + Clone + AsProgressStyle + 'static {
  fn new(label: impl ToString) -> Self;
  fn increment(&self, count: usize);
  fn add_worker(&self, id: Uid, body: String);
  fn remove_worker(&self, id: &Uid);
  fn edit_worker(&self, id: &Uid, body: String);
}

pub trait AsProgressStyle {
  fn as_progress_style(&self) -> ProgressStyle;
}

#[derive(Debug)]
pub struct DefaultTemplate
where
  Self: Send + Sync,
{
  tbody: Arc<RwLock<HashMap<Uid, String>>>,
  footer: Arc<str>,
  main_key: Arc<str>,
  main_count: Arc<AtomicUsize>,
}

impl DefaultTemplate {
  const TABLE_WIDTH: usize = 120;

  fn main_count(&self) -> usize {
    self.main_count.load(Ordering::SeqCst)
  }

  fn header(&self) -> String {
    let main_count = self.main_count().to_string();
    let pad_left = Self::TABLE_WIDTH - self.main_key.len() - (main_count.len() + 2) - 20 - 20 - 4;
    format!(
      "╭{main_key}: {main_count} {top_fill}{{pos_pad_left}}/{{len_pad_right}}╮\n",
      main_key = self.main_key.bright_blue(),
      main_count = main_count.bright_green(),
      top_fill = "─".repeat(pad_left)
    )
  }

  fn body(&self) -> String {
    self
      .tbody
      .read()
      .unwrap()
      .clone()
      .into_values()
      .map(|value| format!("│ {value:>114} │\n"))
      .collect()
  }
}

impl WorkerTemplate for DefaultTemplate {
  fn new(label: impl ToString) -> Self {
    let footer = "\
    │ {prefix:!116} │\n\
    │ {msg:!116} │\n\
    │ Elapsed: {elapsed_precise:.blue} │ Est. Time remaining: {human_eta:!30} │ P/Sec: {per_sec:!35} │\n\
    ╰{bar:!118.green/.cyan.dim}╯\n\
    ";
    Self {
      tbody: Default::default(),
      footer: Arc::from(footer),
      main_key: Arc::from(label.to_string()),
      main_count: Default::default(),
    }
  }

  fn increment(&self, count: usize) {
    self.main_count.fetch_add(count, Ordering::AcqRel);
  }

  fn add_worker(&self, id: Uid, body: String) {
    self.tbody.write().unwrap().insert(id, body);
  }

  fn remove_worker(&self, id: &Uid) {
    if self.tbody.read().unwrap().contains_key(id) {
      self.tbody.write().unwrap().remove(id);
    }
  }

  fn edit_worker(&self, id: &Uid, body: String) {
    if !self.tbody.read().unwrap().contains_key(id) {
      return;
    }

    if let Some(inner) = self.tbody.write().unwrap().get_mut(id) {
      *inner = body
    }
  }
}

impl AsProgressStyle for DefaultTemplate {
  fn as_progress_style(&self) -> ProgressStyle {
    self.into()
  }
}

impl Display for DefaultTemplate {
  fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}{}{}", self.header(), self.body(), self.footer)
  }
}

impl Clone for DefaultTemplate {
  fn clone(&self) -> Self {
    Self {
      tbody: Arc::clone(&self.tbody),
      footer: Arc::clone(&self.footer),
      main_key: Arc::clone(&self.main_key),
      main_count: Arc::clone(&self.main_count),
    }
  }
}

impl From<&DefaultTemplate> for ProgressStyle {
  fn from(value: &DefaultTemplate) -> ProgressStyle {
    ProgressStyle::with_template(value.to_string().as_str())
      .unwrap()
      .progress_chars("──")
      .tick_strings(&["◜", "◠", "◝", "◞", "◡", "◟"])
      .with_key("human_eta", |s: &ProgressState, w: &mut dyn Write| match s.eta().as_secs() {
        0..=600 => write!(w, "{}", HumanDuration(s.eta()).to_string().bright_green()).unwrap(),
        601..=6000 => write!(w, "{}", HumanDuration(s.eta()).to_string().bright_yellow()).unwrap(),
        _ => write!(w, "{}", HumanDuration(s.eta()).to_string().bright_red()).unwrap(),
      })
      .with_key("len_pad_left", |s: &ProgressState, w: &mut dyn Write| {
        let Some(len) = s.len().map(HumanCount) else { return };
        let len = len.to_string();
        let pad = "─".repeat(19 - len.len());
        write!(w, "{pad} {}", len.bright_cyan()).unwrap();
      })
      .with_key("len_pad_right", |s: &ProgressState, w: &mut dyn Write| {
        let Some(len) = s.len().map(HumanCount) else { return };
        let len = len.to_string();
        let pad = "─".repeat(19 - len.len());
        write!(w, "{} {pad}", len.bright_cyan()).unwrap();
      })
      .with_key("pos_pad_left", |s: &ProgressState, w: &mut dyn Write| {
        let len = HumanCount(s.pos()).to_string();
        let pad = "─".repeat(19 - len.len());
        write!(w, "{pad} {}", len.bright_green().bold()).unwrap();
      })
      .with_key("pos_pad_right", |s: &ProgressState, w: &mut dyn Write| {
        let len = HumanCount(s.pos()).to_string();
        let pad = "─".repeat(19 - len.len());
        write!(w, "{} {pad}", len.bright_green().bold()).unwrap();
      })
  }
}
