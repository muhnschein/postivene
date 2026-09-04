//! What the phone can say about the app while it runs: the developer
//! view's recorder.
//!
//! A phone has none of the tools a workstation profiles with -- no
//! `heaptrack`, no `perf` inside the sandbox, no `valgrind` -- but it has
//! `/proc`, and the app can read its own. [`DevRecorder`] is the QML face
//! of that: while a recording runs it writes, once a second, what the app
//! and the core weigh and do -- proportional memory, CPU by thread, the
//! frames the window presented and the longest gap between them -- into a
//! directory the reader can copy off the phone over SSH, with the marks
//! they typed while reproducing something, a full `smaps` on demand, the
//! kernel facts docs/SECURITY.md left to a device to answer, and a script
//! for the one thing the app cannot do from inside its sandbox, which is
//! trace its own syscalls. docs/BUILDING.md says how to read what it
//! writes.
//!
//! The recorder proper, [`Recorder`], knows nothing of Qt, so it can be
//! run and read back in a unit test; the QObject is the thin part.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use qmetaobject::*;

/// `USER_HZ`: what `/proc/<pid>/stat` counts CPU time in. 100 on every
/// Linux that has shipped to userspace, whatever the kernel's own tick.
const CLOCK_TICKS: u64 = 100;
/// How often a recording samples both processes.
const SAMPLE_EVERY: Duration = Duration::from_secs(1);
/// The slice the sampler sleeps in, so that stopping does not wait out a
/// whole sample.
const SLICE: Duration = Duration::from_millis(100);
/// How many of a process's threads a sample names, busiest first.
const BUSIEST_THREADS: usize = 4;

/// What each kind of line in `timeline.tsv` carries.
const TIMELINE_HEADER: &str = "\
# Postivene developer recording. One line per event, tab-separated, the
# first column milliseconds since the recording started.
# frame     <t> frame <frames presented> <main-thread beats> <worst frame gap ms> <worst beat gap ms>
# mem       <t> mem <app|core> <pid> <rss KiB> <pss KiB> <anon KiB> <private dirty KiB> <threads> <fds> <cpu %>
# thread    <t> thread <app|core> <thread name> <cpu %>
# mark      <t> mark <what the reader typed>
# snapshot  <t> snapshot <directory>
# stop      <t> stop";

// Frames and beats are counted from whichever thread sees them -- the
// window's render thread for a frame, the main thread for a beat -- so
// they are atomics rather than fields, and reset when a recording starts.
static FRAMES: AtomicU64 = AtomicU64::new(0);
static BEATS: AtomicU64 = AtomicU64::new(0);
static WORST_FRAME_GAP_US: AtomicU64 = AtomicU64::new(0);
static WORST_BEAT_GAP_US: AtomicU64 = AtomicU64::new(0);
static LAST_FRAME_US: AtomicU64 = AtomicU64::new(0);
static LAST_BEAT_US: AtomicU64 = AtomicU64::new(0);

/// Microseconds since the first time anything asked.
fn now_us() -> u64 {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    u64::try_from(EPOCH.get_or_init(Instant::now).elapsed().as_micros()).unwrap_or(u64::MAX)
}

/// The window presented a frame.
///
/// Called by the app's window hook from the render thread, which is why
/// this touches nothing but counters: a frame that takes longer than the
/// last widens the worst gap of the current second.
pub fn note_frame() {
    pulse(&FRAMES, &LAST_FRAME_US, &WORST_FRAME_GAP_US);
}

/// The heartbeat animation stepped on the main thread.
///
/// The window can present frames while the main thread is stuck -- the
/// render thread has its own -- so the two are counted apart: a gap in
/// the beats is the main thread stalling, which is the thread QML, image
/// decoding hand-offs and the core's replies all run on.
pub fn note_beat() {
    pulse(&BEATS, &LAST_BEAT_US, &WORST_BEAT_GAP_US);
}

fn pulse(count: &AtomicU64, last: &AtomicU64, worst: &AtomicU64) {
    let now = now_us();
    count.fetch_add(1, Ordering::Relaxed);
    let before = last.swap(now, Ordering::Relaxed);
    if before != 0 {
        worst.fetch_max(now.saturating_sub(before), Ordering::Relaxed);
    }
}

/// The counts since the last reading, which this resets.
struct PulseReading {
    frames: u64,
    beats: u64,
    worst_frame_gap_us: u64,
    worst_beat_gap_us: u64,
}

fn take_pulse() -> PulseReading {
    PulseReading {
        frames: FRAMES.swap(0, Ordering::Relaxed),
        beats: BEATS.swap(0, Ordering::Relaxed),
        worst_frame_gap_us: WORST_FRAME_GAP_US.swap(0, Ordering::Relaxed),
        worst_beat_gap_us: WORST_BEAT_GAP_US.swap(0, Ordering::Relaxed),
    }
}

fn reset_pulse() {
    for counter in [
        &FRAMES,
        &BEATS,
        &WORST_FRAME_GAP_US,
        &WORST_BEAT_GAP_US,
        &LAST_FRAME_US,
        &LAST_BEAT_US,
    ] {
        counter.store(0, Ordering::Relaxed);
    }
}

/// The processes a recording watches: the app, and the core while it
/// runs.
fn watched() -> Vec<(&'static str, u32)> {
    let mut out = vec![("app", std::process::id())];
    if let Some(pid) = crate::core::server_pid() {
        out.push(("core", pid));
    }
    out
}

/// One `Key:   1234 kB` line of a `/proc/<pid>/status`-shaped file, in
/// KiB. Exact on the key: `Pss` does not read `Pss_Anon`, and `VmRSS`
/// does not read `VmRSSx`.
#[must_use]
pub fn status_kib(status: &str, key: &str) -> Option<u64> {
    status_text(status, key)
        .and_then(|value| value.split_whitespace().next().map(str::to_string))
        .and_then(|kib| kib.parse().ok())
}

/// A field of `/proc/<pid>/status`, such as `Seccomp` or `CapEff`.
fn status_text(status: &str, key: &str) -> Option<String> {
    status
        .lines()
        .find_map(|line| line.strip_prefix(key)?.strip_prefix(':'))
        .map(|rest| rest.trim().to_string())
}

/// The name and the CPU ticks (`utime + stime`) of a `/proc/<pid>/stat`
/// or `task/<tid>/stat` line.
///
/// The name sits in parentheses and can hold spaces and parentheses of
/// its own, so the fields are counted from the last `)` rather than from
/// the front.
fn stat_cpu(stat: &str) -> Option<(String, u64)> {
    let open = stat.find('(')?;
    let close = stat.rfind(')')?;
    let comm = stat.get(open + 1..close)?.to_string();
    // After the name: state, ppid, pgrp, session, tty_nr, tpgid, flags,
    // minflt, cminflt, majflt, cmajflt, then utime and stime.
    let mut fields = stat.get(close + 1..)?.split_whitespace();
    let utime: u64 = fields.nth(11)?.parse().ok()?;
    let stime: u64 = fields.next()?.parse().ok()?;
    Some((comm, utime + stime))
}

/// A process's or a thread's CPU clock, keyed by (pid, tid) with tid 0
/// for the process, remembered between samples.
type Clocks = HashMap<(u32, u32), u64>;

/// Per mille of one CPU this clock spent since it was last read: 1000 is
/// one core flat out. 0 the first time, when there is nothing to compare.
fn permille(clocks: &mut Clocks, key: (u32, u32), ticks: u64, interval: Duration) -> u64 {
    let Some(before) = clocks.insert(key, ticks) else {
        return 0;
    };
    let elapsed_ms = u64::try_from(interval.as_millis())
        .unwrap_or(u64::MAX)
        .max(1);
    ticks.saturating_sub(before) * 1_000_000 / (CLOCK_TICKS * elapsed_ms)
}

/// `12.3` from 123 per mille.
fn percent(permille: u64) -> String {
    format!("{}.{}", permille / 10, permille % 10)
}

/// What one process weighed and did over the last interval.
struct Sample {
    rss_kib: u64,
    pss_kib: u64,
    anon_kib: u64,
    dirty_kib: u64,
    threads: u64,
    fds: usize,
    cpu_permille: u64,
    /// Thread name and its share, busiest first.
    busiest: Vec<(String, u64)>,
}

fn sample(pid: u32, clocks: &mut Clocks, interval: Duration) -> Option<Sample> {
    let proc_dir = PathBuf::from("/proc").join(pid.to_string());
    let status = fs::read_to_string(proc_dir.join("status")).ok()?;
    // Proportional figures: `smaps_rollup` is 4.14 and later, which every
    // phone this targets has; an older kernel leaves the column at 0.
    let rollup = fs::read_to_string(proc_dir.join("smaps_rollup")).unwrap_or_default();
    let fds = fs::read_dir(proc_dir.join("fd")).map_or(0, Iterator::count);
    let (_, ticks) = stat_cpu(&fs::read_to_string(proc_dir.join("stat")).ok()?)?;
    let cpu_permille = permille(clocks, (pid, 0), ticks, interval);

    let mut busiest = Vec::new();
    if let Ok(tasks) = fs::read_dir(proc_dir.join("task")) {
        for task in tasks.flatten() {
            let Some(tid) = task
                .file_name()
                .to_str()
                .and_then(|name| name.parse::<u32>().ok())
            else {
                continue;
            };
            let Some((comm, ticks)) = fs::read_to_string(task.path().join("stat"))
                .ok()
                .and_then(|stat| stat_cpu(&stat))
            else {
                continue;
            };
            let share = permille(clocks, (pid, tid), ticks, interval);
            if share > 0 {
                busiest.push((comm, share));
            }
        }
    }
    busiest.sort_by(|a, b| b.1.cmp(&a.1));
    busiest.truncate(BUSIEST_THREADS);

    Some(Sample {
        rss_kib: status_kib(&status, "VmRSS").unwrap_or(0),
        pss_kib: status_kib(&rollup, "Pss").unwrap_or(0),
        anon_kib: status_kib(&status, "RssAnon").unwrap_or(0),
        dirty_kib: status_kib(&rollup, "Private_Dirty").unwrap_or(0),
        threads: status_text(&status, "Threads")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0),
        fds,
        cpu_permille,
        busiest,
    })
}

/// The first line of a file, or why it could not be read.
fn first_line(path: &str) -> String {
    match fs::read_to_string(path) {
        Ok(text) => text.lines().next().unwrap_or("").trim().to_string(),
        Err(err) => format!("unreadable ({err})"),
    }
}

/// Whether the kernel has Landlock, from what it says about itself: the
/// LSM list names it when it is enabled, and the kernel's symbol table
/// carries its entry point when it is merely built in -- which is what
/// docs/SECURITY.md wanted read off a device. The symbol table's
/// addresses are hidden from an unprivileged reader; its names are not.
fn describe_landlock(lsm: &str, kallsyms: &str) -> String {
    if lsm.split(',').any(|name| name.trim() == "landlock") {
        return "enabled (in the LSM list): confining the core is a contained change".into();
    }
    if kallsyms
        .lines()
        .any(|line| line.split_whitespace().nth(2) == Some("landlock_create_ruleset"))
    {
        return "built into the kernel but not enabled (not in the LSM list)".into();
    }
    if lsm.is_empty() && kallsyms.is_empty() {
        return "unknown: neither /sys/kernel/security/lsm nor /proc/kallsyms is readable here"
            .into();
    }
    "not in this kernel".into()
}

fn describe_seccomp(mode: Option<String>) -> String {
    match mode.as_deref() {
        Some("0") => "0 (none)".into(),
        Some("1") => "1 (strict)".into(),
        Some("2") => "2 (filter mode: a seccomp filter is on, the sandbox's)".into(),
        Some(other) => other.to_string(),
        None => "unknown (no Seccomp line in /proc/self/status)".into(),
    }
}

/// The kernel and sandbox facts docs/SECURITY.md could only guess at from
/// a build machine, read off the phone.
#[must_use]
pub fn system_report() -> String {
    let status = fs::read_to_string("/proc/self/status").unwrap_or_default();
    let field = |key: &str| status_text(&status, key).unwrap_or_else(|| "unknown".into());
    let core = crate::core::server_pid().map_or_else(|| "not running".to_string(), |pid| pid.to_string());
    let lsm = fs::read_to_string("/sys/kernel/security/lsm").unwrap_or_default();
    let kallsyms = fs::read_to_string("/proc/kallsyms").unwrap_or_default();
    [
        format!("Kernel: {}", first_line("/proc/version")),
        format!("App pid: {}", std::process::id()),
        format!("Core pid: {core}"),
        format!("Uid: {}", field("Uid")),
        format!("Seccomp: {}", describe_seccomp(status_text(&status, "Seccomp"))),
        format!(
            "Seccomp filters: {}",
            status_text(&status, "Seccomp_filters")
                .unwrap_or_else(|| "unknown (kernel before 5.9)".into())
        ),
        format!("NoNewPrivs: {}", field("NoNewPrivs")),
        format!("CapEff: {}", field("CapEff")),
        format!("CapBnd: {}", field("CapBnd")),
        format!("LSM: {}", first_line("/sys/kernel/security/lsm")),
        format!("Landlock: {}", describe_landlock(lsm.trim(), &kallsyms)),
        format!(
            "ptrace_scope: {}",
            first_line("/proc/sys/kernel/yama/ptrace_scope")
        ),
        format!(
            "Home: {}",
            std::env::var("HOME").unwrap_or_else(|_| "unset".into())
        ),
    ]
    .join("\n")
}

/// The files a process has mapped, each once, sorted: what it loaded,
/// for the syscall question and the memory one.
fn mapped_files(pid: u32) -> String {
    let maps = fs::read_to_string(format!("/proc/{pid}/maps")).unwrap_or_default();
    let mut files: Vec<&str> = maps
        .lines()
        .filter_map(|line| line.split_whitespace().nth(5))
        .filter(|path| path.starts_with('/'))
        .collect();
    files.sort_unstable();
    files.dedup();
    let mut out = files.join("\n");
    out.push('\n');
    out
}

/// The script that traces both processes from outside the sandbox.
///
/// The app cannot trace itself: the sandbox's seccomp filter drops
/// `ptrace`, and a tracer is another process in any case. Over SSH, as
/// root, `strace` attaches from outside; this writes the invocation with
/// the pids filled in and the summary step that turns a trace into the
/// distinct syscalls each process made.
fn strace_script(dir: &Path, app: u32, core: Option<u32>) -> String {
    let core = core.map_or_else(String::new, |pid| pid.to_string());
    format!(
        "#!/bin/sh
# Written by Postivene's developer view. Run it on the phone as root:
#
#     devel-su sh {dir}/strace.sh [seconds]
#
# The app cannot trace itself: its sandbox's seccomp filter drops ptrace.
# From outside the sandbox strace attaches to both processes and, after
# the seconds asked for (60 by default), writes the distinct syscalls of
# each -- what a whitelist would have to allow -- to syscalls-app.txt and
# syscalls-core.txt beside the raw traces. Drive the app meanwhile: open
# chats, send a picture, play a voice message, so the paths that only run
# then are in the list.
set -u
OUT=\"{dir}\"
APP={app}
CORE=\"{core}\"
SECONDS_TO_RUN=\"${{1:-60}}\"
if ! command -v strace >/dev/null 2>&1; then
    echo \"strace is not installed: pkcon install strace\" >&2
    exit 1
fi
strace -f -qq -o \"$OUT/strace-app.txt\" -p \"$APP\" &
APP_TRACER=$!
CORE_TRACER=\"\"
if [ -n \"$CORE\" ]; then
    strace -f -qq -o \"$OUT/strace-core.txt\" -p \"$CORE\" &
    CORE_TRACER=$!
fi
echo \"tracing for $SECONDS_TO_RUN seconds; use the app now\"
sleep \"$SECONDS_TO_RUN\"
kill -INT \"$APP_TRACER\" $CORE_TRACER 2>/dev/null
wait
for side in app core; do
    [ -f \"$OUT/strace-$side.txt\" ] || continue
    sed -n 's/^\\([0-9]* *\\)\\{{0,1\\}}\\([a-z_0-9]*\\)(.*/\\2/p' \"$OUT/strace-$side.txt\" \\
        | sort | uniq -c | sort -rn > \"$OUT/syscalls-$side.txt\"
    echo \"$side: $(wc -l < \"$OUT/syscalls-$side.txt\") distinct syscalls, in $OUT/syscalls-$side.txt\"
done
",
        dir = dir.display(),
    )
}

/// What the sampler thread and the recorder share.
struct Shared {
    running: AtomicBool,
    timeline: Mutex<Option<File>>,
    snapshots: AtomicU64,
}

impl Shared {
    fn append(&self, line: &str) {
        if let Some(file) = self
            .timeline
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_mut()
        {
            let _ = writeln!(file, "{line}");
        }
    }
}

/// What has been said since the recording started, in whole
/// milliseconds.
fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// The thread a recording runs on: a sample a second until stopped.
fn sample_loop(shared: Arc<Shared>, started: Instant, on_sample: Box<dyn Fn(String) + Send>) {
    let mut clocks = Clocks::new();
    // Read once so the first sample has a clock to compare against.
    for (_, pid) in watched() {
        let _ = sample(pid, &mut clocks, SAMPLE_EVERY);
    }
    let mut last = Instant::now();
    while shared.running.load(Ordering::Relaxed) {
        let due = last + SAMPLE_EVERY;
        while Instant::now() < due {
            if !shared.running.load(Ordering::Relaxed) {
                return;
            }
            std::thread::sleep(SLICE);
        }
        let now = Instant::now();
        let interval = now.duration_since(last);
        last = now;

        let at = elapsed_ms(started);
        let pulse = take_pulse();
        shared.append(&format!(
            "{at}\tframe\t{}\t{}\t{}\t{}",
            pulse.frames,
            pulse.beats,
            pulse.worst_frame_gap_us / 1000,
            pulse.worst_beat_gap_us / 1000
        ));
        let mut summary = format!(
            "{} fps, worst gap {} ms (main thread {} ms)",
            pulse.frames,
            pulse.worst_frame_gap_us / 1000,
            pulse.worst_beat_gap_us / 1000
        );
        for (name, pid) in watched() {
            let Some(sample) = sample(pid, &mut clocks, interval) else {
                continue;
            };
            shared.append(&format!(
                "{at}\tmem\t{name}\t{pid}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                sample.rss_kib,
                sample.pss_kib,
                sample.anon_kib,
                sample.dirty_kib,
                sample.threads,
                sample.fds,
                percent(sample.cpu_permille)
            ));
            for (comm, share) in &sample.busiest {
                shared.append(&format!("{at}\tthread\t{name}\t{comm}\t{}", percent(*share)));
            }
            summary.push_str(&format!(
                "; {name} {} MiB pss, {}% cpu",
                sample.pss_kib / 1024,
                percent(sample.cpu_permille)
            ));
        }
        on_sample(summary);
    }
}

/// A recording in progress.
struct Run {
    dir: PathBuf,
    started: Instant,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Run {
    fn drop(&mut self) {
        self.shared.running.store(false, Ordering::Relaxed);
    }
}

/// The recorder itself, with no Qt in it.
pub struct Recorder {
    root: PathBuf,
    run: Option<Run>,
}

fn write_file(path: &Path, text: &str) -> Result<(), String> {
    fs::write(path, text).map_err(|err| format!("cannot write {}: {err}", path.display()))
}

impl Recorder {
    /// A recorder that puts each recording in a directory of its own
    /// under `root`.
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root, run: None }
    }

    /// Whether a recording is running.
    #[must_use]
    pub fn is_recording(&self) -> bool {
        self.run.is_some()
    }

    /// Start a recording: a fresh directory with the system report, the
    /// trace script, the mounts and the mapped files in it, and the
    /// timeline being written by a thread of its own. `on_sample` is
    /// handed a one-line summary of each sample, for a live display.
    ///
    /// # Errors
    ///
    /// When a recording is already running, or the directory or its
    /// first files cannot be written.
    pub fn start(&mut self, on_sample: Box<dyn Fn(String) + Send>) -> Result<PathBuf, String> {
        if self.run.is_some() {
            return Err("already recording".into());
        }
        let dir = self
            .root
            .join(chrono::Local::now().format("%Y%m%d-%H%M%S").to_string());
        fs::create_dir_all(&dir).map_err(|err| format!("cannot create {}: {err}", dir.display()))?;

        let app = std::process::id();
        let core = crate::core::server_pid();
        write_file(&dir.join("system.txt"), &system_report())?;
        write_file(&dir.join("strace.sh"), &strace_script(&dir, app, core))?;
        write_file(
            &dir.join("mounts.txt"),
            &fs::read_to_string("/proc/self/mounts").unwrap_or_default(),
        )?;
        for (name, pid) in watched() {
            write_file(&dir.join(format!("maps-{name}.txt")), &mapped_files(pid))?;
        }
        let mut timeline = File::create(dir.join("timeline.tsv"))
            .map_err(|err| format!("cannot create the timeline: {err}"))?;
        writeln!(timeline, "{TIMELINE_HEADER}")
            .map_err(|err| format!("cannot write the timeline: {err}"))?;

        reset_pulse();
        let shared = Arc::new(Shared {
            running: AtomicBool::new(true),
            timeline: Mutex::new(Some(timeline)),
            snapshots: AtomicU64::new(0),
        });
        let started = Instant::now();
        let sampler = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("dev-recorder".into())
            .spawn(move || sample_loop(sampler, started, on_sample))
            .map_err(|err| format!("cannot start the sampler: {err}"))?;
        self.run = Some(Run {
            dir: dir.clone(),
            started,
            shared,
            thread: Some(thread),
        });
        Ok(dir)
    }

    /// Stop the recording, once the sampler has written its last line.
    pub fn stop(&mut self) {
        let Some(mut run) = self.run.take() else {
            return;
        };
        run.shared
            .append(&format!("{}\tstop", elapsed_ms(run.started)));
        run.shared.running.store(false, Ordering::Relaxed);
        if let Some(thread) = run.thread.take() {
            let _ = thread.join();
        }
    }

    /// Put a line the reader wrote on the timeline: "opening the chat with
    /// the twenty photos", so the numbers around it mean something later.
    /// False when nothing is recording.
    pub fn mark(&self, label: &str) -> bool {
        let Some(run) = self.run.as_ref() else {
            return false;
        };
        let label: String = label
            .chars()
            .map(|c| if c == '\t' || c == '\n' || c == '\r' { ' ' } else { c })
            .collect();
        run.shared
            .append(&format!("{}\tmark\t{label}", elapsed_ms(run.started)));
        true
    }

    /// Everything `/proc` says about both processes right now -- the full
    /// `smaps`, the file descriptors, the threads -- in a numbered
    /// directory of the recording, noted on the timeline.
    ///
    /// # Errors
    ///
    /// When nothing is recording, or the directory cannot be made.
    pub fn snapshot(&self) -> Result<PathBuf, String> {
        let run = self.run.as_ref().ok_or("not recording")?;
        let number = run.shared.snapshots.fetch_add(1, Ordering::Relaxed) + 1;
        let name = format!("snapshot-{number}");
        let dir = run.dir.join(&name);
        fs::create_dir_all(&dir).map_err(|err| format!("cannot create {}: {err}", dir.display()))?;
        for (side, pid) in watched() {
            let proc_dir = PathBuf::from("/proc").join(pid.to_string());
            // Read and written rather than copied: procfs files report a
            // size of zero, which a copy takes at its word.
            for file in ["status", "smaps", "maps", "stat"] {
                let text = fs::read_to_string(proc_dir.join(file)).unwrap_or_default();
                write_file(&dir.join(format!("{file}-{side}.txt")), &text)?;
            }
            let mut fds = String::new();
            if let Ok(entries) = fs::read_dir(proc_dir.join("fd")) {
                for entry in entries.flatten() {
                    let target = fs::read_link(entry.path())
                        .map_or_else(|_| "?".into(), |t| t.display().to_string());
                    fds.push_str(&format!("{} -> {target}\n", entry.file_name().to_string_lossy()));
                }
            }
            write_file(&dir.join(format!("fd-{side}.txt")), &fds)?;
            let mut threads = String::new();
            if let Ok(tasks) = fs::read_dir(proc_dir.join("task")) {
                for task in tasks.flatten() {
                    let comm = fs::read_to_string(task.path().join("comm")).unwrap_or_default();
                    threads.push_str(&format!(
                        "{} {}\n",
                        task.file_name().to_string_lossy(),
                        comm.trim()
                    ));
                }
            }
            write_file(&dir.join(format!("threads-{side}.txt")), &threads)?;
        }
        run.shared
            .append(&format!("{}\tsnapshot\t{name}", elapsed_ms(run.started)));
        Ok(dir)
    }
}

/// Where recordings go unless told otherwise: `POSTIVENE_RECORDINGS_DIR`,
/// else `Documents/postivene-recordings` under the home directory, which
/// the sandbox's `UserDirs` grant lets the app write and an SSH session
/// read.
fn default_root() -> PathBuf {
    if let Ok(dir) = std::env::var("POSTIVENE_RECORDINGS_DIR") {
        return PathBuf::from(dir);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home)
        .join("Documents")
        .join("postivene-recordings")
}

/// The developer view's recorder, for QML.
///
/// ```qml
/// DevRecorder { id: recorder }
/// Button { text: recorder.recording ? "Stop" : "Start"
///          onClicked: recorder.recording ? recorder.stop() : recorder.start() }
/// ```
///
/// One per app, made by the root window: a recording has to outlive the
/// page that started it, since what is being recorded is the reader
/// going elsewhere in the app.
#[derive(QObject, Default)]
pub struct DevRecorder {
    base: qt_base_class!(trait QObject),

    /// True while a recording runs.
    pub recording: qt_property!(bool; NOTIFY recording_changed),
    /// Emitted when [`Self::recording`] changes.
    pub recording_changed: qt_signal!(),

    /// Where recordings go, each in a directory of its own. Empty for
    /// the default (see the module docs); a test points it at a temporary
    /// directory.
    pub root_dir: qt_property!(QString; WRITE set_root_dir NOTIFY root_dir_changed),
    /// Emitted when [`Self::root_dir`] changes.
    pub root_dir_changed: qt_signal!(),

    /// The directory the running, or the last, recording writes to.
    pub output_dir: qt_property!(QString; NOTIFY output_dir_changed),
    /// Emitted when [`Self::output_dir`] changes.
    pub output_dir_changed: qt_signal!(),

    /// What the last thing asked for came to: where a recording went, or
    /// why it did not.
    pub status: qt_property!(QString; NOTIFY status_changed),
    /// Emitted when [`Self::status`] changes.
    pub status_changed: qt_signal!(),

    /// The latest sample in a line, while recording.
    pub summary: qt_property!(QString; NOTIFY summary_changed),
    /// Emitted when [`Self::summary`] changes.
    pub summary_changed: qt_signal!(),

    /// The kernel and sandbox facts, once [`Self::probe_system`] has been
    /// asked for them.
    pub system_report: qt_property!(QString; NOTIFY system_report_changed),
    /// Emitted when [`Self::system_report`] changes.
    pub system_report_changed: qt_signal!(),

    /// Start a recording.
    pub start: qt_method!(fn(&mut self)),
    /// Stop it.
    pub stop: qt_method!(fn(&mut self)),
    /// Put a line the reader wrote on the timeline.
    pub mark: qt_method!(fn(&mut self, label: QString)),
    /// Dump both processes' `/proc` state into the recording.
    pub snapshot: qt_method!(fn(&mut self)),
    /// The heartbeat animation stepped: see [`note_beat`].
    pub beat: qt_method!(fn(&mut self)),
    /// Read the kernel and sandbox facts into [`Self::system_report`].
    pub probe_system: qt_method!(fn(&mut self)),

    recorder: Option<Recorder>,
}

impl DevRecorder {
    fn recorder(&mut self) -> &mut Recorder {
        if self.recorder.is_none() {
            let root = if self.root_dir.is_empty() {
                default_root()
            } else {
                PathBuf::from(self.root_dir.to_string())
            };
            self.recorder = Some(Recorder::new(root));
        }
        // Just put there when it was not.
        self.recorder.get_or_insert_with(|| Recorder::new(default_root()))
    }

    fn set_status(&mut self, text: String) {
        self.status = text.into();
        self.status_changed();
    }

    /// Point recordings somewhere else. Ignored while one runs.
    pub fn set_root_dir(&mut self, dir: QString) {
        self.root_dir = dir;
        self.root_dir_changed();
        if !self.recording {
            self.recorder = None;
        }
    }

    /// Start a recording; [`Self::status`] says where, or why not.
    pub fn start(&mut self) {
        let ptr: QPointer<Self> = QPointer::from(&*self);
        let on_sample = queued_callback(move |line: String| {
            let Some(this) = ptr.as_pinned() else { return };
            this.borrow_mut().summary = line.into();
            this.borrow().summary_changed();
        });
        match self.recorder().start(Box::new(on_sample)) {
            Ok(dir) => {
                self.output_dir = dir.display().to_string().into();
                self.output_dir_changed();
                self.recording = true;
                self.recording_changed();
                self.set_status(format!("Recording to {}", dir.display()));
            }
            Err(err) => self.set_status(err),
        }
    }

    /// Stop the recording.
    pub fn stop(&mut self) {
        self.recorder().stop();
        if self.recording {
            self.recording = false;
            self.recording_changed();
        }
        self.summary = QString::default();
        self.summary_changed();
        let dir = self.output_dir.to_string();
        self.set_status(format!("Stopped. The recording is in {dir}"));
    }

    /// Put a line the reader wrote on the timeline.
    pub fn mark(&mut self, label: QString) {
        let label = label.to_string();
        if self.recorder().mark(&label) {
            self.set_status(format!("Marked: {label}"));
        } else {
            self.set_status("Not recording, so there is no timeline to mark".into());
        }
    }

    /// Dump both processes' `/proc` state into the recording.
    pub fn snapshot(&mut self) {
        match self.recorder().snapshot() {
            Ok(dir) => self.set_status(format!("Snapshot in {}", dir.display())),
            Err(err) => self.set_status(err),
        }
    }

    /// The heartbeat animation stepped on the main thread.
    pub fn beat(&mut self) {
        note_beat();
    }

    /// Read the kernel and sandbox facts.
    pub fn probe_system(&mut self) {
        self.system_report = system_report().into();
        self.system_report_changed();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stat_line_is_read_from_behind_the_name() {
        // A name with a space and parentheses in it, as a thread can have.
        let stat = "4242 (Thread (pooled)) S 1 4242 4242 0 -1 4194560 99 0 0 0 150 25 0 0 20 0 9 0 12345 0 0 18446744073709551615 0 0 0 0 0 0 0 0 0 0 0 0 17 3 0 0 0 0 0";
        assert_eq!(
            stat_cpu(stat),
            Some(("Thread (pooled)".to_string(), 175)),
            "utime 150 and stime 25 are the fields after the name"
        );
        assert_eq!(stat_cpu("garbage"), None);
    }

    #[test]
    fn a_status_field_is_matched_whole() {
        let rollup = "Rss:  1000 kB\nPss_Anon:  200 kB\nPss:  300 kB\nPrivate_Dirty:  40 kB\n";
        assert_eq!(status_kib(rollup, "Pss"), Some(300), "not Pss_Anon");
        assert_eq!(status_kib(rollup, "Rss"), Some(1000));
        assert_eq!(status_kib(rollup, "Private_Dirty"), Some(40));
        assert_eq!(status_kib(rollup, "Swap"), None);
        let status = "Name:\tharbour-postiven\nVmRSSx:\t 1 kB\nVmRSS:\t  262144 kB\nSeccomp:\t2\n";
        assert_eq!(status_kib(status, "VmRSS"), Some(262_144));
        assert_eq!(status_text(status, "Seccomp").as_deref(), Some("2"));
    }

    #[test]
    fn cpu_is_the_share_of_one_core_since_the_last_reading() {
        let mut clocks = Clocks::new();
        let second = Duration::from_secs(1);
        assert_eq!(permille(&mut clocks, (1, 0), 100, second), 0, "nothing to compare with yet");
        assert_eq!(permille(&mut clocks, (1, 0), 150, second), 500, "50 ticks of 100 in a second");
        assert_eq!(
            permille(&mut clocks, (1, 0), 350, Duration::from_secs(2)),
            1000,
            "200 ticks over two seconds is one core flat out"
        );
        assert_eq!(percent(1000), "100.0");
        assert_eq!(percent(123), "12.3");
        assert_eq!(percent(5), "0.5");
    }

    #[test]
    fn landlock_is_read_off_the_lsm_list_and_then_the_symbol_table() {
        let symbols = "0000000000000000 T landlock_create_ruleset\n0000000000000000 t other\n";
        assert!(describe_landlock("lockdown,capability,landlock,yama", "").starts_with("enabled"));
        assert!(describe_landlock("capability,yama", symbols).starts_with("built into"));
        assert_eq!(describe_landlock("capability,yama", "0 T nothing\n"), "not in this kernel");
        assert!(describe_landlock("", "").starts_with("unknown"));
        assert!(describe_seccomp(Some("2".into())).starts_with("2 (filter"));
        // Whatever this machine is, the report has a line for it.
        assert!(system_report().contains("\nLandlock: "));
    }

    #[test]
    fn the_trace_script_names_both_processes_and_the_directory() {
        let script = strace_script(Path::new("/home/x/Documents/r/1"), 1234, Some(5678));
        assert!(script.starts_with("#!/bin/sh\n"));
        assert!(script.contains("OUT=\"/home/x/Documents/r/1\""));
        assert!(script.contains("APP=1234\n"));
        assert!(script.contains("CORE=\"5678\"\n"));
        assert!(script.contains("-p \"$APP\""));
        let alone = strace_script(Path::new("/r"), 1, None);
        assert!(alone.contains("CORE=\"\"\n"), "no core is an empty CORE, which the script skips");
    }

    #[test]
    fn a_recording_writes_the_report_the_timeline_the_marks_and_a_snapshot() {
        let root = std::env::temp_dir().join(format!("postivene-recorder-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let mut recorder = Recorder::new(root.clone());
        assert!(!recorder.is_recording());
        assert!(!recorder.mark("nothing"), "a mark with no recording has nowhere to go");

        let summaries = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&summaries);
        let dir = recorder
            .start(Box::new(move |line| {
                seen.lock().unwrap_or_else(PoisonError::into_inner).push(line);
            }))
            .expect("start");
        assert!(recorder.is_recording());
        assert!(
            recorder.start(Box::new(|_: String| {})).is_err(),
            "one at a time"
        );

        for _ in 0..3 {
            note_frame();
        }
        note_beat();
        note_beat();
        // A sample lands after a second.
        std::thread::sleep(Duration::from_millis(1300));
        assert!(recorder.mark("opening\tthe chat"));
        let snapshot = recorder.snapshot().expect("snapshot");
        recorder.stop();
        assert!(!recorder.is_recording());

        let system = fs::read_to_string(dir.join("system.txt")).expect("system.txt");
        for key in ["Kernel:", "Seccomp:", "Landlock:", "LSM:", "App pid:"] {
            assert!(system.contains(key), "system.txt lacks {key}: {system}");
        }
        assert!(fs::read_to_string(dir.join("strace.sh"))
            .expect("strace.sh")
            .contains(&format!("APP={}", std::process::id())));
        assert!(dir.join("mounts.txt").is_file());
        assert!(fs::read_to_string(dir.join("maps-app.txt"))
            .expect("maps-app.txt")
            .lines()
            .any(|line| line.ends_with(".so") || line.contains(".so.")));

        let timeline = fs::read_to_string(dir.join("timeline.tsv")).expect("timeline");
        let frame = timeline
            .lines()
            .find(|line| line.split('\t').nth(1) == Some("frame"))
            .expect("a frame line");
        let fields: Vec<&str> = frame.split('\t').collect();
        assert_eq!(fields[2], "3", "three frames were noted: {frame}");
        assert_eq!(fields[3], "2", "two beats were noted: {frame}");
        assert!(
            timeline.lines().any(|line| line.starts_with(|c: char| c.is_ascii_digit()) && line.contains("\tmem\tapp\t")),
            "no memory sample for the app: {timeline}"
        );
        assert!(
            timeline.contains("\tmark\topening the chat\n"),
            "the mark is not there, or its tab was kept: {timeline}"
        );
        assert!(timeline.contains("\tsnapshot\tsnapshot-1\n"), "{timeline}");
        assert!(timeline.contains("\tstop\n"), "{timeline}");
        assert_eq!(snapshot, dir.join("snapshot-1"));
        for file in ["status-app.txt", "smaps-app.txt", "fd-app.txt", "threads-app.txt"] {
            assert!(
                fs::metadata(snapshot.join(file)).map_or(0, |m| m.len()) > 0,
                "{file} is missing or empty"
            );
        }
        let summaries = summaries.lock().unwrap_or_else(PoisonError::into_inner);
        assert!(
            summaries.iter().any(|line| line.contains("fps") && line.contains("app")),
            "no live summary reached the callback: {summaries:?}"
        );
        let _ = fs::remove_dir_all(&root);
    }
}
