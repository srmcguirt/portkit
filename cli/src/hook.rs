//! Claude Code hook handlers.
//!
//! Two jobs. First, close the measurement gap: portkit's traces only ever saw
//! portkit's own tools, while the expensive calls — `Read`, `Grep`, `Bash` —
//! happen outside it. `PostToolUse` records those too, so the trace bank
//! measures the agent rather than measuring portkit measuring itself.
//!
//! Second, say something when the data says it is worth saying. Suggestions go
//! out on `UserPromptSubmit`, because that is the point where injected text is
//! shown to the model and where it is least disruptive.
//!
//! # A hook must never break the session
//!
//! Every failure here exits 0 with no output. A malformed payload, an
//! unwritable trace file, a missing directory — none of that is worth
//! interrupting the work being measured.

use std::io::Read as _;
use std::path::PathBuf;
use std::process::ExitCode;

use serde::Deserialize;
use serde_json::Value;

use portkit_core::trace::{self, CallRecord, JsonlRecorder, Outcome, Recorder, Surface};
use portkit_core::watch::{inspect, Thresholds};
use portkit_core::Config;

/// The subset of the hook payload we use. Unknown fields are ignored, so new
/// ones in Claude Code cannot break this.
#[derive(Debug, Default, Deserialize)]
struct Payload {
    #[serde(default)]
    session_id: String,
    #[serde(default)]
    tool_name: String,
    #[serde(default)]
    tool_input: Value,
    #[serde(default)]
    tool_output: Value,
}

/// Handle one hook event. Always exits successfully.
pub fn run(event: &str, config: &Config) -> ExitCode {
    let mut raw = String::new();
    if std::io::stdin().read_to_string(&mut raw).is_err() {
        return ExitCode::SUCCESS;
    }
    let payload: Payload = serde_json::from_str(&raw).unwrap_or_default();

    match event {
        "post-tool-use" => record(&payload, config),
        "user-prompt-submit" => suggest(&payload, config),
        "session-end" => summarize(&payload, config),
        _ => {}
    }
    ExitCode::SUCCESS
}

fn trace_dir(config: &Config) -> PathBuf {
    PathBuf::from(&config.trace.dir)
}

fn today() -> String {
    let d = time::OffsetDateTime::now_utc().date();
    format!("{:04}-{:02}-{:02}", d.year(), u8::from(d.month()), d.day())
}

/// What a call was about — enough to spot the same target twice, without
/// storing arguments, which would put file contents into the trace bank.
fn target_of(tool: &str, input: &Value) -> Option<String> {
    let s = |k: &str| input.get(k).and_then(Value::as_str).map(str::to_string);
    match tool {
        "Read" | "NotebookRead" | "Edit" | "Write" | "NotebookEdit" => s("file_path"),
        "WebFetch" => s("url"),
        "WebSearch" => s("query"),
        "Grep" | "Glob" => s("pattern"),
        // Commands vary in their arguments; the head is what repeats.
        "Bash" => s("command").map(|c| {
            c.split_whitespace()
                .take(2)
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(60)
                .collect()
        }),
        _ => s("file_path").or_else(|| s("path")).or_else(|| s("name")),
    }
}

fn output_bytes(output: &Value) -> usize {
    match output {
        Value::String(s) => s.len(),
        Value::Null => 0,
        other => serde_json::to_vec(other).map(|v| v.len()).unwrap_or(0),
    }
}

fn record(payload: &Payload, config: &Config) {
    if !config.trace.enabled || payload.tool_name.is_empty() {
        return;
    }
    let path = trace_dir(config).join(format!("{}.jsonl", today()));
    let Ok(recorder) = JsonlRecorder::create(&path) else {
        return;
    };

    let bytes = output_bytes(&payload.tool_output);
    recorder.record(CallRecord {
        tool: payload.tool_name.clone(),
        session: payload.session_id.clone(),
        target: target_of(&payload.tool_name, &payload.tool_input),
        surface: Surface::Agent,
        outcome: Outcome::Ok,
        at: trace::now_rfc3339(),
        duration_ms: 0.0, // the hook sees the result, not the call
        input_bytes: serde_json::to_vec(&payload.tool_input)
            .map(|v| v.len())
            .unwrap_or(0),
        produced_bytes: bytes,
        // Nothing budgeted it; what arrived is what context paid for.
        delivered_bytes: bytes,
        elisions: 0,
    });
}

/// Where the targets already mentioned this session are remembered, so the
/// same advice is not repeated.
fn said_path(config: &Config, session: &str) -> PathBuf {
    let short: String = session.chars().take(32).collect();
    trace_dir(config).join("said").join(format!("{short}.txt"))
}

fn load_said(path: &PathBuf) -> Vec<String> {
    std::fs::read_to_string(path)
        .map(|s| s.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

fn suggest(payload: &Payload, config: &Config) {
    if !config.trace.enabled {
        return;
    }
    let path = trace_dir(config).join(format!("{}.jsonl", today()));
    let Ok(all) = trace::read_jsonl(&path) else {
        return;
    };

    let mine: Vec<CallRecord> = all
        .into_iter()
        .filter(|r| r.session == payload.session_id)
        .collect();
    let said_at = said_path(config, &payload.session_id);
    let said = load_said(&said_at);

    let Some(s) = inspect(&mine, &said, &Thresholds::default()) else {
        return;
    };

    // Remember before printing: a crash after the advice would otherwise
    // repeat it next turn.
    if let Some(parent) = said_at.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut updated = said;
    updated.push(s.target.clone());
    let _ = std::fs::write(&said_at, updated.join("\n"));

    // Plain text on UserPromptSubmit is added as context the model can see.
    println!("portkit: {}", s.message);
}

fn summarize(payload: &Payload, config: &Config) {
    if !config.trace.enabled {
        return;
    }
    let path = trace_dir(config).join(format!("{}.jsonl", today()));
    let Ok(all) = trace::read_jsonl(&path) else {
        return;
    };
    let mine: Vec<&CallRecord> = all
        .iter()
        .filter(|r| r.session == payload.session_id)
        .collect();
    if mine.is_empty() {
        return;
    }

    let delivered: usize = mine.iter().map(|r| r.delivered_bytes).sum();
    let saved: usize = mine.iter().map(|r| r.saved_bytes()).sum();
    // stderr: session-end stdout is not shown as context, and this is for the
    // human reading their terminal.
    eprintln!(
        "portkit: {} calls, {:.1} KB delivered, {:.1} KB kept out of context",
        mine.len(),
        delivered as f64 / 1000.0,
        saved as f64 / 1000.0
    );

    // The said-list is per session; clean up rather than accumulate files.
    let _ = std::fs::remove_file(said_path(config, &payload.session_id));
}
