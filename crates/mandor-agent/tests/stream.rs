//! Parser stream terhadap fixture NDJSON yang direkam dari Claude Code 2.1.302.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mandor_agent::claude::stream::parse_line;
use mandor_domain::AgentEvent;
use pretty_assertions::assert_eq;

fn fixture() -> String {
    std::fs::read_to_string(format!(
        "{}/tests/stream_fixtures/run_full.ndjson",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn kind(e: &AgentEvent) -> &'static str {
    match e {
        AgentEvent::SessionStarted { .. } => "session",
        AgentEvent::AssistantMessage { .. } => "assistant_message",
        AgentEvent::ToolUse { .. } => "tool_use",
        AgentEvent::ToolResult { .. } => "tool_result",
        AgentEvent::PermissionDenied { .. } => "permission_denied",
        AgentEvent::RateLimit { .. } => "rate_limit",
        AgentEvent::Usage(_) => "usage",
        AgentEvent::Done { .. } => "done",
        AgentEvent::Failed { .. } => "failed",
    }
}

#[test]
fn run_lengkap_menghasilkan_event_berurutan_dengan_session_id() {
    let events: Vec<AgentEvent> = fixture().lines().flat_map(parse_line).collect();
    let kinds: Vec<_> = events.iter().map(kind).collect();
    assert_eq!(
        kinds,
        [
            "session",
            "tool_use",
            "tool_result",
            "tool_use",
            "tool_result",
            "tool_use",
            "permission_denied",
            "tool_result",
            "rate_limit",
            "assistant_message",
            "usage",
            "done",
        ]
    );
    let AgentEvent::SessionStarted { session_id, model } = &events[0] else {
        panic!()
    };
    assert_eq!(session_id, "11111111-2222-3333-4444-555555555555");
    assert_eq!(model.as_deref(), Some("claude-haiku-5-5"));

    let AgentEvent::ToolUse { name, preview, .. } = &events[3] else {
        panic!()
    };
    assert_eq!(
        (name.as_str(), preview.as_str()),
        ("Bash", "git push origin main")
    );
    // Push ditolak hook: hasil tool berupa error.
    let AgentEvent::ToolResult {
        is_error, preview, ..
    } = &events[4]
    else {
        panic!()
    };
    assert!(is_error);
    assert!(preview.contains("push diblokir"), "{preview}");

    let AgentEvent::Usage(u) = &events[10] else {
        panic!()
    };
    assert_eq!(u.output_tokens, 911);
    assert_eq!(u.total_input(), 4 + 18_329 + 40_814);
    let AgentEvent::Done {
        is_error,
        num_turns,
        denials,
        ..
    } = &events[11]
    else {
        panic!()
    };
    assert!(!is_error);
    assert_eq!((*num_turns, *denials), (4, 2));
}

#[test]
fn usage_hanya_dari_event_result() {
    let usages = fixture()
        .lines()
        .flat_map(parse_line)
        .filter(|e| matches!(e, AgentEvent::Usage(_)))
        .count();
    assert_eq!(usages, 1);
}

#[test]
fn baris_rusak_di_tengah_dilewati_dan_stream_berlanjut() {
    let text = fixture();
    let mut lines: Vec<&str> = text.lines().collect();
    lines.insert(3, "{\"type\": \"assistant\", rusak");
    lines.insert(5, "bukan json sama sekali");
    lines.insert(6, "");
    let events: Vec<AgentEvent> = lines.into_iter().flat_map(parse_line).collect();
    assert!(matches!(
        events.first(),
        Some(AgentEvent::SessionStarted { .. })
    ));
    assert!(matches!(events.last(), Some(AgentEvent::Done { .. })));
    assert_eq!(events.len(), 12);
}

#[test]
fn satu_pesan_bisa_menghasilkan_beberapa_event() {
    let line = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Saya cek dulu"},{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"src/a.rs"}},{"type":"tool_use","id":"t2","name":"Grep","input":{"pattern":"fn main"}}]}}"#;
    let events = parse_line(line);
    assert_eq!(
        events.iter().map(kind).collect::<Vec<_>>(),
        ["assistant_message", "tool_use", "tool_use"]
    );
    let AgentEvent::ToolUse { preview, .. } = &events[2] else {
        panic!()
    };
    assert_eq!(preview, "fn main");
}
