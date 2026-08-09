//! Gerenciamento de jobs de render e geração do ARCZ Earth.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum JobStatus {
    Pending,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobTask {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub progress: u8,
    pub status: JobStatus,
    pub error_msg: Option<String>,
    pub heartbeat_utc: String,
}

impl JobTask {
    pub fn new(id: impl Into<String>, name: impl Into<String>, kind: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind: kind.into(),
            progress: 0,
            status: JobStatus::Pending,
            error_msg: None,
            heartbeat_utc: "2026-07-30T15:26:00Z".to_string(),
        }
    }
}

#[derive(Debug, Default)]
pub struct JobQueue {
    pub jobs: HashMap<String, JobTask>,
}

impl JobQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enqueue(&mut self, job: JobTask) {
        self.jobs.insert(job.id.clone(), job);
    }

    pub fn update_progress(&mut self, id: &str, progress: u8) -> bool {
        if let Some(j) = self.jobs.get_mut(id) {
            j.progress = progress.min(100);
            if j.progress == 100 {
                j.status = JobStatus::Completed;
            } else {
                j.status = JobStatus::Running;
            }
            true
        } else {
            false
        }
    }

    pub fn cancel(&mut self, id: &str) -> bool {
        if let Some(j) = self.jobs.get_mut(id) {
            j.status = JobStatus::Cancelled;
            true
        } else {
            false
        }
    }

    pub fn recover_orphans(&mut self) -> usize {
        let mut recovered = 0;
        for j in self.jobs.values_mut() {
            if j.status == JobStatus::Running {
                j.status = JobStatus::Pending;
                recovered += 1;
            }
        }
        recovered
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_queue_enqueue_and_progress() {
        let mut queue = JobQueue::new();
        let job = JobTask::new("job-1", "Render Take 01", "render");
        queue.enqueue(job);

        assert!(queue.update_progress("job-1", 50));
        assert_eq!(queue.jobs.get("job-1").unwrap().status, JobStatus::Running);

        queue.update_progress("job-1", 100);
        assert_eq!(
            queue.jobs.get("job-1").unwrap().status,
            JobStatus::Completed
        );
    }

    #[test]
    fn test_recover_orphans_after_crash() {
        let mut queue = JobQueue::new();
        let mut job = JobTask::new("job-2", "Reconstrução 3D", "reconstruct");
        job.status = JobStatus::Running;
        queue.enqueue(job);

        assert_eq!(queue.recover_orphans(), 1);
        assert_eq!(queue.jobs.get("job-2").unwrap().status, JobStatus::Pending);
    }
}
