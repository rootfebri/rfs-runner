use std::fmt;

use colored::Colorize;

pub fn limit_string(max: usize, input: impl ToString, pad_str: impl Into<Option<&'static str>>) -> String {
  let input = input.to_string();

  let mut input = input.to_string();
  if input.len() < max {
    input.push_str(&pad_str.into().unwrap_or(" ").repeat(max - input.len()));
  }
  input.truncate(max);
  input
}

pub fn line_err(error: impl ToString) -> String {
  format!("[{}] {}", dt_now_rfc2822(), error.to_string().red())
}

pub fn line_ok(info: impl fmt::Display) -> String {
  format!("[{}] {info}", dt_now_rfc2822())
}

pub fn dt_now_rfc2822() -> impl fmt::Display {
  chrono::Local::now().to_rfc2822().dimmed()
}
