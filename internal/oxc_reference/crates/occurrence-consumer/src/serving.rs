use std::{
    io::{Read, Write},
    process::{Child, Command, ExitStatus, Stdio},
    time::Instant,
};

use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShadowObservation {
    Observed,
    FailedAndRolledBack,
    SkippedAfterRollback,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServingDecision {
    pub served_response: Vec<u8>,
    pub shadow_observation: ShadowObservation,
    pub shadow_failure: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServingShadowController {
    shadow_enabled: bool,
    last_shadow_failure: Option<String>,
}

impl Default for ServingShadowController {
    fn default() -> Self {
        Self {
            shadow_enabled: true,
            last_shadow_failure: None,
        }
    }
}

impl ServingShadowController {
    pub fn serve_with_shadow<F>(
        &mut self,
        go_serving_result: Result<Vec<u8>, String>,
        shadow: F,
    ) -> Result<ServingDecision, String>
    where
        F: FnOnce() -> Result<Vec<u8>, String>,
    {
        let served_response = go_serving_result?;
        if !self.shadow_enabled {
            return Ok(ServingDecision {
                served_response,
                shadow_observation: ShadowObservation::SkippedAfterRollback,
                shadow_failure: self.last_shadow_failure.clone(),
            });
        }

        let (shadow_observation, shadow_failure) = match shadow() {
            Ok(_) => (ShadowObservation::Observed, None),
            Err(error) => {
                self.shadow_enabled = false;
                self.last_shadow_failure = Some(error.clone());
                (ShadowObservation::FailedAndRolledBack, Some(error))
            }
        };
        Ok(ServingDecision {
            served_response,
            shadow_observation,
            shadow_failure,
        })
    }

    pub fn reset_shadow(&mut self) {
        self.shadow_enabled = true;
        self.last_shadow_failure = None;
    }

    pub fn shadow_enabled(&self) -> bool {
        self.shadow_enabled
    }
}

#[derive(Clone, Debug)]
pub struct ProcessObservation {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub wall_nanoseconds: u64,
    pub peak_resident_bytes: Option<u64>,
    pub resident_measurement: &'static str,
}

pub fn run_child(command: &mut Command, stdin: &[u8]) -> Result<ProcessObservation, String> {
    let started = Instant::now();
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("launch {:?}: {error}", command.get_program()))?;
    child
        .stdin
        .take()
        .ok_or_else(|| "child stdin is unavailable".to_owned())?
        .write_all(stdin)
        .map_err(|error| format!("write child stdin: {error}"))?;
    wait_with_usage(child, started)
}

#[cfg(unix)]
fn wait_with_usage(mut child: Child, started: Instant) -> Result<ProcessObservation, String> {
    use std::os::unix::process::ExitStatusExt;

    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| "child stdout is unavailable".to_owned())?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| "child stderr is unavailable".to_owned())?;
    let pid = i32::try_from(child.id()).map_err(|_| "child PID exceeds i32".to_owned())?;
    let (status, usage, stdout, stderr) = std::thread::scope(|scope| {
        let stdout_thread = scope.spawn(move || {
            let mut bytes = Vec::new();
            stdout
                .read_to_end(&mut bytes)
                .map_err(|error| format!("read child stdout: {error}"))?;
            Ok::<_, String>(bytes)
        });
        let stderr_thread = scope.spawn(move || {
            let mut bytes = Vec::new();
            stderr
                .read_to_end(&mut bytes)
                .map_err(|error| format!("read child stderr: {error}"))?;
            Ok::<_, String>(bytes)
        });
        let mut status = 0;
        // SAFETY: `usage` is initialized before being passed to `wait4`; `pid`
        // belongs to `child`, and both output pipes are drained concurrently so
        // the child cannot block on a full pipe while `wait4` waits.
        let mut usage = unsafe { std::mem::zeroed::<libc::rusage>() };
        // SAFETY: the pointers reference valid writable values for this call,
        // and `wait4` retains neither pointer after returning.
        let waited = unsafe { libc::wait4(pid, &raw mut status, 0, &raw mut usage) };
        let stdout = stdout_thread
            .join()
            .map_err(|_| "child stdout reader panicked".to_owned())??;
        let stderr = stderr_thread
            .join()
            .map_err(|_| "child stderr reader panicked".to_owned())??;
        if waited == -1 {
            return Err(format!("wait4 child: {}", std::io::Error::last_os_error()));
        }
        Ok::<_, String>((ExitStatus::from_raw(status), usage, stdout, stderr))
    })?;

    #[cfg(target_os = "macos")]
    let peak_resident_bytes = u64::try_from(usage.ru_maxrss).ok();
    #[cfg(not(target_os = "macos"))]
    let peak_resident_bytes = u64::try_from(usage.ru_maxrss)
        .ok()
        .map(|kilobytes| kilobytes.saturating_mul(1024));

    Ok(ProcessObservation {
        status,
        stdout,
        stderr,
        wall_nanoseconds: duration_nanoseconds(started.elapsed().as_nanos()),
        peak_resident_bytes,
        resident_measurement: "wait4-ru-maxrss-peak",
    })
}

#[cfg(not(unix))]
fn wait_with_usage(child: Child, started: Instant) -> Result<ProcessObservation, String> {
    let output = child
        .wait_with_output()
        .map_err(|error| format!("wait for child: {error}"))?;
    Ok(ProcessObservation {
        status: output.status,
        stdout: output.stdout,
        stderr: output.stderr,
        wall_nanoseconds: duration_nanoseconds(started.elapsed().as_nanos()),
        peak_resident_bytes: None,
        resident_measurement: "unavailable-on-this-platform",
    })
}

fn duration_nanoseconds(value: u128) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shadow_failure_preserves_go_response_and_rolls_back() {
        let expected = b"go-serving-response".to_vec();
        let mut controller = ServingShadowController::default();
        let failed = controller
            .serve_with_shadow(Ok(expected.clone()), || Err("shadow failed".to_owned()))
            .expect("Go response remains serviceable");
        assert_eq!(failed.served_response, expected);
        assert_eq!(
            failed.shadow_observation,
            ShadowObservation::FailedAndRolledBack
        );
        assert_eq!(failed.shadow_failure.as_deref(), Some("shadow failed"));
        assert!(!controller.shadow_enabled());

        let mut shadow_called = false;
        let skipped = controller
            .serve_with_shadow(Ok(expected.clone()), || {
                shadow_called = true;
                Ok(Vec::new())
            })
            .expect("rolled-back shadow cannot affect serving");
        assert!(!shadow_called);
        assert_eq!(skipped.served_response, expected);
        assert_eq!(
            skipped.shadow_observation,
            ShadowObservation::SkippedAfterRollback
        );
        assert_eq!(skipped.shadow_failure.as_deref(), Some("shadow failed"));
    }

    #[test]
    fn explicit_reset_reenables_shadow_observation() {
        let mut controller = ServingShadowController::default();
        controller
            .serve_with_shadow(Ok(Vec::new()), || Err("shadow failed".to_owned()))
            .expect("Go response remains serviceable");
        controller.reset_shadow();
        let observed = controller
            .serve_with_shadow(Ok(Vec::new()), || Ok(b"shadow".to_vec()))
            .expect("reset shadow is observable");
        assert_eq!(observed.shadow_observation, ShadowObservation::Observed);
        assert!(observed.shadow_failure.is_none());
        assert!(controller.shadow_enabled());
    }

    #[test]
    fn go_failure_is_not_masked_by_shadow() {
        let mut controller = ServingShadowController::default();
        let mut shadow_called = false;
        let error = controller
            .serve_with_shadow(Err("Go serving failed".to_owned()), || {
                shadow_called = true;
                Ok(Vec::new())
            })
            .expect_err("the semantic authority failure must be returned");
        assert_eq!(error, "Go serving failed");
        assert!(!shadow_called);
    }
}
