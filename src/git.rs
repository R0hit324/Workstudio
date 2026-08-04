use std::path::Path;
use std::process::Command;

struct Out {
    ok: bool,
    out: String,
    err: String,
}

fn git(dir: &Path, args: &[&str]) -> Out {
    match Command::new("git").arg("-C").arg(dir).args(args).output() {
        Ok(o) => Out {
            ok: o.status.success(),
            out: String::from_utf8_lossy(&o.stdout).trim().to_string(),
            err: String::from_utf8_lossy(&o.stderr).trim().to_string(),
        },
        Err(e) => Out {
            ok: false,
            out: String::new(),
            err: e.to_string(),
        },
    }
}

/// True when `dir` is inside a git repository.
pub fn is_repo(dir: &Path) -> bool {
    git(dir, &["rev-parse", "--is-inside-work-tree"]).ok
}

/// Current branch name (or "HEAD" when detached).
pub fn branch(dir: &Path) -> String {
    let out = git(dir, &["rev-parse", "--abbrev-ref", "HEAD"]);
    if out.ok {
        out.out
    } else {
        "HEAD".into()
    }
}

/// Last N commit subjects, newest first.
pub fn log(dir: &Path, n: usize) -> Vec<String> {
    let out = git(
        dir,
        &["log", "--oneline", "-n", &n.to_string(), "--pretty=format:%h %s"],
    );
    if !out.ok {
        return Vec::new();
    }
    out.out.lines().map(|s| s.to_string()).collect()
}

/// Stage everything and commit as `author`. Idempotent when nothing changed
/// (returns Ok). Initializes a repo if `dir` is not one yet.
pub fn commit(dir: &Path, message: &str, author: &str) -> Result<String, String> {
    if !dir.exists() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    if !is_repo(dir) && !git(dir, &["init"]).ok {
        return Err("git init failed".into());
    }
    // Ensure a local identity exists so commits don't fail when the user has no
    // global git config.
    ensure_identity(dir);
    if !git(dir, &["add", "-A"]).ok {
        return Err("git add failed".into());
    }
    let email = format!("{author}@nexus.local");
    let author_arg = format!("{author} <{email}>");
    let out = git(dir, &["commit", "-m", message, "--author", &author_arg]);
    if out.ok {
        Ok(out.out)
    } else if out.err.contains("nothing to commit") {
        Ok("nothing to commit".into())
    } else {
        Err(out.err)
    }
}

fn ensure_identity(dir: &Path) {
    if !git(dir, &["config", "user.name"]).ok {
        let _ = git(dir, &["config", "user.name", "Nexus User"]);
    }
    if !git(dir, &["config", "user.email"]).ok {
        let _ = git(dir, &["config", "user.email", "nexus@local"]);
    }
}

/// Latest commit count (0 when there are no commits yet).
pub fn commit_count(dir: &Path) -> usize {
    let out = git(dir, &["rev-list", "--count", "HEAD"]);
    if out.ok {
        out.out.parse().unwrap_or(0)
    } else {
        0
    }
}
