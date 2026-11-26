//! Blocking image/OCR/file work has a bounded queue and fixed concurrency.
//! Send closures cannot capture GTK objects. Completion is awaited on the GLib
//! main context; submitting a job never blocks that context on a full queue.
use async_channel::{Receiver, Sender};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Debug)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    pub fn cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn check(&self) -> Result<(), TaskError> {
        if self.cancelled() {
            Err(TaskError::Cancelled)
        } else {
            Ok(())
        }
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum TaskError {
    Busy,
    Cancelled,
    Failed(String),
    Stopped,
}
impl std::fmt::Display for TaskError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use clivuu_core::i18n::t;
        match self {
            Self::Failed(message) => f.write_str(message),
            Self::Busy => f.write_str(&t("A capture or import session is already in progress")),
            Self::Cancelled => f.write_str(&t("Operation cancelled")),
            Self::Stopped => f.write_str(&t("The desktop service is stopped")),
        }
    }
}
impl std::error::Error for TaskError {}
pub struct Task<T> {
    result: Receiver<Result<T, TaskError>>,
    cancel: Cancellation,
}
impl<T> Task<T> {
    pub fn cancellation(&self) -> Cancellation {
        self.cancel.clone()
    }
    pub async fn result(self) -> Result<T, TaskError> {
        self.result.recv().await.unwrap_or(Err(TaskError::Stopped))
    }
}
impl<T> Drop for Task<T> {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
type Job = Box<dyn FnOnce() + Send + 'static>;
pub struct TaskPool {
    queue: Sender<Job>,
}
impl TaskPool {
    pub fn new(workers: usize, waiting: usize) -> std::io::Result<Self> {
        assert!(workers > 0 && waiting > 0);
        let (tx, rx) = async_channel::bounded::<Job>(waiting);
        for n in 0..workers {
            let rx = rx.clone();
            std::thread::Builder::new()
                .name(format!("clivuu-work-{n}"))
                .spawn(move || {
                    while let Ok(job) = rx.recv_blocking() {
                        job();
                    }
                })?;
        }
        Ok(Self { queue: tx })
    }
    pub fn submit<T: Send + 'static>(
        &self,
        work: impl FnOnce(Cancellation) -> Result<T, TaskError> + Send + 'static,
    ) -> Result<Task<T>, TaskError> {
        let (tx, result) = async_channel::bounded(1);
        let cancel = Cancellation(Arc::new(AtomicBool::new(false)));
        let token = cancel.clone();
        let job = Box::new(move || {
            let value = if token.cancelled() {
                Err(TaskError::Cancelled)
            } else {
                let value = work(token.clone());
                if token.cancelled() {
                    Err(TaskError::Cancelled)
                } else {
                    value
                }
            };
            // A dropped UI/session receiver must never stall a worker.
            let _ = tx.try_send(value);
        });
        self.queue.try_send(job).map_err(|e| {
            if e.is_full() {
                TaskError::Busy
            } else {
                TaskError::Stopped
            }
        })?;
        Ok(Task { result, cancel })
    }
    pub fn close(&self) {
        self.queue.close();
    }
}
impl Drop for TaskPool {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn jobs_are_bounded_and_waiting_cancellation_does_not_run_the_job() {
        let pool = TaskPool::new(1, 1).unwrap();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (gate_tx, gate_rx) = std::sync::mpsc::channel();
        let first = pool
            .submit(move |_| {
                started_tx.send(()).unwrap();
                gate_rx.recv().unwrap();
                Ok(7)
            })
            .unwrap();
        started_rx.recv().unwrap();
        let ran = Arc::new(AtomicBool::new(false));
        let ran_job = ran.clone();
        let second = pool
            .submit(move |_| {
                ran_job.store(true, Ordering::Release);
                Ok(8)
            })
            .unwrap();
        assert!(matches!(pool.submit(|_| Ok(9)), Err(TaskError::Busy)));
        second.cancellation().cancel();
        gate_tx.send(()).unwrap();
        assert_eq!(
            gtk::glib::MainContext::new().block_on(first.result()),
            Ok(7)
        );
        assert_eq!(
            gtk::glib::MainContext::new().block_on(second.result()),
            Err(TaskError::Cancelled)
        );
        // Completion above proves the queued job was processed and skipped.
        pool.close();
        assert!(!ran.load(Ordering::Acquire));
    }
    #[test]
    fn a_closed_pool_rejects_new_jobs_without_blocking() {
        let pool = TaskPool::new(1, 1).unwrap();
        pool.close();
        assert!(matches!(pool.submit(|_| Ok(())), Err(TaskError::Stopped)));
    }
}
