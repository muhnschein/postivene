//! `POSTIVENE_MEMORY_LOG=<seconds>`: a line on stderr every so often with
//! the resident size of this process and of the core it spawned.
//!
//! Profiling a Qt app on a phone has no good tools -- no `heaptrack`, no
//! `valgrind`, and `top` shows one number for two processes that grow
//! for different reasons. This is the cheap thing that answers the first
//! question, which is *which one*: the app, where every decoded image is
//! a texture, or the core, where the database cache lives. Read from
//! `/proc`, which is what `ps` reads; the same two numbers are the ones
//! `smem` and `/proc/<pid>/smaps_rollup` refine.

use std::time::Duration;

/// Start logging if the variable is set to a whole number of seconds;
/// do nothing otherwise. The thread is a daemon: it holds nothing and
/// dies with the process.
pub fn start_from_env() {
    let Some(every) = std::env::var("POSTIVENE_MEMORY_LOG")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|seconds| *seconds > 0)
    else {
        return;
    };
    let result = std::thread::Builder::new()
        .name("memory-log".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_secs(every));
            eprintln!("{}", report());
        });
    if result.is_err() {
        eprintln!("memory: could not start the log thread");
    }
}

/// One line: the app's resident size, and the core's while it runs.
fn report() -> String {
    let app = resident_mib("self").map_or_else(|| "?".to_string(), |mib| format!("{mib} MiB"));
    let core = postivene_shim::server_pid()
        .and_then(|pid| resident_mib(&pid.to_string()))
        .map_or_else(|| "not running".to_string(), |mib| format!("{mib} MiB"));
    format!("memory: app {app} resident, core {core}")
}

/// `VmRSS` of `/proc/<pid>/status`, in whole MiB. The developer view's
/// recorder reads the same line, and more; this is the one-line version.
fn resident_mib(pid: &str) -> Option<u64> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    postivene_shim::recorder::status_kib(&status, "VmRSS").map(|kib| kib / 1024)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_process_has_a_resident_size() {
        assert!(
            resident_mib("self").is_some(),
            "/proc/self/status is unreadable, so the log would say nothing"
        );
    }
}
