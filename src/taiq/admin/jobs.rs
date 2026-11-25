use anyhow::Result;
use serde_json::Value;
use std::future::Future;
use std::sync::mpsc;

pub enum JobUpdate {
    Completed(Value),
    Progress(Value),
    Failed(String),
}

pub struct JobRunner {
    runtime: tokio::runtime::Runtime,
    sender: mpsc::Sender<JobUpdate>,
    receiver: mpsc::Receiver<JobUpdate>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl JobRunner {
    pub fn new() -> Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        let (sender, receiver) = mpsc::channel();
        Ok(Self {
            runtime,
            sender,
            receiver,
            task: None,
        })
    }

    pub fn submit<F>(&mut self, future: F)
    where
        F: Future<Output = Result<Value>> + Send + 'static,
    {
        self.cancel();
        let sender = self.sender.clone();
        self.task = Some(self.runtime.spawn(async move {
            let update = match future.await {
                Ok(value) => JobUpdate::Completed(value),
                Err(error) => JobUpdate::Failed(format!("{error:#}")),
            };
            let _ = sender.send(update);
        }));
    }

    pub fn submit_stream<F, Fut>(&mut self, work: F)
    where
        F: FnOnce(mpsc::Sender<JobUpdate>) -> Fut + Send + 'static,
        Fut: Future<Output = Result<Value>> + Send + 'static,
    {
        self.cancel();
        let sender = self.sender.clone();
        self.task = Some(self.runtime.spawn(async move {
            let update = match work(sender.clone()).await {
                Ok(value) => JobUpdate::Completed(value),
                Err(error) => JobUpdate::Failed(format!("{error:#}")),
            };
            let _ = sender.send(update);
        }));
    }

    pub fn cancel(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
        while self.receiver.try_recv().is_ok() {}
    }

    pub fn try_recv(&self) -> Option<JobUpdate> {
        self.receiver.try_recv().ok()
    }
}
