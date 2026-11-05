use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::Duration,
};
use tempfile::TempDir;

fn git(path: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .expect("git");
    assert!(
        output.status.success(),
        "git {:?}: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_output(path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .expect("git");
    assert!(
        output.status.success(),
        "git {:?}: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn fixture(path: &Path) {
    fs::create_dir_all(path).unwrap();
    git(path, &["init", "-q", "-b", "main"]);
    git(path, &["config", "user.email", "xs@example.invalid"]);
    git(path, &["config", "user.name", "XS test"]);
    fs::write(
        path.join("needle.txt"),
        "before\nneedle in committed file\nafter\n",
    )
    .unwrap();
    fs::write(path.join("multi.txt"), "needle first\nneedle second\n").unwrap();
    fs::create_dir_all(path.join("src")).unwrap();
    fs::write(
        path.join("src/code.rs"),
        "fn main() { /* indexed path */ }\n",
    )
    .unwrap();
    fs::write(
        path.join("long.txt"),
        format!("needle{}\n", "x".repeat(5_000)),
    )
    .unwrap();
    fs::create_dir_all(path.join("target")).unwrap();
    fs::create_dir_all(path.join("assets")).unwrap();
    fs::write(path.join("target/generated.txt"), "needle generated text\n").unwrap();
    fs::write(
        path.join("assets/readable.png"),
        "needle readable binary-extension text\n",
    )
    .unwrap();
    fs::write(path.join("contains-nul.txt"), b"needle\0binary").unwrap();
    fs::write(path.join("invalid.txt"), [0xFF, 0xFE, b'n']).unwrap();
    fs::write(path.join("image.png"), "needle binary extension text\n").unwrap();
    git(path, &["add", "."]);
    git(path, &["commit", "-q", "-m", "fixture"]);
}

fn run(db: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xs"))
        .arg("--db")
        .arg(db)
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn xs_help_and_binary_name() {
    let output = Command::new(env!("CARGO_BIN_EXE_xs"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("Usage: xs"), "{text}");
    for command in ["index", "search", "grep", "files", "read"] {
        assert!(
            text.contains(command),
            "missing {command} command in help: {text}"
        );
    }
}

#[test]
fn indexed_search_and_worktree_provenance_are_json() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("root");
    let repo = root.join("org").join("demo");
    fixture(&repo);
    fs::write(
        repo.join("oversize.txt"),
        format!("needle{}", "z".repeat(6 * 1024 * 1024)),
    )
    .unwrap();
    git(&repo, &["add", "oversize.txt"]);
    git(&repo, &["commit", "-q", "-m", "oversize fixture"]);
    let db = temp.path().join("meta.db");
    assert!(
        run(&db, &["roots", "add", root.to_str().unwrap()])
            .status
            .success()
    );
    assert!(run(&db, &["scan"]).status.success());
    let source_before_index = (
        git_output(&repo, &["rev-parse", "HEAD^{tree}"]),
        git_output(&repo, &["status", "--porcelain", "--untracked-files=all"]),
    );
    let update = run(&db, &["index", "update", "--repo", "org/demo", "--json"]);
    assert!(
        update.status.success(),
        "{}",
        String::from_utf8_lossy(&update.stderr)
    );
    let update_json: serde_json::Value = serde_json::from_slice(&update.stdout).unwrap();
    for key in ["summary", "items", "errors"] {
        assert!(update_json.get(key).is_some(), "missing {key}");
    }
    for key in ["requested", "updated", "skipped", "failed", "complete"] {
        assert!(
            update_json["summary"].get(key).is_some(),
            "missing summary.{key}"
        );
    }
    for key in [
        "repo",
        "rootId",
        "outcome",
        "indexedCommit",
        "textFiles",
        "textBytes",
        "error",
    ] {
        assert!(
            update_json["items"][0].get(key).is_some(),
            "missing update item {key}"
        );
    }
    let rebuild = run(&db, &["index", "rebuild", "--repo", "org/demo", "--json"]);
    assert!(
        rebuild.status.success(),
        "{}",
        String::from_utf8_lossy(&rebuild.stderr)
    );
    let rebuild_json: serde_json::Value = serde_json::from_slice(&rebuild.stdout).unwrap();
    assert!(rebuild_json["items"][0]["indexedCommit"].is_string());
    let status = run(&db, &["index", "status", "--repo", "org/demo", "--json"]);
    assert!(status.status.success());
    let status: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    for key in ["items", "errors"] {
        assert!(status.get(key).is_some(), "missing {key}");
    }
    for key in [
        "repo",
        "rootId",
        "currentHead",
        "indexedCommit",
        "state",
        "textFiles",
        "textBytes",
        "indexBytes",
        "filterVersion",
        "updatedAt",
        "error",
    ] {
        assert!(
            status["items"][0].get(key).is_some(),
            "missing index status item {key}"
        );
    }
    assert_eq!(status["items"][0]["state"], "ready");
    let source_after_index = (
        git_output(&repo, &["rev-parse", "HEAD^{tree}"]),
        git_output(&repo, &["status", "--porcelain", "--untracked-files=all"]),
    );
    assert_eq!(source_before_index, source_after_index);
    let searched = run(&db, &["search", "needle", "--repo", "org/demo", "--json"]);
    assert!(
        searched.status.success(),
        "{}",
        String::from_utf8_lossy(&searched.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&searched.stdout).unwrap();
    assert!(
        value["results"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["text"].as_str().unwrap().contains("needle"))
    );
    let indexed_head = git_output(&repo, &["rev-parse", "HEAD"]).trim().to_owned();
    let needle_hit = value["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|result| result["path"] == "needle.txt")
        .unwrap();
    assert_eq!(needle_hit["source"], "index");
    assert_eq!(needle_hit["revision"], indexed_head);
    for key in [
        "repo",
        "rootId",
        "path",
        "line",
        "column",
        "text",
        "source",
        "baseHead",
        "fileState",
        "truncated",
        "context",
    ] {
        assert!(needle_hit.get(key).is_some(), "missing search result {key}");
    }
    let excluded_body_paths = [
        "image.png",
        "assets/readable.png",
        "target/generated.txt",
        "contains-nul.txt",
        "invalid.txt",
        "oversize.txt",
    ];
    for excluded in excluded_body_paths {
        assert!(
            !value["results"]
                .as_array()
                .unwrap()
                .iter()
                .any(|result| result["path"] == excluded),
            "excluded body unexpectedly matched: {excluded}"
        );
    }
    let long_line = value["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|result| result["path"] == "long.txt")
        .unwrap();
    assert_eq!(long_line["truncated"], true);
    assert_eq!(long_line["text"].as_str().unwrap().chars().count(), 4096);
    let capped_files = run(
        &db,
        &[
            "files",
            "**/*",
            "--repo",
            "org/demo",
            "--max-results",
            "1",
            "--json",
        ],
    );
    let capped_files: serde_json::Value = serde_json::from_slice(&capped_files.stdout).unwrap();
    assert_eq!(capped_files["summary"]["truncated"], true);
    assert_eq!(capped_files["summary"]["complete"], false);
    let excluded = run(
        &db,
        &[
            "search",
            "needle",
            "--repo",
            "org/demo",
            "--glob",
            "*.txt",
            "--glob",
            "!long.txt",
            "--json",
        ],
    );
    let excluded: serde_json::Value = serde_json::from_slice(&excluded.stdout).unwrap();
    assert!(
        !excluded["results"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["path"] == "long.txt")
    );
    assert!(
        excluded["results"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["path"] == "needle.txt")
    );
    let typed = run(
        &db,
        &[
            "files", "**/*", "--repo", "org/demo", "--type", "rust", "--json",
        ],
    );
    let typed: serde_json::Value = serde_json::from_slice(&typed.stdout).unwrap();
    assert_eq!(typed["summary"]["files"], 1);
    let paths = run(&db, &["files", "**/*.rs", "--repo", "org/demo", "--json"]);
    assert!(
        paths.status.success(),
        "{}",
        String::from_utf8_lossy(&paths.stderr)
    );
    let paths: serde_json::Value = serde_json::from_slice(&paths.stdout).unwrap();
    let expected_head = git_output(&repo, &["rev-parse", "HEAD"]).trim().to_owned();
    let code_path = paths["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|result| result["path"] == "src/code.rs")
        .unwrap();
    assert_eq!(code_path["revision"], expected_head);
    let all_paths = run(&db, &["files", "**/*", "--repo", "org/demo", "--json"]);
    let all_paths: serde_json::Value = serde_json::from_slice(&all_paths.stdout).unwrap();
    assert!(
        all_paths["results"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["path"] == "image.png")
    );
    for expected_path in excluded_body_paths {
        assert!(
            all_paths["results"]
                .as_array()
                .unwrap()
                .iter()
                .any(|result| result["path"] == expected_path),
            "missing path-only file {expected_path}"
        );
    }
    assert!(
        run(&db, &["tag", "org/demo", "searchable"])
            .status
            .success()
    );
    let tagged = run(
        &db,
        &[
            "search",
            "needle",
            "--repo",
            "org/demo",
            "--tag",
            "searchable",
            "--json",
        ],
    );
    assert!(tagged.status.success());
    let tagged: serde_json::Value = serde_json::from_slice(&tagged.stdout).unwrap();
    assert!(tagged["summary"]["matches"].as_u64().unwrap() > 0);
    let regex_match = run(
        &db,
        &[
            "search",
            "needle.*committed",
            "--repo",
            "org/demo",
            "--json",
        ],
    );
    let regex_match: serde_json::Value = serde_json::from_slice(&regex_match.stdout).unwrap();
    assert!(
        regex_match["results"]
            .as_array()
            .unwrap()
            .iter()
            .any(|result| result["path"] == "needle.txt")
    );
    let word_match = run(
        &db,
        &[
            "search", "NEEDLE", "--repo", "org/demo", "-F", "-i", "-w", "--json",
        ],
    );
    assert!(word_match.status.success());
    let word_match: serde_json::Value = serde_json::from_slice(&word_match.stdout).unwrap();
    assert!(word_match["summary"]["matches"].as_u64().unwrap() > 0);
    let files_with_matches = run(
        &db,
        &["search", "needle", "--repo", "org/demo", "-l", "--json"],
    );
    let files_with_matches: serde_json::Value =
        serde_json::from_slice(&files_with_matches.stdout).unwrap();
    let matched_paths: Vec<_> = files_with_matches["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|result| result["path"].as_str().unwrap())
        .collect();
    let unique_paths: std::collections::HashSet<_> = matched_paths.iter().copied().collect();
    assert_eq!(matched_paths.len(), unique_paths.len());
    let human_file_only = run(&db, &["search", "needle", "--repo", "org/demo", "-l"]);
    assert!(human_file_only.status.success());
    let human_file_only = String::from_utf8_lossy(&human_file_only.stdout);
    assert!(human_file_only.contains("needle.txt"));
    assert!(!human_file_only.contains(":2:"));
    let limited = run(
        &db,
        &[
            "search",
            "needle",
            "--repo",
            "org/demo",
            "--max-count",
            "1",
            "--json",
        ],
    );
    let limited: serde_json::Value = serde_json::from_slice(&limited.stdout).unwrap();
    let multi_hits: Vec<_> = limited["results"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|result| result["path"] == "multi.txt")
        .collect();
    assert_eq!(multi_hits.len(), 1);
    assert_eq!(multi_hits[0]["line"], 1);
    let counted = run(
        &db,
        &[
            "search",
            "needle in committed file",
            "--repo",
            "org/demo",
            "--count",
            "--json",
        ],
    );
    let counted: serde_json::Value = serde_json::from_slice(&counted.stdout).unwrap();
    assert_eq!(counted["results"].as_array().unwrap().len(), 1);
    assert_eq!(counted["results"][0]["count"], 1);
    let contextual = run(
        &db,
        &[
            "search",
            "needle in committed file",
            "--repo",
            "org/demo",
            "-C",
            "1",
            "--json",
        ],
    );
    let contextual: serde_json::Value = serde_json::from_slice(&contextual.stdout).unwrap();
    assert_eq!(
        contextual["results"][0]["context"]["before"][0]["text"],
        "before"
    );
    assert_eq!(
        contextual["results"][0]["context"]["after"][0]["text"],
        "after"
    );
    let human = run(
        &db,
        &[
            "search",
            "needle in committed file",
            "--repo",
            "org/demo",
            "-C",
            "1",
        ],
    );
    assert!(human.status.success());
    let human = String::from_utf8_lossy(&human.stdout);
    assert!(human.contains("needle.txt:1- before"));
    assert!(human.contains("needle.txt:3- after"));
    let human_files = run(&db, &["files", "**/*.rs", "--repo", "org/demo"]);
    assert!(human_files.status.success());
    assert!(String::from_utf8_lossy(&human_files.stdout).contains("src/code.rs"));
    fs::write(repo.join("dirty.txt"), "working tree needle\n").unwrap();
    fs::write(repo.join("needle.txt"), "dirty needle replacement\n").unwrap();
    git(&repo, &["add", "needle.txt"]);
    let source_state_before = (
        git_output(&repo, &["rev-parse", "HEAD^{tree}"]),
        git_output(&repo, &["status", "--porcelain", "--untracked-files=all"]),
    );
    let indexed_only = run(
        &db,
        &[
            "search",
            "working tree needle",
            "--repo",
            "org/demo",
            "--json",
        ],
    );
    assert!(indexed_only.status.success());
    let indexed_only: serde_json::Value = serde_json::from_slice(&indexed_only.stdout).unwrap();
    assert!(indexed_only["results"].as_array().unwrap().is_empty());
    let grep = run(&db, &["grep", "needle", "--repo", "org/demo", "--json"]);
    assert!(
        grep.status.success(),
        "{}",
        String::from_utf8_lossy(&grep.stderr)
    );
    let progress = String::from_utf8_lossy(&grep.stderr);
    assert!(progress.contains("grep scope: repo selector(s): org/demo; repositories=1"));
    assert!(progress.contains("grep scanning org/demo"));
    assert!(progress.contains("grep complete:"));
    assert!(progress.contains("skipped="));
    assert!(progress.contains("elapsedMs="));
    let value: serde_json::Value = serde_json::from_slice(&grep.stdout).unwrap();
    assert!(
        value["results"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["path"] == "dirty.txt" && r["fileState"] == "untracked")
    );
    assert!(
        value["results"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["path"] == "needle.txt" && r["fileState"] == "modified")
    );
    for excluded in excluded_body_paths {
        assert!(
            !value["results"]
                .as_array()
                .unwrap()
                .iter()
                .any(|result| result["path"] == excluded)
        );
    }
    let head_read = run(&db, &["read", "org/demo", "needle.txt", "--json"]);
    assert!(
        head_read.status.success(),
        "{}",
        String::from_utf8_lossy(&head_read.stderr)
    );
    let head_read: serde_json::Value = serde_json::from_slice(&head_read.stdout).unwrap();
    assert!(head_read["result"].is_object());
    assert!(head_read["errors"].as_array().unwrap().is_empty());
    for key in [
        "repo",
        "rootId",
        "revision",
        "path",
        "startLine",
        "endLine",
        "content",
        "truncated",
        "source",
    ] {
        assert!(
            head_read["result"].get(key).is_some(),
            "missing read result {key}"
        );
    }
    assert!(
        head_read["result"]["content"]
            .as_str()
            .unwrap()
            .contains("needle in committed file")
    );
    assert!(
        !head_read["result"]["content"]
            .as_str()
            .unwrap()
            .contains("dirty needle replacement")
    );
    for (unsafe_or_invalid_path, expected_message) in [
        ("image.png", "binary file extension"),
        ("target/generated.txt", "generated path"),
        ("oversize.txt", "5 MiB"),
        ("contains-nul.txt", "NUL"),
        ("invalid.txt", "UTF-8"),
        ("missing.txt", "file not found"),
    ] {
        let output = run(&db, &["read", "org/demo", unsafe_or_invalid_path, "--json"]);
        assert!(
            !output.status.success(),
            "expected read error for {unsafe_or_invalid_path}"
        );
        let output: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(
            output["errors"][0]["message"]
                .as_str()
                .unwrap()
                .contains(expected_message),
            "unexpected read error for {unsafe_or_invalid_path}: {output}"
        );
    }
    let ref_read = run(
        &db,
        &["read", "org/demo", "needle.txt", "--ref", "HEAD", "--json"],
    );
    assert!(
        ref_read.status.success(),
        "{}",
        String::from_utf8_lossy(&ref_read.stderr)
    );
    let ref_read: serde_json::Value = serde_json::from_slice(&ref_read.stdout).unwrap();
    assert!(ref_read["errors"].as_array().unwrap().is_empty());
    assert_eq!(ref_read["result"]["source"], "git-ref");
    assert!(
        ref_read["result"]["content"]
            .as_str()
            .unwrap()
            .contains("needle in committed file")
    );
    let grep_ref = run(
        &db,
        &[
            "grep", "needle", "--repo", "org/demo", "--ref", "HEAD", "--json",
        ],
    );
    assert!(
        grep_ref.status.success(),
        "{}",
        String::from_utf8_lossy(&grep_ref.stderr)
    );
    let grep_ref: serde_json::Value = serde_json::from_slice(&grep_ref.stdout).unwrap();
    let expected_head = git_output(&repo, &["rev-parse", "HEAD"]).trim().to_owned();
    assert!(
        grep_ref["results"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| { r["source"] == "git-ref" && r["revision"] == expected_head })
    );
    for excluded in excluded_body_paths {
        assert!(
            !grep_ref["results"]
                .as_array()
                .unwrap()
                .iter()
                .any(|result| result["path"] == excluded)
        );
    }
    let source_state_after = (
        git_output(&repo, &["rev-parse", "HEAD^{tree}"]),
        git_output(&repo, &["status", "--porcelain", "--untracked-files=all"]),
    );
    assert_eq!(source_state_before, source_state_after);
}

#[test]
fn head_tree_update_removes_deleted_paths_and_duplicate_repos_aggregate() {
    let temp = TempDir::new().unwrap();
    let root_a = temp.path().join("root-a");
    let root_b = temp.path().join("root-b");
    let repo_a = root_a.join("org").join("demo");
    let repo_b = root_b.join("org").join("demo");
    fixture(&repo_a);
    fixture(&repo_b);
    let initial_head_a = git_output(&repo_a, &["rev-parse", "HEAD"])
        .trim()
        .to_owned();
    let db = temp.path().join("meta.db");
    let add_a = run(&db, &["roots", "add", root_a.to_str().unwrap()]);
    assert!(add_a.status.success());
    let root_a_id: i64 = String::from_utf8_lossy(&add_a.stdout)
        .split_whitespace()
        .last()
        .unwrap()
        .parse()
        .unwrap();
    let add_b = run(&db, &["roots", "add", root_b.to_str().unwrap()]);
    assert!(add_b.status.success());
    assert!(run(&db, &["scan"]).status.success());

    let ambiguous = run(&db, &["index", "update", "--repo", "org/demo", "--json"]);
    assert!(!ambiguous.status.success());
    let ambiguous: serde_json::Value = serde_json::from_slice(&ambiguous.stdout).unwrap();
    assert_eq!(ambiguous["summary"]["complete"], false);
    assert!(!ambiguous["errors"].as_array().unwrap().is_empty());
    let updated = run(&db, &["index", "update", "--all-roots", "--json"]);
    assert!(
        updated.status.success(),
        "{}",
        String::from_utf8_lossy(&updated.stderr)
    );
    let aggregate = run(&db, &["search", "needle", "--repo", "org/demo", "--json"]);
    let aggregate: serde_json::Value = serde_json::from_slice(&aggregate.stdout).unwrap();
    assert_eq!(aggregate["summary"]["repos"], 2);
    let result_root_ids: std::collections::HashSet<_> = aggregate["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|result| result["rootId"].as_i64().unwrap())
        .collect();
    assert_eq!(result_root_ids.len(), 2);
    let ambiguous_read = run(&db, &["read", "org/demo", "needle.txt", "--json"]);
    assert!(!ambiguous_read.status.success());
    let rooted_read = run(
        &db,
        &[
            "read",
            "org/demo",
            "needle.txt",
            "--root",
            &root_a_id.to_string(),
            "--json",
        ],
    );
    assert!(rooted_read.status.success());

    fs::write(repo_a.join("added.txt"), "new head marker\n").unwrap();
    git(&repo_a, &["add", "-A"]);
    git(&repo_a, &["commit", "-q", "-m", "add marker file"]);
    let root_selector = root_a_id.to_string();
    let stale = run(
        &db,
        &[
            "search",
            "needle",
            "--repo",
            "org/demo",
            "--root",
            &root_selector,
            "--json",
        ],
    );
    assert!(!stale.status.success());
    let stale: serde_json::Value = serde_json::from_slice(&stale.stdout).unwrap();
    assert_eq!(stale["summary"]["complete"], false);
    assert_eq!(stale["errors"][0]["code"], "index_stale");
    assert!(
        stale["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("xs index update")
    );
    let update_added = run(
        &db,
        &[
            "index",
            "update",
            "--repo",
            "org/demo",
            "--root",
            &root_selector,
            "--json",
        ],
    );
    assert!(update_added.status.success());
    let added = run(
        &db,
        &[
            "search",
            "marker",
            "--repo",
            "org/demo",
            "--root",
            &root_selector,
            "--json",
        ],
    );
    let added: serde_json::Value = serde_json::from_slice(&added.stdout).unwrap();
    assert!(
        added["results"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["path"] == "added.txt")
    );

    let removed = repo_a.join("needle.txt");
    fs::remove_file(&removed).unwrap();
    git(&repo_a, &["add", "-A"]);
    git(&repo_a, &["commit", "-q", "-m", "delete needle file"]);
    let update_a = run(
        &db,
        &[
            "index",
            "update",
            "--repo",
            "org/demo",
            "--root",
            &root_a_id.to_string(),
            "--json",
        ],
    );
    assert!(
        update_a.status.success(),
        "{}",
        String::from_utf8_lossy(&update_a.stderr)
    );
    let files = run(
        &db,
        &[
            "files",
            "needle.txt",
            "--repo",
            "org/demo",
            "--root",
            &root_a_id.to_string(),
            "--json",
        ],
    );
    let files: serde_json::Value = serde_json::from_slice(&files.stdout).unwrap();
    assert_eq!(files["summary"]["files"], 0);

    git(&repo_a, &["mv", "src/code.rs", "src/renamed.rs"]);
    git(&repo_a, &["commit", "-q", "-m", "rename source file"]);
    let rename_update = run(
        &db,
        &[
            "index",
            "update",
            "--repo",
            "org/demo",
            "--root",
            &root_a_id.to_string(),
            "--json",
        ],
    );
    assert!(rename_update.status.success());
    let old_name = run(
        &db,
        &[
            "files",
            "src/code.rs",
            "--repo",
            "org/demo",
            "--root",
            &root_a_id.to_string(),
            "--json",
        ],
    );
    let old_name: serde_json::Value = serde_json::from_slice(&old_name.stdout).unwrap();
    assert_eq!(old_name["summary"]["files"], 0);
    let new_name = run(
        &db,
        &[
            "files",
            "src/renamed.rs",
            "--repo",
            "org/demo",
            "--root",
            &root_a_id.to_string(),
            "--json",
        ],
    );
    let new_name: serde_json::Value = serde_json::from_slice(&new_name.stdout).unwrap();
    assert_eq!(new_name["summary"]["files"], 1);

    git(
        &repo_a,
        &["checkout", "-q", "-b", "feature", &initial_head_a],
    );
    fs::write(repo_a.join("feature.txt"), "feature branch marker\n").unwrap();
    git(&repo_a, &["add", "feature.txt"]);
    git(&repo_a, &["commit", "-q", "-m", "feature branch commit"]);
    let branch_update = run(
        &db,
        &[
            "index",
            "update",
            "--repo",
            "org/demo",
            "--root",
            &root_a_id.to_string(),
            "--json",
        ],
    );
    assert!(
        branch_update.status.success(),
        "{}",
        String::from_utf8_lossy(&branch_update.stderr)
    );
    let feature_path = run(
        &db,
        &[
            "files",
            "feature.txt",
            "--repo",
            "org/demo",
            "--root",
            &root_a_id.to_string(),
            "--json",
        ],
    );
    let feature_path: serde_json::Value = serde_json::from_slice(&feature_path.stdout).unwrap();
    assert_eq!(feature_path["summary"]["files"], 1);
    let main_only_path = run(
        &db,
        &[
            "files",
            "added.txt",
            "--repo",
            "org/demo",
            "--root",
            &root_a_id.to_string(),
            "--json",
        ],
    );
    let main_only_path: serde_json::Value = serde_json::from_slice(&main_only_path.stdout).unwrap();
    assert_eq!(main_only_path["summary"]["files"], 0);

    fs::write(repo_a.join("feature.txt"), "modified branch token\n").unwrap();
    git(&repo_a, &["add", "feature.txt"]);
    git(&repo_a, &["commit", "-q", "-m", "modify feature file"]);
    let modified_update = run(
        &db,
        &[
            "index",
            "update",
            "--repo",
            "org/demo",
            "--root",
            &root_a_id.to_string(),
            "--json",
        ],
    );
    assert!(modified_update.status.success());
    let modified_search = run(
        &db,
        &[
            "search",
            "modified branch token",
            "--repo",
            "org/demo",
            "--root",
            &root_a_id.to_string(),
            "--json",
        ],
    );
    let modified_search: serde_json::Value =
        serde_json::from_slice(&modified_search.stdout).unwrap();
    assert!(
        modified_search["results"]
            .as_array()
            .unwrap()
            .iter()
            .any(|result| result["path"] == "feature.txt")
    );

    let indexed_commit_before_child = git_output(&repo_a, &["rev-parse", "HEAD"])
        .trim()
        .to_owned();
    fs::write(repo_a.join("recovered.txt"), "full rebuild marker\n").unwrap();
    git(&repo_a, &["add", "recovered.txt"]);
    git(&repo_a, &["commit", "-q", "-m", "child of indexed commit"]);
    let commit_object = repo_a
        .join(".git")
        .join("objects")
        .join(&indexed_commit_before_child[..2])
        .join(&indexed_commit_before_child[2..]);
    assert!(
        commit_object.is_file(),
        "expected an unpacked synthetic parent commit"
    );
    fs::remove_file(&commit_object).unwrap();
    let recovered = run(
        &db,
        &[
            "index",
            "update",
            "--repo",
            "org/demo",
            "--root",
            &root_a_id.to_string(),
            "--json",
        ],
    );
    assert!(
        recovered.status.success(),
        "{}",
        String::from_utf8_lossy(&recovered.stderr)
    );
    let recovered: serde_json::Value = serde_json::from_slice(&recovered.stdout).unwrap();
    assert_eq!(recovered["items"][0]["outcome"], "rebuilt");
    let repeat = run(
        &db,
        &[
            "index",
            "update",
            "--repo",
            "org/demo",
            "--root",
            &root_a_id.to_string(),
            "--json",
        ],
    );
    assert!(repeat.status.success());
    let repeat: serde_json::Value = serde_json::from_slice(&repeat.stdout).unwrap();
    assert_eq!(repeat["items"][0]["outcome"], "updated");
    let repeated_search = run(
        &db,
        &[
            "search",
            "full rebuild marker",
            "--repo",
            "org/demo",
            "--root",
            &root_a_id.to_string(),
            "--json",
        ],
    );
    let repeated_search: serde_json::Value =
        serde_json::from_slice(&repeated_search.stdout).unwrap();
    assert_eq!(repeated_search["summary"]["matches"], 1);
    let recovered_path = run(
        &db,
        &[
            "files",
            "recovered.txt",
            "--repo",
            "org/demo",
            "--root",
            &root_a_id.to_string(),
            "--json",
        ],
    );
    let recovered_path: serde_json::Value = serde_json::from_slice(&recovered_path.stdout).unwrap();
    assert_eq!(recovered_path["summary"]["files"], 1);

    let metadata = xingshu_core::db::Database::open(&db).unwrap();
    let removed_repo = metadata
        .list_repos()
        .unwrap()
        .into_iter()
        .find(|repo| repo.root_id == root_a_id)
        .unwrap();
    let removed_root_record = metadata
        .list_roots()
        .unwrap()
        .into_iter()
        .find(|root| root.id == Some(root_a_id))
        .unwrap();
    let removed_search_repo = xingshu_core::search_index::SearchRepo {
        repo: removed_repo,
        root: removed_root_record,
    };
    let removed_root = run(&db, &["roots", "rm", &root_a_id.to_string()]);
    assert!(removed_root.status.success());
    assert!(
        repo_a.is_dir(),
        "roots rm must not delete source repositories"
    );
    let index_service =
        xingshu_core::search_index::IndexService::open(&db, xingshu_core::config::current_env())
            .unwrap();
    let purged = index_service
        .status(std::slice::from_ref(&removed_search_repo))
        .unwrap();
    assert_eq!(purged[0].state, "missing");
    let remaining = run(&db, &["search", "needle", "--repo", "org/demo", "--json"]);
    assert!(remaining.status.success());
    let remaining: serde_json::Value = serde_json::from_slice(&remaining.stdout).unwrap();
    assert_eq!(remaining["summary"]["repos"], 1);
    assert_eq!(remaining["results"][0]["rootId"], 2);
}

#[test]
fn index_cache_isolated_by_metadata_database_identity() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("root");
    fixture(&root.join("org").join("demo"));
    let db_a = temp.path().join("a.db");
    let db_b = temp.path().join("b.db");
    assert!(
        run(&db_a, &["roots", "add", root.to_str().unwrap()])
            .status
            .success()
    );
    assert!(run(&db_a, &["scan"]).status.success());
    assert!(
        run(&db_a, &["index", "update", "--repo", "org/demo", "--json"])
            .status
            .success()
    );
    assert!(
        run(&db_b, &["roots", "add", root.to_str().unwrap()])
            .status
            .success()
    );
    assert!(run(&db_b, &["scan"]).status.success());
    let status = run(&db_b, &["index", "status", "--repo", "org/demo", "--json"]);
    let status: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    let metadata = xingshu_core::db::Database::open(&db_a).unwrap();
    let repo = metadata.list_repos().unwrap().into_iter().next().unwrap();
    let root = metadata.list_roots().unwrap().into_iter().next().unwrap();
    let search_repo = xingshu_core::search_index::SearchRepo { repo, root };
    let production = xingshu_core::search_index::IndexService::open(&db_a, "production").unwrap();
    let production_status = production
        .status(std::slice::from_ref(&search_repo))
        .unwrap();
    assert_eq!(production_status[0].state, "missing");
    assert_eq!(status["items"][0]["state"], "missing");
}

#[test]
fn missing_new_blob_rolls_back_and_keeps_previous_generation() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("root");
    let repo = root.join("org").join("demo");
    fixture(&repo);
    let db = temp.path().join("meta.db");
    assert!(
        run(&db, &["roots", "add", root.to_str().unwrap()])
            .status
            .success()
    );
    assert!(run(&db, &["scan"]).status.success());
    assert!(
        run(&db, &["index", "update", "--repo", "org/demo", "--json"])
            .status
            .success()
    );
    let indexed_head = git_output(&repo, &["rev-parse", "HEAD"]).trim().to_owned();

    fs::write(repo.join("added.txt"), "new token\n").unwrap();
    git(&repo, &["add", "added.txt"]);
    git(&repo, &["commit", "-q", "-m", "add new file"]);
    let new_blob = git_output(&repo, &["rev-parse", "HEAD:added.txt"]);
    let new_blob = new_blob.trim();
    let object_path = repo
        .join(".git")
        .join("objects")
        .join(&new_blob[..2])
        .join(&new_blob[2..]);
    assert!(object_path.is_file(), "expected a loose synthetic Git blob");
    fs::remove_file(&object_path).unwrap();

    let failed = run(&db, &["index", "update", "--repo", "org/demo", "--json"]);
    assert!(!failed.status.success());
    let failed: serde_json::Value = serde_json::from_slice(&failed.stdout).unwrap();
    assert_eq!(failed["summary"]["complete"], false);
    assert_eq!(failed["summary"]["failed"], 1);
    assert!(!failed["errors"].as_array().unwrap().is_empty());

    let status = run(&db, &["index", "status", "--repo", "org/demo", "--json"]);
    let status: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(status["items"][0]["indexedCommit"], indexed_head);
    assert_eq!(status["items"][0]["state"], "stale");
    let new_path = run(&db, &["files", "added.txt", "--repo", "org/demo", "--json"]);
    let new_path: serde_json::Value = serde_json::from_slice(&new_path.stdout).unwrap();
    assert_eq!(new_path["summary"]["files"], 0);
}

#[test]
fn killed_index_update_recovers_previous_generation_and_retry_is_clean() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("root");
    let repo = root.join("org").join("demo");
    fixture(&repo);
    let db = temp.path().join("meta.db");
    assert!(
        run(&db, &["roots", "add", root.to_str().unwrap()])
            .status
            .success()
    );
    assert!(run(&db, &["scan"]).status.success());
    assert!(
        run(&db, &["index", "update", "--repo", "org/demo", "--json"])
            .status
            .success()
    );
    let indexed_head = git_output(&repo, &["rev-parse", "HEAD"]).trim().to_owned();

    let bulk = repo.join("bulk");
    fs::create_dir_all(&bulk).unwrap();
    let filler = "x".repeat(2_048);
    for index in 0..5_000 {
        fs::write(
            bulk.join(format!("doc-{index:05}.txt")),
            format!("bulk-marker-{index:05}\n{filler}\n"),
        )
        .unwrap();
    }
    git(&repo, &["add", "bulk"]);
    git(&repo, &["commit", "-q", "-m", "bulk update fixture"]);

    let sidecar = fs::read_dir(temp.path())
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .starts_with(".xs-search-v2-")
        })
        .expect("index sidecar should exist");
    let mut wal_path = sidecar.clone();
    wal_path.set_extension("sqlite-wal");
    let baseline_wal = fs::metadata(&wal_path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let mut child = Command::new(env!("CARGO_BIN_EXE_xs"))
        .arg("--db")
        .arg(&db)
        .args(["index", "update", "--repo", "org/demo", "--json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut observed_wal_growth = false;
    for _ in 0..2_000 {
        assert!(
            child.try_wait().unwrap().is_none(),
            "index update finished before WAL growth was observed"
        );
        let wal_size = fs::metadata(&wal_path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        if wal_size > baseline_wal + 4_096 {
            observed_wal_growth = true;
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert!(
        observed_wal_growth,
        "expected update transaction to write WAL before cancellation"
    );
    child.kill().unwrap();
    assert!(!child.wait().unwrap().success());

    let status = run(&db, &["index", "status", "--repo", "org/demo", "--json"]);
    assert!(status.status.success());
    let status: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(status["items"][0]["indexedCommit"], indexed_head);
    assert_eq!(status["items"][0]["state"], "stale");
    let before_retry = run(
        &db,
        &[
            "files",
            "bulk/doc-00000.txt",
            "--repo",
            "org/demo",
            "--json",
        ],
    );
    let before_retry: serde_json::Value = serde_json::from_slice(&before_retry.stdout).unwrap();
    assert_eq!(before_retry["summary"]["files"], 0);

    let retry = run(&db, &["index", "update", "--repo", "org/demo", "--json"]);
    assert!(
        retry.status.success(),
        "{}",
        String::from_utf8_lossy(&retry.stderr)
    );
    let after_retry = run(
        &db,
        &[
            "search",
            "bulk-marker-00000",
            "--repo",
            "org/demo",
            "--json",
        ],
    );
    assert!(after_retry.status.success());
    let after_retry: serde_json::Value = serde_json::from_slice(&after_retry.stdout).unwrap();
    assert_eq!(after_retry["summary"]["matches"], 1);
}

#[test]
fn index_update_requires_explicit_scope() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("meta.db");
    let output = run(&db, &["index", "update", "--json"]);
    assert!(!output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["summary"]["complete"], false);
    assert_eq!(value["summary"]["failed"], 1);
    assert!(!value["errors"].as_array().unwrap().is_empty());
    let status = run(
        &db,
        &["index", "status", "--repo", "missing/demo", "--json"],
    );
    assert!(!status.status.success());
    let status: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert!(status["items"].as_array().unwrap().is_empty());
    assert!(!status["errors"].as_array().unwrap().is_empty());
}

#[test]
fn missing_index_and_invalid_read_emit_domain_json_errors() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("root");
    fixture(&root.join("org").join("demo"));
    let db = temp.path().join("meta.db");
    assert!(
        run(&db, &["roots", "add", root.to_str().unwrap()])
            .status
            .success()
    );
    assert!(run(&db, &["scan"]).status.success());
    let search = run(&db, &["search", "needle", "--repo", "org/demo", "--json"]);
    assert!(!search.status.success());
    let value: serde_json::Value = serde_json::from_slice(&search.stdout).unwrap();
    assert_eq!(value["summary"]["complete"], false);
    assert_eq!(value["errors"][0]["code"], "index_missing");
    assert!(
        value["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("xs index update")
    );
    let read = run(&db, &["read", "org/demo", "../escape", "--json"]);
    assert!(!read.status.success());
    let value: serde_json::Value = serde_json::from_slice(&read.stdout).unwrap();
    assert!(!value["errors"].as_array().unwrap().is_empty());
}

#[cfg(windows)]
#[test]
fn read_rejects_symlink_blob_without_reading_target() {
    use std::os::windows::fs::symlink_file;
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("root");
    let repo = root.join("org").join("demo");
    fixture(&repo);
    let outside = temp.path().join("outside.txt");
    fs::write(&outside, "outside secret").unwrap();
    let link = repo.join("outside-link.txt");
    if symlink_file(&outside, &link).is_err() {
        // Windows Developer Mode or elevated symlink privilege is required.
        return;
    }
    git(&repo, &["add", "outside-link.txt"]);
    git(&repo, &["commit", "-q", "-m", "symlink fixture"]);
    let db = temp.path().join("meta.db");
    assert!(
        run(&db, &["roots", "add", root.to_str().unwrap()])
            .status
            .success()
    );
    assert!(run(&db, &["scan"]).status.success());
    let read = run(&db, &["read", "org/demo", "outside-link.txt", "--json"]);
    assert!(!read.status.success());
    let output: serde_json::Value = serde_json::from_slice(&read.stdout).unwrap();
    assert!(!output["errors"].as_array().unwrap().is_empty());
    assert_eq!(fs::read_to_string(outside).unwrap(), "outside secret");
}

#[test]
fn flags_reject_invalid_caps_and_ref_requires_one_repo() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("meta.db");
    let bad_scope = run(&db, &["grep", "needle", "--json"]);
    assert!(!bad_scope.status.success());
    let value: serde_json::Value = serde_json::from_slice(&bad_scope.stdout).unwrap();
    assert!(!value["errors"].as_array().unwrap().is_empty());
    let bad_type = run(&db, &["files", "**/*", "--type", "unknown", "--json"]);
    assert!(!bad_type.status.success());
    let value: serde_json::Value = serde_json::from_slice(&bad_type.stdout).unwrap();
    assert!(!value["errors"].as_array().unwrap().is_empty());
    let bad_repo = run(
        &db,
        &["search", "needle", "--repo", "missing/demo", "--json"],
    );
    assert!(!bad_repo.status.success());
    let value: serde_json::Value = serde_json::from_slice(&bad_repo.stdout).unwrap();
    assert!(!value["errors"].as_array().unwrap().is_empty());
    let bad_tag = run(&db, &["files", "**/*", "--tag", "missing-tag", "--json"]);
    assert!(!bad_tag.status.success());
    let value: serde_json::Value = serde_json::from_slice(&bad_tag.stdout).unwrap();
    assert!(!value["errors"].as_array().unwrap().is_empty());
    let bad_read = run(&db, &["read", "missing/demo", "file.txt", "--json"]);
    assert!(!bad_read.status.success());
    let value: serde_json::Value = serde_json::from_slice(&bad_read.stdout).unwrap();
    assert!(value["result"].is_object());
    assert!(!value["errors"].as_array().unwrap().is_empty());
    let bad_cap = run(
        &db,
        &["grep", "x", "--repo", "org/demo", "--max-results", "0"],
    );
    assert!(!bad_cap.status.success());
    let bad_ref = run(&db, &["grep", "x", "--ref", "HEAD", "--json"]);
    assert!(!bad_ref.status.success());
    let value: serde_json::Value = serde_json::from_slice(&bad_ref.stdout).unwrap();
    assert!(!value["errors"].as_array().unwrap().is_empty());
}
