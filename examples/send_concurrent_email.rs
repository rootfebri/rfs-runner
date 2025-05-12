use std::sync::Arc;

use anyhow::{anyhow, Result};
use lettre::message::{Mailbox, MessageBuilder};
use lettre::transport::smtp::authentication::{Credentials, Mechanism};
use lettre::transport::smtp::PoolConfig;
use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};
use rfs_runner::{DefaultTemplate, MainProgress, WorkerPool, WorkerTemplate};
use tokio::io::{AsyncBufReadExt, AsyncSeekExt};
const MAX_WORKER: u32 = 8;

#[tokio::main]
async fn main() -> Result<()> {
  use std::env::temp_dir;
  let smtp_credentials = Credentials::new("example@gmail.com".to_owned(), "KeepSecretPasswordFromCommit".to_owned());
  let pool_config = PoolConfig::new().max_size(MAX_WORKER);
  let smtp_server: Arc<AsyncSmtpTransport<Tokio1Executor>> = Arc::new(
    AsyncSmtpTransport::<Tokio1Executor>::starttls_relay("smtp-relay.gmail.com")?
      .authentication(vec![Mechanism::Login])
      .credentials(smtp_credentials)
      .pool_config(pool_config)
      .build(),
  );

  let tmp = temp_dir().join("maillist.txt");
  let file = tokio::fs::File::options().create(true).append(true).open(tmp).await?;
  let mut reader = tokio::io::BufReader::new(file);

  let mut total_lines = 0;
  loop {
    match reader.read_line(&mut String::new()).await? {
      0 => break,
      _ => total_lines += 1,
    }
  }

  reader.rewind().await?;
  let mut lines = reader.lines();

  // <-- Start of Main Example -->
  let template: DefaultTemplate = WorkerTemplate::new("Email Sent");
  let mut worker_pool: WorkerPool<String, DefaultTemplate> = WorkerPool::new(total_lines, template, 20);

  // Spawn all worker and hold the connections
  (0..MAX_WORKER).for_each(|_| {
    let smtp = smtp_server.clone();
    worker_pool.spawn_worker(move |line, progress| send_email(line, progress, smtp.clone()));
  });

  while let Some(line) = lines.next_line().await? {
    _ = worker_pool.send_seqcst(line).await;
  }

  // Wait for all workers to complete
  worker_pool.join_all().await;
  // <-- End of Main Example -->

  Ok(())
}

async fn send_email(line: String, progress: MainProgress<DefaultTemplate>, smtp_server: Arc<AsyncSmtpTransport<Tokio1Executor>>) -> Result<()> {
  let recipients = line.parse::<Mailbox>()?;
  let message = MessageBuilder::new()
    .to(recipients)
    .from("example@gmail.com".parse()?)
    .subject(format!("This is an email sent at #{} progress", progress.length().unwrap()))
    .body(String::from("Test mail body"))?;

  smtp_server
    .send(message)
    .await
    .map(|_| progress.increment(1))
    .map_err(|err| anyhow!("{err}"))
}
