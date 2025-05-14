use std::fmt;

use colored::Colorize;

pub fn limit_string(max: usize, input: impl ToString, pad: impl Into<Option<char>>) -> String {
  let input = input.to_string();
  let pad = pad.into().unwrap_or(' ');

  let truncated: String = input.chars().take(max).collect();
  let pad_len = max.saturating_sub(truncated.chars().count());

  truncated + &pad.to_string().repeat(pad_len)
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
