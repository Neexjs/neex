//! Process runner - executes one task command and captures its output
//!
//! - stdout and stderr are read concurrently (a child that fills the stderr
//!   pipe can no longer deadlock the run)
//! - non-UTF-8 output is captured lossily instead of aborting
//! - the whole process group/job is killed if the future is dropped

use crate::artifacts::{LogLine, Stream};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct ExecResult {
    pub exit_code: i32,
    pub duration_ms: u64,
    pub logs: Vec<LogLine>,
}

impl ExecResult {
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }
}

/// Callback invoked for every output line as it arrives
pub type LineSink = Arc<dyn Fn(&LogLine) + Send + Sync>;

/// Run `command` through the platform shell in `cwd`
pub async fn execute(
    command: &str,
    cwd: &Path,
    env: &HashMap<String, String>,
    sink: Option<LineSink>,
) -> Result<ExecResult> {
    let start = Instant::now();

    #[cfg(unix)]
    let mut cmd = {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(command).process_group(0);
        cmd
    };
    #[cfg(windows)]
    let mut cmd = {
        let mut cmd = Command::new("cmd");
        // Block before running user commands until the shell belongs to a
        // kill-on-close job. Descendants can never escape the assignment race.
        cmd.arg("/D")
            .arg("/S")
            .arg("/C")
            // cmd.exe uses its own quoting grammar, not the CRT argv rules.
            // /S strips this outer pair while preserving quoted executable paths.
            .raw_arg(format!("\"set /p \"__NEEX_GATE=\" >nul & {}\"", command));
        cmd
    };
    cmd.current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    cmd.stdin(Stdio::null());
    #[cfg(windows)]
    cmd.stdin(Stdio::piped());
    for (k, v) in env {
        cmd.env(k, v);
    }
    #[cfg(windows)]
    let tree = process_tree::ProcessTree::new()?;
    let mut child = cmd.spawn()?;
    #[cfg(unix)]
    let tree = process_tree::ProcessTree::new(child.id().context("missing child pid")?);
    #[cfg(windows)]
    {
        use tokio::io::AsyncWriteExt;
        tree.assign(child.raw_handle().context("missing child process handle")?)?;
        let mut gate = child.stdin.take().context("missing task startup pipe")?;
        gate.write_all(b"\r\n").await?;
        gate.shutdown().await?;
    }

    let logs: Arc<Mutex<Vec<LogLine>>> = Arc::new(Mutex::new(Vec::new()));

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let out_task = spawn_reader(stdout, Stream::Stdout, Arc::clone(&logs), sink.clone());
    let err_task = spawn_reader(stderr, Stream::Stderr, Arc::clone(&logs), sink);

    let status = child.wait().await?;
    // Background descendants can keep pipes open after the shell exits.
    drop(tree);
    let _ = tokio::join!(out_task, err_task);

    let logs = std::mem::take(&mut *logs.lock().unwrap());
    Ok(ExecResult {
        exit_code: status.code().unwrap_or(-1),
        duration_ms: start.elapsed().as_millis() as u64,
        logs,
    })
}

#[cfg(unix)]
mod process_tree {
    pub struct ProcessTree(libc::pid_t);

    impl ProcessTree {
        pub fn new(pid: u32) -> Self {
            Self(pid as libc::pid_t)
        }
    }

    impl Drop for ProcessTree {
        fn drop(&mut self) {
            // The shell created its own process group before exec. A negative
            // pid addresses only that group, including nested package managers.
            unsafe {
                libc::kill(-self.0, libc::SIGKILL);
            }
        }
    }
}

#[cfg(windows)]
mod process_tree {
    use anyhow::{Context, Result};
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    pub struct ProcessTree(OwnedHandle);

    impl ProcessTree {
        pub fn new() -> Result<Self> {
            let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if job.is_null() {
                return Err(std::io::Error::last_os_error()).context("creating task job");
            }
            let job = unsafe { OwnedHandle::from_raw_handle(job) };
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = unsafe {
                SetInformationJobObject(
                    job.as_raw_handle(),
                    JobObjectExtendedLimitInformation,
                    &limits as *const _ as *const _,
                    std::mem::size_of_val(&limits) as u32,
                )
            };
            if ok == 0 {
                return Err(std::io::Error::last_os_error()).context("configuring task job");
            }
            Ok(Self(job))
        }

        pub fn assign(&self, process: RawHandle) -> Result<()> {
            if unsafe { AssignProcessToJobObject(self.0.as_raw_handle(), process) } == 0 {
                return Err(std::io::Error::last_os_error()).context("assigning task to job");
            }
            Ok(())
        }
    }
}

fn spawn_reader<R>(
    reader: Option<R>,
    stream: Stream,
    logs: Arc<Mutex<Vec<LogLine>>>,
    sink: Option<LineSink>,
) -> tokio::task::JoinHandle<()>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let Some(reader) = reader else {
            return;
        };
        let mut reader = BufReader::new(reader);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let mut text = String::from_utf8_lossy(&buf).to_string();
                    while text.ends_with('\n') || text.ends_with('\r') {
                        text.pop();
                    }
                    let line = LogLine {
                        stream: stream.clone(),
                        text,
                    };
                    if let Some(s) = &sink {
                        s(&line);
                    }
                    logs.lock().unwrap().push(line);
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Re-execute this test binary to create a real child + grandchild on
    // every OS, without requiring Node, bash or an external test fixture.
    #[test]
    fn process_fixture() {
        let Ok(path) = std::env::var("NEEX_FIXTURE_HEARTBEAT") else {
            return;
        };
        if std::env::var("NEEX_FIXTURE_LEAF").is_err() {
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "runner::tests::process_fixture", "--nocapture"])
                .env("NEEX_FIXTURE_LEAF", "1")
                .spawn()
                .unwrap();
            let _ = child.wait();
        } else {
            use std::io::Write;
            for _ in 0..200 {
                if let Ok(mut file) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                {
                    let _ = file.write_all(b"beat\n");
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }

    #[tokio::test]
    async fn cancellation_kills_child_and_grandchild() {
        let tmp = tempfile::tempdir().unwrap();
        let heartbeat = tmp.path().join("heartbeat");
        let mut env = HashMap::new();
        env.insert(
            "NEEX_FIXTURE_HEARTBEAT".into(),
            heartbeat.to_string_lossy().into_owned(),
        );
        let binary = std::env::current_exe().unwrap();
        let quoted = if cfg!(windows) {
            format!("\"{}\"", binary.display())
        } else {
            format!("'{}'", binary.display().to_string().replace('\'', "'\\''"))
        };
        let command = format!(
            "{} --exact runner::tests::process_fixture --nocapture",
            quoted
        );
        let root = tmp.path().to_path_buf();
        let handle = tokio::spawn(async move { execute(&command, &root, &env, None).await });
        let ready = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while !heartbeat.exists() {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await;
        handle.abort();
        let stopped = handle.await;
        assert!(ready.is_ok(), "descendant never started: {stopped:?}");
        // Allow an already-buffered write to settle, then verify the actual
        // grandchild stopped rather than merely losing its stdout connection.
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        let size = std::fs::metadata(&heartbeat).unwrap().len();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert_eq!(
            std::fs::metadata(&heartbeat).unwrap().len(),
            size,
            "grandchild survived cancellation"
        );
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn background_children_do_not_keep_completed_task_pipes_open() {
        let tmp = tempfile::tempdir().unwrap();
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            execute("sleep 30 & echo done", tmp.path(), &HashMap::new(), None),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(result.success());
        assert!(result.logs.iter().any(|line| line.text == "done"));
    }

    #[tokio::test]
    async fn captures_both_streams_and_exit_code() {
        let tmp = tempfile::tempdir().unwrap();
        let script = if cfg!(windows) {
            "echo out& echo err>&2& exit /b 3"
        } else {
            "echo out; echo err 1>&2; exit 3"
        };
        let r = execute(script, tmp.path(), &HashMap::new(), None)
            .await
            .unwrap();
        assert_eq!(r.exit_code, 3);
        assert!(r
            .logs
            .iter()
            .any(|l| l.stream == Stream::Stdout && l.text == "out"));
        assert!(r
            .logs
            .iter()
            .any(|l| l.stream == Stream::Stderr && l.text == "err"));
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn large_stderr_does_not_deadlock() {
        let tmp = tempfile::tempdir().unwrap();
        // 200k lines on stderr before anything on stdout
        let script = "i=0; while [ $i -lt 20000 ]; do echo 'xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx' 1>&2; i=$((i+1)); done; echo done";
        let r = tokio::time::timeout(
            std::time::Duration::from_secs(60),
            execute(script, tmp.path(), &HashMap::new(), None),
        )
        .await
        .expect("timed out: stderr deadlock")
        .unwrap();
        assert_eq!(r.exit_code, 0);
        assert_eq!(
            r.logs.iter().filter(|l| l.stream == Stream::Stderr).count(),
            20000
        );
    }

    #[tokio::test]
    async fn env_is_passed() {
        let tmp = tempfile::tempdir().unwrap();
        let mut env = HashMap::new();
        env.insert("NEEX_TEST_VAR".to_string(), "42".to_string());
        let command = if cfg!(windows) {
            "echo %NEEX_TEST_VAR%"
        } else {
            "echo $NEEX_TEST_VAR"
        };
        let r = execute(command, tmp.path(), &env, None).await.unwrap();
        assert_eq!(r.logs[0].text, "42");
    }
}
