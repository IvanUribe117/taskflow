//! El adaptador completo contra un `claude` **falso**: un script de shell que guarda lo que
//! recibe y responde un JSON fijo. Prueba el proceso real (argumentos, entrada estándar,
//! carpeta, tiempo límite) sin gastar ni una llamada a la IA.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use adapter_claude_code::{ClaudeCodeConfig, ClaudeCodeSummarizer};
use application::dto::BuiltContext;
use application::ports::{
    AiError, Complexity, PlanInput, ProjectBrief, Summarizer, SummaryInput, TaskBrief,
};
use tempfile::TempDir;

const SUMMARY: &str = r#"{"type":"result","subtype":"success","is_error":false,"result":"","structured_output":{"what_is_requested":"Corregir el redondeo","suggested_steps":["Reproducir"],"open_questions":[],"estimated_complexity":"medium","agent_prompt":"Corrige invoicing"}}"#;

/// Escribe un script `claude` ejecutable en una carpeta temporal. Guarda sus argumentos,
/// su entrada estándar y su carpeta de trabajo en archivos junto a él.
fn fake_claude(body: &str) -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("claude");
    let content = format!(
        "#!/bin/sh\n\
         here=\"$(dirname \"$0\")\"\n\
         printf '%s\\n' \"$@\" > \"$here/args.txt\"\n\
         cat > \"$here/stdin.txt\"\n\
         pwd -P > \"$here/cwd.txt\"\n\
         {body}\n"
    );
    fs::write(&script, content).unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    (dir, script)
}

fn summarizer(program: &Path, timeout: Duration) -> ClaudeCodeSummarizer {
    ClaudeCodeSummarizer::new(ClaudeCodeConfig {
        program: program.to_path_buf(),
        timeout,
        ..ClaudeCodeConfig::default()
    })
}

fn input(repo: Option<PathBuf>) -> SummaryInput {
    SummaryInput {
        task: TaskBrief {
            id: "86b2x1abc".into(),
            title: "Fix invoice rounding".into(),
            description: None,
            status: "to do".into(),
            priority: "high".into(),
            due: None,
            url: "https://example.test/t/86b2x1abc".into(),
            tags: vec![],
            subtasks: vec![],
            checklists: vec![],
        },
        comments: vec![],
        project: Some(ProjectBrief {
            name: "ACME".into(),
            repo_path: repo,
        }),
        context: BuiltContext::default(),
        note: None,
    }
}

fn read(dir: &TempDir, file: &str) -> String {
    fs::read_to_string(dir.path().join(file)).unwrap()
}

#[tokio::test]
async fn summarizes_through_the_process() {
    let (dir, program) = fake_claude(&format!("echo '{SUMMARY}'"));

    let summary = summarizer(&program, Duration::from_secs(10))
        .summarize_task(&input(None))
        .await
        .unwrap();

    assert_eq!(summary.what_is_requested, "Corregir el redondeo");
    assert_eq!(summary.estimated_complexity, Complexity::Medium);
    assert!(read(&dir, "args.txt").contains("--json-schema"));
    assert!(read(&dir, "stdin.txt").starts_with("<task>"));
}

#[tokio::test]
async fn runs_inside_the_repository_when_it_exists() {
    let (dir, program) = fake_claude(&format!("echo '{SUMMARY}'"));
    let repo = tempfile::tempdir().unwrap();

    summarizer(&program, Duration::from_secs(10))
        .summarize_task(&input(Some(repo.path().to_path_buf())))
        .await
        .unwrap();

    let cwd = PathBuf::from(read(&dir, "cwd.txt").trim());
    assert_eq!(cwd, repo.path().canonicalize().unwrap());
    assert!(read(&dir, "args.txt").contains("Read,Grep,Glob"));
}

#[tokio::test]
async fn a_missing_repository_is_ignored() {
    let (dir, program) = fake_claude(&format!("echo '{SUMMARY}'"));

    summarizer(&program, Duration::from_secs(10))
        .summarize_task(&input(Some(PathBuf::from("/no/existe"))))
        .await
        .unwrap();

    assert!(!read(&dir, "args.txt").contains("Read,Grep,Glob"));
}

#[tokio::test]
async fn plans_through_the_process() {
    let (_dir, program) =
        fake_claude(r#"echo '{"is_error":false,"structured_output":{"plan":"1. Fix"}}'"#);
    let plan_input = PlanInput {
        date: "2026-10-14".parse().unwrap(),
        tasks: vec![],
        context: BuiltContext::default(),
    };

    let plan = summarizer(&program, Duration::from_secs(10))
        .plan_day(&plan_input)
        .await
        .unwrap();

    assert_eq!(plan, "1. Fix");
}

#[tokio::test]
async fn errors_printed_by_claude_are_failures() {
    let (_dir, program) =
        fake_claude(r#"echo '{"is_error":true,"result":"Not logged in"}'; exit 1"#);

    let error = summarizer(&program, Duration::from_secs(10))
        .summarize_task(&input(None))
        .await
        .unwrap_err();

    assert!(matches!(error, AiError::Failed(m) if m.contains("Not logged in")));
}

#[tokio::test]
async fn a_crash_without_output_reports_stderr() {
    let (_dir, program) = fake_claude("echo 'unknown option' >&2; exit 2");

    let error = summarizer(&program, Duration::from_secs(10))
        .summarize_task(&input(None))
        .await
        .unwrap_err();

    assert!(matches!(error, AiError::Failed(m) if m.contains("unknown option")));
}

#[tokio::test]
async fn slow_answers_time_out() {
    let (_dir, program) = fake_claude("sleep 5");

    let error = summarizer(&program, Duration::from_millis(300))
        .summarize_task(&input(None))
        .await
        .unwrap_err();

    assert!(matches!(error, AiError::Timeout(_)));
}

#[tokio::test]
async fn a_missing_program_is_unavailable() {
    let error = summarizer(Path::new("/no/existe/claude"), Duration::from_secs(1))
        .summarize_task(&input(None))
        .await
        .unwrap_err();

    assert!(matches!(error, AiError::Unavailable(_)));
}

#[tokio::test]
async fn summarize_work_resumes_the_agent_conversation_inside_the_repo() {
    const WORK: &str = r#"{"type":"result","subtype":"success","is_error":false,"result":"","structured_output":{"summary":"Hecho","changes":["login.html"],"verification":[],"pending":[]}}"#;
    let (dir, program) = fake_claude(&format!("echo '{WORK}'"));
    let repo = tempfile::tempdir().unwrap();
    let base = input(Some(repo.path().to_path_buf()));
    let work_input = application::ports::WorkInput {
        task: base.task,
        project: base.project,
        plan: "## Plan".into(),
        changes: None,
        agent_session: Some("1b4e28ba-2fa1-41d2-883f-0016d3cca427".into()),
        context: BuiltContext::default(),
    };

    let work = summarizer(&program, Duration::from_secs(10))
        .summarize_work(&work_input)
        .await
        .unwrap();

    assert_eq!(work.summary, "Hecho");
    let args = read(&dir, "args.txt");
    assert!(
        args.contains("--resume\n1b4e28ba-2fa1-41d2-883f-0016d3cca427\n"),
        "{args}"
    );
    assert!(read(&dir, "stdin.txt").starts_with("Taskflow: terminaste de trabajar"));
    let cwd = PathBuf::from(read(&dir, "cwd.txt").trim());
    assert_eq!(cwd, repo.path().canonicalize().unwrap());
}
