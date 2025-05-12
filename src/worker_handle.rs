use colored::Colorize;
use tokio::select;
use tokio::sync::mpsc::Receiver;

use crate::error::Error;
use crate::templates::WorkerTemplate;
use crate::{limit_string, wsupdate, wsupdate_async, MainProgress, Result, Uid, WorkerState, WorkerStatus};

pub struct WorkerHandle<D, F, Fut, S>
where
    Fut: Future<Output=Result<()>> + Send + 'static,
    Fut::Output: Send + 'static,
    F: Fn(D, MainProgress<S>) -> Fut,
{
    fn_ptr: F,
    receiver: Receiver<D>,
    main_ui: MainProgress<S>,
    worker_state: WorkerState<S>,
}

impl<D, F, Fut, S: WorkerTemplate> WorkerHandle<D, F, Fut, S>
where
    D: Send,
    Fut: Future<Output=Result<()>> + Send + 'static,
    Fut::Output: Send + 'static,
    F: Fn(D, MainProgress<S>) -> Fut,
{
    fn initialize(mut self) -> Self {
        self.main_ui.add_worker(self.worker_state.id, &self.worker_state);

        wsupdate! {
      self.worker_state,
      self.receiver,
      "New worker spawned",
      WorkerStatus::Spawned,
    }

        self.worker_state.set_status(WorkerStatus::Waiting);
        self
    }

    pub async fn run(mut self) {
        use WorkerStatus::*;

        while let Some(data) = self.receiver.recv().await {
            self.main_ui.inc(1);

            wsupdate! {
        self.worker_state,
        self.receiver,
        "Processing...",
        Running
      }

            let call = &mut self.fn_ptr;
            select! {
        _ = wsupdate_async!(self.worker_state, self.receiver,) => (),
        result = call(data, self.main_ui.clone()) => self.update_state(result)
      }
        }

        self.worker_state.set_jobs(self.receiver.len());
        self.worker_state.set_status(Stopped);
        self.worker_state.set_task("All finished!");
    }

    fn update_state(&mut self, output: Result<()>) {
        if let Err(error) = output {
            let (task, status) = if self.receiver.is_empty() {
                ("Waiting", WorkerStatus::Waiting)
            } else {
                ("Processing", WorkerStatus::Running)
            };

            wsupdate! {
        &mut self.worker_state,
        self.receiver,
        "Waiting Jobs",
        status
      }

            self.worker_state.set_jobs(self.receiver.len());
            self.worker_state.set_task(task);

            let error = limit_string(116, error, None).bright_red();
            self.main_ui.set_message(error.to_string());
        }
    }
}

pub struct WorkerHandleBuilder<D, F, Fut, S: WorkerTemplate>
where
    D: Send,
    Fut: Future<Output=Result<()>> + Send + 'static,
    Fut::Output: Send + 'static,
    F: Fn(D, MainProgress<S>) -> Fut,
{
    fn_ptr: Option<F>,
    main_ui: Option<MainProgress<S>>,
    receiver: Option<Receiver<D>>,
}

impl<D, F, Fut, S: WorkerTemplate> Default for WorkerHandleBuilder<D, F, Fut, S>
where
    D: Send,
    Fut: Future<Output=Result<()>> + Send + 'static,
    Fut::Output: Send + 'static,
    F: Fn(D, MainProgress<S>) -> Fut,
{
    fn default() -> Self {
        Self {
            fn_ptr: None,
            main_ui: None,
            receiver: None,
        }
    }
}

impl<D, F, Fut, S: WorkerTemplate> WorkerHandleBuilder<D, F, Fut, S>
where
    D: Send,
    Fut: Future<Output=Result<()>> + Send + 'static,
    Fut::Output: Send + 'static,
    F: Fn(D, MainProgress<S>) -> Fut,
{
    pub fn fn_ptr(mut self, f: F) -> Self {
        self.fn_ptr = Some(f);
        self
    }

    pub fn main_ui(mut self, ui: MainProgress<S>) -> Self {
        self.main_ui = Some(ui);
        self
    }

    pub fn receiver(mut self, receiver: Receiver<D>) -> Self {
        self.receiver = Some(receiver);
        self
    }

    pub fn build(self) -> Result<WorkerHandle<D, F, Fut, S>> {
        let fn_ptr = self.fn_ptr.ok_or(Error::Missing("[`WorkerHandleBuilder`] missing [`FnMut`]"))?;
        let receiver = self.receiver.ok_or(Error::Missing("[`WorkerHandleBuilder`] missing [`Receiver<T>`]"))?;
        let main_ui = self.main_ui.ok_or(Error::Missing("[`WorkerHandleBuilder`] missing [`MainProgress`]"))?;
        let worker_state = WorkerState::new(Uid::new(1).unwrap(), main_ui.clone());

        let handle = WorkerHandle {
            fn_ptr,
            receiver,
            main_ui,
            worker_state,
        };

        Ok(handle.initialize())
    }
}

#[cfg(test)]
mod test {
  use std::sync::atomic::AtomicBool;
  use std::sync::Arc;

  use indicatif::MultiProgress;
  use tokio::spawn;

  use super::*;
  use crate::DefaultTemplate;

  #[test]
    fn test_build() {
        let style = DefaultTemplate::new("Test");
        let (_, rx) = tokio::sync::mpsc::channel::<String>(1);
        let main_ui = MainProgress::new(10, MultiProgress::new(), style);

        let handle = WorkerHandleBuilder::default()
            .fn_ptr(async |_: String, _: MainProgress<DefaultTemplate>| Ok(()))
            .main_ui(main_ui)
            .receiver(rx)
            .build();

        assert!(handle.is_ok());

        let handle = handle.unwrap();
        let join_handle = spawn(handle.run());
        join_handle.abort();
    }

    #[test]
    fn test_capture_owned_var() {
        async fn the_solver_fn(_: String, _: MainProgress<DefaultTemplate>, _: Arc<AtomicBool>) -> Result<()> {
            Ok(())
        }

        let (_, rx) = tokio::sync::mpsc::channel::<String>(1);
        let main_ui = MainProgress::new(10, MultiProgress::new(), DefaultTemplate::new("Test"));
        let moveable = Arc::new(AtomicBool::default());

        let ref_4_fn_ptr_owned = moveable.clone();

        let handle = WorkerHandleBuilder::default()
            .fn_ptr(move |msg: String, ui: MainProgress<DefaultTemplate>| the_solver_fn(msg, ui, ref_4_fn_ptr_owned.clone()))
            .main_ui(main_ui)
            .receiver(rx)
            .build();

        assert!(handle.is_ok());

        let handle = handle.unwrap();
        let join_handle = spawn(handle.run());
        join_handle.abort();
    }
}
