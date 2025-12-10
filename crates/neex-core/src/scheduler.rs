//! Parallel Task Scheduler - Dependency-aware parallel execution
//!
//! Features:
//! - Runs tasks in parallel when dependencies allow
//! - Semaphore for concurrency control
//! - Fail-fast: stops on first error
//! - Respects dependency graph from Phase 5

use anyhow::Result;
#[cfg(test)]
use anyhow::anyhow;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex, Semaphore};
use tokio::task::JoinHandle;

/// Task status
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Task result
#[derive(Debug)]
pub struct TaskResult {
    pub name: String,
    pub status: TaskStatus,
    pub duration: Duration,
    pub error: Option<String>,
}

/// A schedulable task
pub struct SchedulerTask {
    pub name: String,
    pub dependencies: Vec<String>,
    pub action: Box<dyn FnOnce() -> Result<()> + Send + 'static>,
}

impl SchedulerTask {
    pub fn new<F>(name: impl Into<String>, deps: Vec<String>, action: F) -> Self
    where
        F: FnOnce() -> Result<()> + Send + 'static,
    {
        Self {
            name: name.into(),
            dependencies: deps,
            action: Box::new(action),
        }
    }
}

/// Parallel task scheduler
pub struct Scheduler {
    concurrency: usize,
    fail_fast: bool,
}

impl Scheduler {
    /// Create new scheduler with concurrency limit
    pub fn new(concurrency: usize) -> Self {
        Self {
            concurrency,
            fail_fast: true,
        }
    }

    /// Create scheduler with default concurrency (CPU cores)
    pub fn with_default_concurrency() -> Self {
        let cpus = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(4);
        Self::new(cpus)
    }

    /// Set fail-fast behavior
    pub fn fail_fast(mut self, enabled: bool) -> Self {
        self.fail_fast = enabled;
        self
    }

    /// Execute tasks respecting dependencies
    pub async fn execute(&self, tasks: Vec<SchedulerTask>) -> Result<Vec<TaskResult>> {
        if tasks.is_empty() {
            return Ok(vec![]);
        }

        let start = Instant::now();
        let semaphore = Arc::new(Semaphore::new(self.concurrency));
        let (tx, mut rx) = mpsc::channel::<TaskResult>(tasks.len());

        // Task state tracking
        let task_count = tasks.len();
        let completed: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
        let failed: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));

        // Build dependency map and task map
        let mut dep_map: HashMap<String, Vec<String>> = HashMap::new();
        let mut pending_tasks: HashMap<String, SchedulerTask> = HashMap::new();

        for task in tasks {
            dep_map.insert(task.name.clone(), task.dependencies.clone());
            pending_tasks.insert(task.name.clone(), task);
        }

        // Find tasks with no dependencies (can start immediately)
        let ready: Vec<String> = dep_map
            .iter()
            .filter(|(_, deps)| deps.is_empty())
            .map(|(name, _)| name.clone())
            .collect();

        // Spawn initial ready tasks
        let mut handles: Vec<JoinHandle<()>> = Vec::new();
        let pending_tasks = Arc::new(Mutex::new(pending_tasks));

        for task_name in ready {
            let handle = spawn_task(
                task_name,
                Arc::clone(&pending_tasks),
                Arc::clone(&semaphore),
                tx.clone(),
                Arc::clone(&completed),
                Arc::clone(&failed),
                self.fail_fast,
            );
            handles.push(handle);
        }

        // Collect results and spawn dependent tasks
        let mut results = Vec::new();
        let mut received = 0;
        let dep_map = Arc::new(dep_map);

        while received < task_count {
            if let Some(result) = rx.recv().await {
                received += 1;

                let _task_name = result.name.clone();
                let task_succeeded = result.status == TaskStatus::Completed;

                if result.status == TaskStatus::Failed && self.fail_fast {
                    *failed.lock().await = true;
                }

                results.push(result);

                // If task succeeded, find dependent tasks that are now ready
                if task_succeeded {
                    let completed_guard = completed.lock().await;

                    // Find tasks whose dependencies are now all satisfied
                    let ready_tasks: Vec<String> = {
                        let pending = pending_tasks.lock().await;
                        pending
                            .keys()
                            .filter(|name| {
                                if let Some(deps) = dep_map.get(*name) {
                                    deps.iter().all(|d| completed_guard.contains(d))
                                } else {
                                    false
                                }
                            })
                            .cloned()
                            .collect()
                    };
                    drop(completed_guard);

                    for task_name in ready_tasks {
                        let handle = spawn_task(
                            task_name,
                            Arc::clone(&pending_tasks),
                            Arc::clone(&semaphore),
                            tx.clone(),
                            Arc::clone(&completed),
                            Arc::clone(&failed),
                            self.fail_fast,
                        );
                        handles.push(handle);
                    }
                }

                // Break early if failed and fail_fast
                if *failed.lock().await && self.fail_fast {
                    // Cancel remaining by not collecting more
                    break;
                }
            }
        }

        // Wait for all spawned tasks
        for handle in handles {
            let _ = handle.await;
        }

        let total_duration = start.elapsed();
        tracing::info!(
            "Scheduler completed {} tasks in {:?}",
            results.len(),
            total_duration
        );

        Ok(results)
    }
}

/// Spawn a single task
fn spawn_task(
    task_name: String,
    pending_tasks: Arc<Mutex<HashMap<String, SchedulerTask>>>,
    semaphore: Arc<Semaphore>,
    tx: mpsc::Sender<TaskResult>,
    completed: Arc<Mutex<HashSet<String>>>,
    failed: Arc<Mutex<bool>>,
    fail_fast: bool,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        // Check if we should cancel
        if fail_fast && *failed.lock().await {
            let _ = tx
                .send(TaskResult {
                    name: task_name,
                    status: TaskStatus::Cancelled,
                    duration: Duration::ZERO,
                    error: Some("Cancelled due to earlier failure".into()),
                })
                .await;
            return;
        }

        // Acquire semaphore permit
        let _permit = semaphore.acquire().await.unwrap();

        // Take task from pending
        let task = {
            let mut pending = pending_tasks.lock().await;
            pending.remove(&task_name)
        };

        let Some(task) = task else {
            return;
        };

        let start = Instant::now();

        // Execute task
        let result = tokio::task::spawn_blocking(move || (task.action)()).await;

        let duration = start.elapsed();

        let (status, error) = match result {
            Ok(Ok(())) => (TaskStatus::Completed, None),
            Ok(Err(e)) => (TaskStatus::Failed, Some(e.to_string())),
            Err(e) => (TaskStatus::Failed, Some(format!("Task panicked: {}", e))),
        };

        // Mark as completed
        if status == TaskStatus::Completed {
            completed.lock().await.insert(task_name.clone());
        }

        let _ = tx
            .send(TaskResult {
                name: task_name,
                status,
                duration,
                error,
            })
            .await;
    })
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::with_default_concurrency()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn test_parallel_execution() {
        // Test: A -> (B, C) should take ~200ms, not 300ms
        // A: 100ms, then B and C run in parallel: 100ms each

        let execution_order = Arc::new(Mutex::new(Vec::new()));
        let order_clone1 = Arc::clone(&execution_order);
        let order_clone2 = Arc::clone(&execution_order);
        let order_clone3 = Arc::clone(&execution_order);

        let tasks = vec![
            SchedulerTask::new("A", vec![], move || {
                std::thread::sleep(Duration::from_millis(100));
                order_clone1.blocking_lock().push("A");
                Ok(())
            }),
            SchedulerTask::new("B", vec!["A".into()], move || {
                std::thread::sleep(Duration::from_millis(100));
                order_clone2.blocking_lock().push("B");
                Ok(())
            }),
            SchedulerTask::new("C", vec!["A".into()], move || {
                std::thread::sleep(Duration::from_millis(100));
                order_clone3.blocking_lock().push("C");
                Ok(())
            }),
        ];

        let scheduler = Scheduler::new(4);
        let start = Instant::now();
        let results = scheduler.execute(tasks).await.unwrap();
        let duration = start.elapsed();

        // Check all tasks completed
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| r.status == TaskStatus::Completed));

        // Check A ran first
        let order = execution_order.lock().await;
        assert_eq!(order[0], "A");

        // Check total time is ~200ms (not 300ms)
        // Allow some margin for task overhead
        assert!(
            duration < Duration::from_millis(250),
            "Expected ~200ms, got {:?}. B and C should run in parallel!",
            duration
        );
    }

    #[tokio::test]
    async fn test_fail_fast() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone1 = Arc::clone(&counter);
        let counter_clone2 = Arc::clone(&counter);
        let counter_clone3 = Arc::clone(&counter);

        let tasks = vec![
            SchedulerTask::new("A", vec![], move || {
                counter_clone1.fetch_add(1, Ordering::SeqCst);
                Err(anyhow!("Task A failed!"))
            }),
            SchedulerTask::new("B", vec!["A".into()], move || {
                counter_clone2.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }),
            SchedulerTask::new("C", vec!["A".into()], move || {
                counter_clone3.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }),
        ];

        let scheduler = Scheduler::new(4).fail_fast(true);
        let results = scheduler.execute(tasks).await.unwrap();

        // Only A should have executed
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        // A should be failed
        let a_result = results.iter().find(|r| r.name == "A").unwrap();
        assert_eq!(a_result.status, TaskStatus::Failed);
    }

    #[tokio::test]
    async fn test_concurrency_limit() {
        // Test that semaphore limits concurrent tasks
        let max_concurrent = Arc::new(AtomicUsize::new(0));
        let current_concurrent = Arc::new(AtomicUsize::new(0));

        let tasks: Vec<_> = (0..10)
            .map(|i| {
                let max = Arc::clone(&max_concurrent);
                let current = Arc::clone(&current_concurrent);

                SchedulerTask::new(format!("Task{}", i), vec![], move || {
                    let prev = current.fetch_add(1, Ordering::SeqCst);
                    let now = prev + 1;

                    // Update max if current is higher
                    max.fetch_max(now, Ordering::SeqCst);

                    std::thread::sleep(Duration::from_millis(50));
                    current.fetch_sub(1, Ordering::SeqCst);
                    Ok(())
                })
            })
            .collect();

        let scheduler = Scheduler::new(3); // Limit to 3
        scheduler.execute(tasks).await.unwrap();

        // Max concurrent should not exceed 3
        assert!(
            max_concurrent.load(Ordering::SeqCst) <= 3,
            "Concurrency limit exceeded!"
        );
    }
}
