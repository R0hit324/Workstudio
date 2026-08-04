use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tokio::sync::mpsc;

pub const PISTON_URL: &str = "https://emkc.org/api/v2/piston/execute";

#[derive(Serialize)]
struct PistonFile {
    name: String,
    content: String,
}

#[derive(Serialize)]
struct PistonRequest {
    language: String,
    version: String,
    files: Vec<PistonFile>,
    stdin: String,
}

#[derive(Deserialize)]
struct PistonResponse {
    run: Option<PistonRun>,
    compile: Option<PistonCompile>,
    message: Option<String>,
}

#[derive(Deserialize)]
struct PistonRun {
    stdout: String,
    stderr: String,
    code: Option<i64>,
}

#[derive(Deserialize)]
struct PistonCompile {
    stderr: String,
    output: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OutputKind {
    Stdout,
    Stderr,
    Info,
}

pub enum RunnerEvent {
    Started,
    Line(String, OutputKind),
    Done { ok: bool, exit_code: Option<i64> },
    Failed(String),
}

struct RunOut {
    ok: bool,
    exit_code: Option<i64>,
    lines: Vec<(String, OutputKind)>,
}

async fn execute(
    lang: &str,
    version: &str,
    filename: &str,
    code: String,
) -> Result<RunOut, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;
    let req = PistonRequest {
        language: lang.to_string(),
        version: version.to_string(),
        files: vec![PistonFile {
            name: filename.to_string(),
            content: code,
        }],
        stdin: String::new(),
    };
    let res = client
        .post(PISTON_URL)
        .header("Content-Type", "application/json")
        .json(&req)
        .send()
        .await
        .map_err(|e| format!("network error: {e} (check internet — uses Piston API)"))?;
    let data: PistonResponse = res
        .json()
        .await
        .map_err(|e| format!("bad response from Piston: {e}"))?;

    if let Some(msg) = data.message {
        return Err(format!(
            "Piston API error: {msg} (hint: host your own Piston instance, or use Python locally)"
        ));
    }

    let mut ok = true;
    let mut lines = Vec::new();
    if let Some(c) = &data.compile {
        if !c.stderr.trim().is_empty() {
            ok = false;
        }
        split_lines(&c.stderr, &mut lines, OutputKind::Stderr);
        split_lines(&c.output, &mut lines, OutputKind::Stdout);
    }
    let mut exit_code = None;
    if let Some(r) = &data.run {
        if !r.stderr.trim().is_empty() {
            ok = false;
        }
        split_lines(&r.stdout, &mut lines, OutputKind::Stdout);
        split_lines(&r.stderr, &mut lines, OutputKind::Stderr);
        exit_code = r.code;
    }
    if lines.is_empty() {
        lines.push(("(no output)".into(), OutputKind::Info));
    }
    Ok(RunOut {
        ok,
        exit_code,
        lines,
    })
}

async fn execute_local_python(code: String) -> Result<RunOut, String> {
    let out = Command::new("python3")
        .arg("-u")
        .arg("-c")
        .arg(code)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("failed to start python3: {e}"))?;
    let mut ok = true;
    let mut lines = Vec::new();
    if !out.stderr.is_empty() {
        ok = false;
    }
    split_lines(
        &String::from_utf8_lossy(&out.stdout),
        &mut lines,
        OutputKind::Stdout,
    );
    split_lines(
        &String::from_utf8_lossy(&out.stderr),
        &mut lines,
        OutputKind::Stderr,
    );
    if let Some(code) = out.status.code() {
        if code != 0 {
            ok = false;
        }
    }
    if lines.is_empty() {
        lines.push(("(no output)".into(), OutputKind::Info));
    }
    Ok(RunOut {
        ok,
        exit_code: out.status.code().map(|c| c as i64),
        lines,
    })
}

fn split_lines(text: &str, out: &mut Vec<(String, OutputKind)>, kind: OutputKind) {
    for l in text.split('\n').filter(|l| !l.is_empty()) {
        out.push((l.to_string(), kind));
    }
}

pub fn spawn_run(
    tx: mpsc::UnboundedSender<RunnerEvent>,
    lang: &str,
    version: &str,
    filename: String,
    code: String,
) -> tokio::task::JoinHandle<()> {
    let _ = tx.send(RunnerEvent::Started);
    let lang = lang.to_string();
    let version = version.to_string();
    tokio::spawn(async move {
        let out = if lang == "python" && which_local("python3") {
            execute_local_python(code).await
        } else {
            execute(&lang, &version, &filename, code).await
        };
        match out {
            Ok(o) => {
                for (line, kind) in o.lines {
                    let _ = tx.send(RunnerEvent::Line(line, kind));
                }
                let _ = tx.send(RunnerEvent::Done {
                    ok: o.ok,
                    exit_code: o.exit_code,
                });
            }
            Err(e) => {
                let _ = tx.send(RunnerEvent::Failed(e));
            }
        }
    })
}

fn which_local(bin: &str) -> bool {
    std::process::Command::new(bin)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn local_python_stdout_and_stderr() {
        let code = "print('hello')\nprint(2+2)\nraise ValueError('boom')";
        let out = execute_local_python(code.to_string()).await.expect("runs");
        assert!(!out.ok);
        assert_eq!(out.exit_code, Some(1));
        let stdout: Vec<&str> = out
            .lines
            .iter()
            .filter(|(_, k)| *k == OutputKind::Stdout)
            .map(|(t, _)| t.as_str())
            .collect();
        assert_eq!(stdout, vec!["hello", "4"]);
        assert!(out
            .lines
            .iter()
            .any(|(t, k)| *k == OutputKind::Stderr && t.contains("ValueError: boom")));
    }

    #[tokio::test]
    async fn local_python_exit_code() {
        let out = execute_local_python("import sys\nsys.exit(3)".into())
            .await
            .expect("runs");
        assert_eq!(out.exit_code, Some(3));
        assert!(!out.ok);
    }

    #[tokio::test]
    async fn spawn_run_delivers_lines_in_order() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        spawn_run(
            tx,
            "python",
            "3.10.0",
            "main.py".into(),
            "print('a')\nprint('b')".into(),
        );
        let mut got = Vec::new();
        while let Some(ev) = rx.recv().await {
            match ev {
                RunnerEvent::Line(t, _) => got.push(t),
                RunnerEvent::Done { ok, .. } => {
                    assert!(ok);
                    break;
                }
                _ => {}
            }
        }
        assert_eq!(got, vec!["a", "b"]);
    }

    #[test]
    fn split_lines_skips_empty() {
        let mut v = Vec::new();
        split_lines("a\n\nb\n", &mut v, OutputKind::Stdout);
        assert_eq!(v, vec![("a".to_string(), OutputKind::Stdout), ("b".to_string(), OutputKind::Stdout)]);
    }
}
