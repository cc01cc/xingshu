use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use anyhow::{Result, anyhow};
use chrono::Utc;
use clap::{Args, Parser, Subcommand};
use xingshu_core::ProgressReporter;
use xingshu_core::clone::clone_and_register;
use xingshu_core::db::Database;
use xingshu_core::policy::{PullConflictAction, default_for_kind};
use xingshu_core::puller::{PullMode, make_fetch_log, pull_repo};
use xingshu_core::scanner::scan_roots_with_reporter;
use xingshu_core::types::{
    Policy as RepoPolicy, RepoKind, RepoRecord, Root, ScanOptions, VcsError,
};

mod search_output;

#[derive(Debug, Parser)]
#[command(
    name = "xs",
    version,
    about = "Local Git repository index and workspace manager"
)]
struct Cli {
    #[arg(long, global = true)]
    db: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Args)]
struct DbArgs {
    #[command(subcommand)]
    command: DbCommand,
}

#[derive(Debug, Subcommand)]
enum DbCommand {
    Path,
}

#[derive(Debug, Subcommand)]
enum Command {
    Db(DbArgs),
    Clone(CloneArgs),
    Roots {
        #[command(subcommand)]
        command: RootCommand,
    },
    Scan(ScanArgs),
    List(ListArgs),
    Tag(TagArgs),
    Untag(TagArgs),
    Kind(KindArgs),
    Policy(PolicyArgs),
    Pull(PullArgs),
    Move(MoveArgs),
    Stats(StatsArgs),
    Status(StatusArgs),
    Backup(BackupArgs),
    Index(IndexArgs),
    Search(QueryArgs),
    Files(FilesArgs),
    Grep(GrepArgs),
    Read(ReadArgs),
}

#[derive(Debug, Args)]
struct IndexArgs {
    #[command(subcommand)]
    command: IndexCommand,
}
#[derive(Debug, Subcommand)]
enum IndexCommand {
    Status(RepoSelectArgs),
    Update(IndexUpdateArgs),
    Rebuild(IndexUpdateArgs),
}
#[derive(Debug, Args)]
struct RepoSelectArgs {
    #[arg(long = "repo")]
    repos: Vec<String>,
    #[arg(long)]
    root: Option<i64>,
    #[arg(long)]
    json: bool,
}
#[derive(Debug, Args)]
struct IndexUpdateArgs {
    #[arg(long = "repo")]
    repos: Vec<String>,
    #[arg(long)]
    root: Option<i64>,
    #[arg(long, conflicts_with_all = ["root", "repos"])]
    all_roots: bool,
    #[arg(long)]
    json: bool,
}
#[derive(Debug, Args)]
struct QueryArgs {
    pattern: String,
    #[arg(long = "repo")]
    repos: Vec<String>,
    #[arg(long)]
    tag: Option<String>,
    #[arg(long)]
    root: Option<i64>,
    #[arg(short = 'F', long)]
    fixed_strings: bool,
    #[arg(short = 'i', long)]
    ignore_case: bool,
    #[arg(short = 'w', long)]
    word_regexp: bool,
    #[arg(short = 'C', long, default_value_t = 0, value_parser = context_parser)]
    context: usize,
    #[arg(short = 'l', long)]
    files_with_matches: bool,
    #[arg(short = 'c', long)]
    count: bool,
    #[arg(long)]
    max_count: Option<usize>,
    #[arg(long, default_value_t = 1000, value_parser = results_parser)]
    max_results: usize,
    #[arg(long = "type")]
    types: Vec<String>,
    #[arg(long)]
    glob: Vec<String>,
    #[arg(long)]
    json: bool,
}
#[derive(Debug, Args)]
struct GrepArgs {
    pattern: String,
    #[arg(long = "repo")]
    repos: Vec<String>,
    #[arg(long)]
    tag: Option<String>,
    #[arg(long)]
    root: Option<i64>,
    #[arg(long, conflicts_with_all = ["root", "repos"])]
    all_roots: bool,
    #[arg(long = "ref")]
    git_ref: Option<String>,
    #[arg(short = 'F', long)]
    fixed_strings: bool,
    #[arg(short = 'i', long)]
    ignore_case: bool,
    #[arg(short = 'w', long)]
    word_regexp: bool,
    #[arg(short = 'C', long, default_value_t = 0, value_parser = context_parser)]
    context: usize,
    #[arg(short = 'l', long)]
    files_with_matches: bool,
    #[arg(short = 'c', long)]
    count: bool,
    #[arg(long)]
    max_count: Option<usize>,
    #[arg(long, default_value_t = 1000, value_parser = results_parser)]
    max_results: usize,
    #[arg(long = "type")]
    types: Vec<String>,
    #[arg(long)]
    glob: Vec<String>,
    #[arg(long)]
    json: bool,
}
#[derive(Debug, Args)]
struct FilesArgs {
    pattern: String,
    #[arg(long = "repo")]
    repos: Vec<String>,
    #[arg(long)]
    tag: Option<String>,
    #[arg(long)]
    root: Option<i64>,
    #[arg(long = "type")]
    types: Vec<String>,
    #[arg(long)]
    glob: Vec<String>,
    #[arg(long, default_value_t = 10000, value_parser = results_parser)]
    max_results: usize,
    #[arg(long)]
    json: bool,
}
#[derive(Debug, Args)]
struct ReadArgs {
    repo: String,
    path: PathBuf,
    #[arg(long)]
    root: Option<i64>,
    #[arg(long = "ref")]
    git_ref: Option<String>,
    #[arg(long)]
    start_line: Option<usize>,
    #[arg(long, default_value_t = 200, value_parser = read_lines_parser)]
    max_lines: usize,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct BackupArgs {
    #[command(subcommand)]
    command: BackupCommand,
}

#[derive(Debug, Subcommand)]
enum BackupCommand {
    /// Create a copy of a repository (directory snapshot or tar archive).
    Create {
        repo: String,
        /// Destination directory (defaults to the repository's parent).
        #[arg(long)]
        dest: Option<PathBuf>,
        /// Archive suffix such as `tar.gz` / `tar.zst`; omit for a directory snapshot.
        #[arg(long)]
        archive: Option<String>,
        /// Keep at most N backups for this repository (0 disables pruning).
        #[arg(long, default_value_t = 5)]
        keep: usize,
    },
    /// List backups for a repository.
    List {
        repo: String,
        #[arg(long)]
        dest: Option<PathBuf>,
    },
    /// Restore a backup over the repository (current state is kept as pre-restore).
    Restore { repo: String, backup: PathBuf },
    /// Prune older backups (dry-run preview by default; `--execute` deletes).
    Prune {
        repo: String,
        #[arg(long)]
        dest: Option<PathBuf>,
        /// Keep at most N backups for this repository.
        #[arg(long, default_value_t = 5)]
        keep: usize,
        /// Actually delete; without this flag only the affected list is printed.
        #[arg(long)]
        execute: bool,
    },
}

#[derive(Debug, Args)]
struct StatusArgs {
    /// Limit to a registered root id.
    #[arg(long)]
    root: Option<i64>,
    /// Limit to a single repository.
    #[arg(long)]
    repo: Option<String>,
    /// Check the real remote with `git ls-remote` (read-only, requires network).
    #[arg(long)]
    remote: bool,
}

#[derive(Debug, Args)]
struct CloneArgs {
    /// Git HTTP(S), SSH/SCP-like, or file URL.
    remote: String,
    /// Registered root id that will contain the clone.
    #[arg(long)]
    root: i64,
    /// New directory relative to the selected root; its parent must exist.
    #[arg(long)]
    dest: PathBuf,
    /// Classify repositories owned by this organization as `own` (repeatable).
    #[arg(long = "my-org")]
    my_orgs: Vec<String>,
}

#[derive(Debug, Subcommand)]
enum RootCommand {
    Add {
        path: PathBuf,
        #[arg(long)]
        disk: Option<String>,
        #[arg(long, default_value_t = 0)]
        priority: i64,
    },
    List,
    Rm {
        id: i64,
    },
}

#[derive(Debug, Args)]
struct ScanArgs {
    #[arg(long = "root")]
    roots: Vec<PathBuf>,
    #[arg(long, default_value_t = 0)]
    max_depth: usize,
    #[arg(long)]
    include_nested: bool,
    #[arg(long = "my-org")]
    my_orgs: Vec<String>,
    /// Disable incremental scan: re-inspect every repository.
    #[arg(long)]
    full: bool,
}

#[derive(Debug, Args)]
struct ListArgs {
    query: Option<String>,
    #[arg(long)]
    tag: Option<String>,
    #[arg(long)]
    root: Option<i64>,
    #[arg(long)]
    status: Option<String>,
    #[arg(long)]
    org: Option<String>,
}

#[derive(Debug, Args)]
struct TagArgs {
    repo: String,
    tags: Vec<String>,
}

#[derive(Debug, Args)]
struct KindArgs {
    #[command(subcommand)]
    command: KindCommand,
}

#[derive(Debug, Args)]
struct PolicyArgs {
    #[command(subcommand)]
    command: PolicyCommand,
}

#[derive(Debug, Subcommand)]
enum PolicyCommand {
    Set {
        repo: Option<String>,
        #[arg(long, conflicts_with = "repo")]
        tag: Option<String>,
        #[arg(long, default_value = "fetch-only")]
        strategy: String,
        #[arg(long, default_value = "stop")]
        conflict: String,
        #[arg(long, default_value = "skip")]
        unattended: String,
    },
}

#[derive(Debug, Subcommand)]
enum KindCommand {
    Set {
        repo: String,
        kind: String,
        #[arg(long)]
        upstream: Option<String>,
    },
}

#[derive(Debug, Args)]
struct PullArgs {
    #[arg(long, default_value_t = 8)]
    jobs: usize,
    #[arg(long)]
    unattended: bool,
    /// Limit the batch pull to a registered root id.
    #[arg(long)]
    root: Option<i64>,
    /// Preview the batch without writing anything (no fetch, no fetch_log/status).
    #[arg(long = "dry-run")]
    dry_run: bool,
    /// With --dry-run: fetch first for an exact prediction (writes local refs).
    #[arg(long)]
    fetch: bool,
}

#[derive(Debug, Args)]
struct StatsArgs {
    /// Aggregate by dimension.
    #[arg(long, value_parser = ["root", "tag", "org"])]
    by: Option<String>,
}

#[derive(Debug, Args)]
struct MoveArgs {
    repo: String,
    root: i64,
}

/// Synchronous CLI progress reporter (PLAN-208 M5).
///
/// Prints per-item progress to stderr so stdout stays machine-readable
/// (scan still emits the final JSON report to stdout).
#[derive(Debug, Default)]
struct CliReporter {
    current: AtomicU64,
}

impl ProgressReporter for CliReporter {
    fn started(&self, total: Option<u64>) {
        self.current.store(0, Ordering::Relaxed);
        match total {
            Some(total) => eprintln!("started: 0/{total}"),
            None => eprintln!("started: discovering repositories"),
        }
    }

    fn item_finished(&self, item: &str, result: &str, duration_ms: Option<u64>) {
        let current = self.current.fetch_add(1, Ordering::Relaxed) + 1;
        match duration_ms {
            Some(duration_ms) => eprintln!("[{current}] {item}: {result} ({duration_ms}ms)"),
            None => eprintln!("[{current}] {item}: {result}"),
        }
    }

    fn finished(&self, result: &str) {
        let current = self.current.load(Ordering::Relaxed);
        eprintln!("finished: {current} items, result={result}");
    }
}

fn parse_bounded(value: &str, min: usize, max: usize) -> Result<usize, String> {
    let parsed = value
        .parse::<usize>()
        .map_err(|_| format!("invalid nonnegative integer: {value}"))?;
    if !(min..=max).contains(&parsed) {
        return Err(format!("value must be between {min} and {max}"));
    }
    Ok(parsed)
}
fn context_parser(value: &str) -> Result<usize, String> {
    parse_bounded(value, 0, 20)
}
fn results_parser(value: &str) -> Result<usize, String> {
    parse_bounded(value, 1, 100_000)
}
fn read_lines_parser(value: &str) -> Result<usize, String> {
    parse_bounded(value, 1, 5_000)
}

fn command_requires_git(command: &Command) -> bool {
    matches!(
        command,
        Command::Clone(_)
            | Command::Scan(_)
            | Command::Pull(_)
            | Command::Move(_)
            | Command::Index(_)
            | Command::Grep(_)
            | Command::Read(_)
            | Command::Search(_)
            | Command::Files(_)
    ) || matches!(command, Command::Status(args) if args.remote)
}

fn main() -> Result<()> {
    init_logging();
    let cli = Cli::parse();
    let db_config = resolve_cli_db_config(cli.db)?;
    xingshu_core::config::check_db_allowed_with(&db_config.path, db_config.environment.is_prod())
        .map_err(|error| anyhow!(error))?;

    if matches!(
        &cli.command,
        Command::Db(DbArgs {
            command: DbCommand::Path
        })
    ) {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "environment": db_config.environment.as_str(),
                "dbPath": db_config.path.display().to_string(),
            }))?
        );
        return Ok(());
    }

    if command_requires_git(&cli.command) {
        let cwd = std::env::current_dir()
            .map_err(|error| anyhow!("could not resolve current directory: {error}"))?;
        xingshu_core::git::run_git(&cwd, &["--version"])
            .map_err(|error| anyhow!(error.to_string()))?;
    }

    let db_path = db_config.path;
    let database = Database::open(&db_path).map_err(|error| anyhow!(error.to_string()))?;
    match cli.command {
        Command::Db(_) => unreachable!("db path command returns before opening the database"),
        Command::Clone(args) => clone_repository(&database, args),
        Command::Roots { command } => roots(&database, &db_path, command),
        Command::Scan(args) => scan(&database, args),
        Command::List(args) => list(&database, args),
        Command::Tag(args) => tag(&database, args, true),
        Command::Untag(args) => tag(&database, args, false),
        Command::Kind(args) => kind(&database, args),
        Command::Policy(args) => policy(&database, args),
        Command::Pull(args) => pull(&database, args),
        Command::Move(args) => move_repo(&database, args),
        Command::Stats(args) => stats(&database, args),
        Command::Status(args) => status(&database, args),
        Command::Backup(args) => run_backup(&database, args),
        Command::Index(args) => index_command(&database, &db_path, args),
        Command::Search(args) => search_command(&database, &db_path, args),
        Command::Files(args) => files_command(&database, &db_path, args),
        Command::Grep(args) => grep_command(&database, &db_path, args),
        Command::Read(args) => read_command(&database, &db_path, args),
    }
}

fn resolve_cli_db_config(
    explicit_db: Option<PathBuf>,
) -> Result<xingshu_core::config::CliDbConfig> {
    use xingshu_core::config::{
        self, CliEnvironment, DB_ENV_VAR, ENV_NAME, resolve_cli_db_path_for_environment_with,
        resolve_cli_environment,
    };

    let env_db = std::env::var_os(DB_ENV_VAR);
    let env_name = match std::env::var_os(ENV_NAME) {
        Some(value) => Some(
            value
                .into_string()
                .map_err(|_| anyhow!("{ENV_NAME} must be valid UTF-8"))?,
        ),
        None => None,
    };
    let release_build = option_env!("XINGSHU_CLI_BUILD_CHANNEL") == Some("release");
    let environment = resolve_cli_environment(env_name.as_deref(), release_build)
        .map_err(|error| anyhow!(error))?;
    let cwd = std::env::current_dir()
        .map_err(|error| anyhow!("could not resolve current directory: {error}"))?;
    let needs_user_data_root = explicit_db.is_none()
        && env_db.is_none()
        && (release_build || environment == CliEnvironment::Prod);
    let user_data_root = if needs_user_data_root {
        config::cli_user_data_root().map_err(|error| anyhow!(error))?
    } else {
        PathBuf::new()
    };

    Ok(resolve_cli_db_path_for_environment_with(
        explicit_db,
        env_db,
        environment,
        release_build,
        &cwd,
        &user_data_root,
    ))
}

fn search_repos(
    database: &Database,
    selectors: &[String],
    root_id: Option<i64>,
) -> Result<Vec<xingshu_core::search_index::SearchRepo>> {
    let roots = database.list_roots().map_err(|e| anyhow!(e.to_string()))?;
    if let Some(id) = root_id
        && !roots.iter().any(|root| root.id == Some(id))
    {
        return Err(anyhow!("root not found: {id}"));
    }
    let repos = database.list_repos().map_err(|e| anyhow!(e.to_string()))?;
    let matches_selector = |repo: &RepoRecord, selector: &str| {
        selector == format!("{}/{}", repo.org, repo.name)
            || selector == repo.name
            || selector == repo.rel_path.to_string_lossy()
            || repo
                .remote_host
                .as_ref()
                .is_some_and(|host| selector == format!("{host}/{}/{}", repo.org, repo.name))
    };
    for selector in selectors {
        if !repos.iter().any(|repo| {
            root_id.is_none_or(|id| repo.root_id == id) && matches_selector(repo, selector)
        }) {
            return Err(anyhow!("repository not found: {selector}"));
        }
    }
    let mut selected = Vec::new();
    for repo in repos {
        if root_id.is_some_and(|id| repo.root_id != id) {
            continue;
        }
        if !selectors.is_empty()
            && !selectors
                .iter()
                .any(|selector| matches_selector(&repo, selector))
        {
            continue;
        }
        if let Some(root) = roots.iter().find(|r| r.id == Some(repo.root_id)) {
            selected.push(xingshu_core::search_index::SearchRepo {
                repo,
                root: root.clone(),
            });
        }
    }
    Ok(selected)
}

fn query_repos(
    database: &Database,
    selectors: &[String],
    root_id: Option<i64>,
    tag: Option<&str>,
    json: bool,
    command: &str,
) -> Result<Vec<xingshu_core::search_index::SearchRepo>> {
    let repos = match search_repos(database, selectors, root_id) {
        Ok(repos) => repos,
        Err(error) => {
            if json {
                emit_query_error(command, &error.to_string());
            }
            return Err(error);
        }
    };
    match search_output::filter_repos_by_tag(database, repos, tag) {
        Ok(repos) => Ok(repos),
        Err(error) => {
            if json {
                emit_query_error(command, &error.to_string());
            }
            Err(error)
        }
    }
}

fn index_service(
    database: &Database,
    _db_path: &std::path::Path,
) -> Result<xingshu_core::search_index::IndexService> {
    xingshu_core::search_index::IndexService::open(
        database.path(),
        xingshu_core::config::current_env(),
    )
    .map_err(|e| anyhow!(e.to_string()))
}

fn index_command(database: &Database, db_path: &std::path::Path, args: IndexArgs) -> Result<()> {
    let mut service = index_service(database, db_path)?;
    match args.command {
        IndexCommand::Status(args) => {
            let repos = match search_repos(database, &args.repos, args.root) {
                Ok(repos) => repos,
                Err(error) => {
                    if args.json {
                        emit_index_error(true, 0, &error.to_string());
                    }
                    return Err(error);
                }
            };
            let rows = match service.status(&repos) {
                Ok(rows) => rows,
                Err(error) => {
                    if args.json {
                        emit_index_error(true, repos.len(), &error.to_string());
                    }
                    return Err(anyhow!(error.to_string()));
                }
            };
            let complete = rows.iter().all(|r| r.error.is_none());
            let value = serde_json::json!({"items":rows,"errors":[],"complete":complete});
            if args.json {
                println!("{}", serde_json::to_string(&value)?);
            } else {
                println!(
                    "{:<40} {:<8} {:<12} {:>6} {:>10}",
                    "repo", "state", "indexed", "files", "bytes"
                );
                for r in &rows {
                    println!(
                        "{:<40} {:<8} {:<12} {:>6} {:>10}",
                        r.repo,
                        r.state,
                        r.indexed_commit
                            .as_deref()
                            .unwrap_or("-")
                            .get(..8)
                            .unwrap_or("-"),
                        r.text_files,
                        r.text_bytes
                    );
                }
            }
        }
        IndexCommand::Update(args) => update_indexes(database, &mut service, args, false)?,
        IndexCommand::Rebuild(args) => update_indexes(database, &mut service, args, true)?,
    }
    Ok(())
}

fn update_indexes(
    database: &Database,
    service: &mut xingshu_core::search_index::IndexService,
    args: IndexUpdateArgs,
    rebuild: bool,
) -> Result<()> {
    if !args.all_roots && args.root.is_none() && args.repos.is_empty() {
        let message = "index update/rebuild requires --repo, --root, or --all-roots";
        if args.json {
            emit_index_error(false, 0, message);
        }
        return Err(anyhow!(message));
    }
    let repos = match search_repos(database, &args.repos, args.root) {
        Ok(repos) => repos,
        Err(error) => {
            if args.json {
                emit_index_error(false, 0, &error.to_string());
            }
            return Err(error);
        }
    };
    if !args.all_roots && args.root.is_none() {
        for selector in &args.repos {
            let selected = match search_repos(database, std::slice::from_ref(selector), None) {
                Ok(selected) => selected,
                Err(error) => {
                    if args.json {
                        emit_index_error(false, 0, &error.to_string());
                    }
                    return Err(error);
                }
            };
            if selected.len() > 1 {
                let message = format!("ambiguous_repo: provide --root for {selector}");
                if args.json {
                    emit_index_error(false, selected.len(), &message);
                }
                return Err(anyhow!(message));
            }
        }
    }
    let mut items = Vec::new();
    let mut errors = Vec::new();
    let mut updated = 0usize;
    let mut skipped = 0usize;
    let mut failed = 0usize;
    for repo in &repos {
        match service.update(repo, rebuild) {
            Ok(row) => {
                let value = serde_json::to_value(&row)?;
                match row.outcome.as_str() {
                    "updated" | "rebuilt" => updated += 1,
                    _ => skipped += 1,
                }
                items.push(value);
            }
            Err(error) => {
                failed += 1;
                errors.push(serde_json::json!({"repo":format!("{}/{}", repo.repo.org, repo.repo.name), "rootId": repo.repo.root_id, "code":"index_error", "message":error.to_string()}));
                items.push(serde_json::json!({"repo":format!("{}/{}",repo.repo.org,repo.repo.name),"rootId":repo.repo.root_id,"outcome":"failed","error":error.to_string()}));
            }
        }
    }
    let incomplete = !errors.is_empty();
    let result = serde_json::json!({
        "summary":{"requested":repos.len(),"updated":updated,"skipped":skipped,"failed":failed,"complete":!incomplete},
        "items":items,"errors":errors
    });
    if args.json {
        println!("{}", serde_json::to_string(&result)?);
    } else {
        println!(
            "index {}: requested={} updated={} skipped={} failed={}",
            if rebuild { "rebuild" } else { "update" },
            repos.len(),
            updated,
            skipped,
            failed
        );
        for e in &errors {
            eprintln!("  {}: {}", e["repo"], e["message"]);
        }
    }
    if incomplete {
        return Err(anyhow!("index update incomplete"));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn search_options(
    pattern: String,
    fixed_strings: bool,
    ignore_case: bool,
    word_regexp: bool,
    context: usize,
    files_with_matches: bool,
    count: bool,
    max_count: Option<usize>,
    max_results: usize,
    types: Vec<String>,
    globs: Vec<String>,
) -> xingshu_core::search_index::SearchOptions {
    xingshu_core::search_index::SearchOptions {
        pattern,
        fixed_strings,
        ignore_case,
        word_regexp,
        context,
        files_with_matches,
        count,
        max_count,
        max_results: Some(max_results),
        types,
        globs,
    }
}

fn emit_query_error(kind: &str, message: &str) {
    let code = if message.contains("index_missing:") {
        "index_missing"
    } else if message.contains("index_stale:") {
        "index_stale"
    } else {
        "query_error"
    };
    let error = serde_json::json!({"code":code,"message":message});
    let value = match kind {
        "files" => {
            serde_json::json!({"summary":{"files":0,"repos":0,"complete":false,"truncated":false,"maxResults":null},"results":[],"errors":[error]})
        }
        "read" => {
            serde_json::json!({"result":{"repo":"","rootId":null,"revision":"","path":"","startLine":1,"endLine":0,"content":"","truncated":false,"source":"head"},"errors":[error]})
        }
        _ => {
            serde_json::json!({"summary":{"matches":0,"files":0,"repos":0,"source":kind,"complete":false,"truncated":false,"maxResults":null},"results":[],"errors":[error]})
        }
    };
    println!("{value}");
}

fn emit_index_error(status: bool, requested: usize, message: &str) {
    let error = serde_json::json!({"code":"query_error","message":message});
    let value = if status {
        serde_json::json!({"items":[],"errors":[error],"complete":false})
    } else {
        serde_json::json!({
            "summary":{"requested":requested,"updated":0,"skipped":0,"failed":1,"complete":false},
            "items":[],"errors":[error]
        })
    };
    println!("{value}");
}

fn output_failed(value: &serde_json::Value) -> bool {
    value
        .get("errors")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|errors| !errors.is_empty())
}

fn search_command(database: &Database, db_path: &std::path::Path, args: QueryArgs) -> Result<()> {
    let repos = query_repos(
        database,
        &args.repos,
        args.root,
        args.tag.as_deref(),
        args.json,
        "search",
    )?;
    let options = search_options(
        args.pattern,
        args.fixed_strings,
        args.ignore_case,
        args.word_regexp,
        args.context,
        args.files_with_matches,
        args.count,
        args.max_count,
        args.max_results,
        args.types,
        args.glob,
    );
    let service = index_service(database, db_path)?;
    match service.search_indexed(&repos, &options) {
        Ok(output) => {
            let value = serde_json::to_value(&output)?;
            if args.json {
                println!("{value}");
            } else {
                search_output::print_search_human(&output);
            }
            if output_failed(&value) {
                return Err(anyhow!("search returned operational errors"));
            }
        }
        Err(error) => {
            if args.json {
                emit_query_error("search", &error.to_string());
            }
            return Err(anyhow!(error.to_string()));
        }
    }
    Ok(())
}

fn grep_command(database: &Database, db_path: &std::path::Path, args: GrepArgs) -> Result<()> {
    if !args.all_roots && args.root.is_none() && args.repos.is_empty() {
        if args.json {
            emit_query_error("grep", "grep requires --repo, --root, or --all-roots");
        }
        return Err(anyhow!("grep requires --repo, --root, or --all-roots"));
    }
    let repos = query_repos(
        database,
        &args.repos,
        args.root,
        args.tag.as_deref(),
        args.json,
        "grep",
    )?;
    let scope = if args.all_roots {
        "all registered roots".to_owned()
    } else if let Some(root_id) = args.root {
        format!("root {root_id}")
    } else {
        format!("repo selector(s): {}", args.repos.join(", "))
    };
    eprintln!("grep scope: {scope}; repositories={}", repos.len());
    for repo in &repos {
        eprintln!(
            "grep scanning {}/{} (rootId={})",
            repo.repo.org, repo.repo.name, repo.repo.root_id
        );
    }
    if args.git_ref.is_some() && repos.len() != 1 {
        if args.json {
            emit_query_error("grep", "--ref requires exactly one repository");
        }
        return Err(anyhow!("--ref requires exactly one repository"));
    }
    let options = search_options(
        args.pattern,
        args.fixed_strings,
        args.ignore_case,
        args.word_regexp,
        args.context,
        args.files_with_matches,
        args.count,
        args.max_count,
        args.max_results,
        args.types,
        args.glob,
    );
    let service = index_service(database, db_path)?;
    let scan_started = Instant::now();
    let result = if let Some(reference) = args.git_ref {
        service.grep_ref(&repos[0], &reference, &options)
    } else {
        service.grep_worktree(&repos, &options)
    };
    match result {
        Ok(output) => {
            let value = serde_json::to_value(&output)?;
            if args.json {
                println!("{value}");
            } else {
                search_output::print_search_human(&output);
            }
            eprintln!(
                "grep complete: matches={} files={} skipped={} elapsedMs={} truncated={}",
                output.summary.matches,
                output.summary.files,
                output.summary.skipped_files,
                scan_started.elapsed().as_millis(),
                output.summary.truncated
            );
            if output_failed(&value) {
                return Err(anyhow!("grep returned operational errors"));
            }
        }
        Err(error) => {
            if args.json {
                emit_query_error("grep", &error.to_string());
            }
            return Err(anyhow!(error.to_string()));
        }
    }
    Ok(())
}

fn files_command(database: &Database, db_path: &std::path::Path, args: FilesArgs) -> Result<()> {
    let repos = query_repos(
        database,
        &args.repos,
        args.root,
        args.tag.as_deref(),
        args.json,
        "files",
    )?;
    let options = xingshu_core::search_index::FilesOptions {
        pattern: Some(args.pattern),
        types: args.types,
        globs: args.glob,
        max_results: Some(args.max_results),
    };
    match index_service(database, db_path)?.files_indexed(&repos, &options) {
        Ok(output) => {
            let value = serde_json::to_value(&output)?;
            let failed = output_failed(&value);
            if args.json {
                println!("{value}");
            } else {
                search_output::print_files_human(&output);
            }
            if failed {
                return Err(anyhow!("files returned operational errors"));
            }
        }
        Err(error) => {
            if args.json {
                emit_query_error("files", &error.to_string());
            }
            return Err(anyhow!(error.to_string()));
        }
    }
    Ok(())
}

fn read_command(database: &Database, db_path: &std::path::Path, args: ReadArgs) -> Result<()> {
    let repos = query_repos(
        database,
        std::slice::from_ref(&args.repo),
        args.root,
        None,
        args.json,
        "read",
    )?;
    if repos.is_empty() {
        if args.json {
            emit_query_error("read", "repository not found");
        }
        return Err(anyhow!("repository not found: {}", args.repo));
    }
    if repos.len() > 1 {
        if args.json {
            emit_query_error(
                "read",
                "ambiguous_repo: provide --root or select exactly one repository",
            );
        }
        return Err(anyhow!("ambiguous_repo: provide --root"));
    }
    let options = xingshu_core::search_index::ReadOptions {
        path: args.path.to_string_lossy().into_owned(),
        revision: args.git_ref,
        start_line: args.start_line,
        max_lines: Some(args.max_lines),
    };
    match index_service(database, db_path)?.read(&repos[0], &options) {
        Ok(output) => {
            if args.json {
                println!("{}", serde_json::json!({"result": output, "errors": []}));
            } else {
                println!("{}", output.content);
            }
        }
        Err(error) => {
            if args.json {
                emit_query_error("read", &error.to_string());
            }
            return Err(anyhow!(error.to_string()));
        }
    }
    Ok(())
}

fn init_logging() {
    let filter = xingshu_core::logging::resolve_log_level();
    if let Some(rotation) = xingshu_core::logging::resolve_rotation() {
        match xingshu_core::logging::SizeRollingFile::new(
            &rotation.path,
            rotation.max_bytes,
            rotation.max_files,
        ) {
            Ok(writer) => {
                let _ = tracing_subscriber::fmt()
                    .json()
                    .with_ansi(false)
                    .with_target(true)
                    .with_env_filter(filter)
                    .with_writer(std::sync::Mutex::new(writer))
                    .try_init();
            }
            Err(error) => eprintln!("xingshu log file disabled: {error}"),
        }
    } else {
        let _ = tracing_subscriber::fmt()
            .json()
            .with_ansi(false)
            .with_target(true)
            .with_env_filter(filter)
            .with_writer(std::io::stderr)
            .try_init();
    }
}

fn clone_repository(database: &Database, args: CloneArgs) -> Result<()> {
    let repo = clone_and_register(database, args.root, &args.remote, &args.dest, &args.my_orgs)
        .map_err(|error| anyhow!(error.to_string()))?;
    let commit = repo.head_commit.as_deref().unwrap_or("unborn");
    println!(
        "cloned {}/{} into root {} at {} (HEAD {})",
        repo.org,
        repo.name,
        repo.root_id,
        repo.rel_path.display(),
        commit
    );
    Ok(())
}

fn roots(database: &Database, db_path: &std::path::Path, command: RootCommand) -> Result<()> {
    match command {
        RootCommand::Add {
            path,
            disk,
            priority,
        } => {
            if !path.is_dir() {
                return Err(anyhow!(
                    "root does not exist or is not a directory: {}",
                    path.display()
                ));
            }
            let now = Utc::now();
            let root = Root {
                id: None,
                name: path
                    .file_name()
                    .map(|value| value.to_string_lossy().to_string())
                    .unwrap_or_else(|| "root".to_owned()),
                mount_point: disk.clone(),
                disk_label: disk,
                path,
                priority,
                created_at: now,
                updated_at: now,
            };
            let id = database
                .upsert_root(&root)
                .map_err(|error| anyhow!(error.to_string()))?;
            println!("registered root {id}");
        }
        RootCommand::List => println!("{}", serde_json::to_string_pretty(&database.list_roots()?)?),
        RootCommand::Rm { id } => {
            let repo_ids: Vec<i64> = database
                .list_repos()
                .map_err(|e| anyhow!(e.to_string()))?
                .into_iter()
                .filter(|r| r.root_id == id)
                .filter_map(|r| r.id)
                .collect();
            database
                .remove_root(id)
                .map_err(|error| anyhow!(error.to_string()))?;
            if let Ok(mut service) = index_service(database, db_path) {
                let _ = service.purge_repos(&repo_ids);
            }
        }
    }
    Ok(())
}

fn scan(database: &Database, args: ScanArgs) -> Result<()> {
    tracing::info!("scan requested");
    let registered = database
        .list_roots()
        .map_err(|error| anyhow!(error.to_string()))?;
    let roots = if args.roots.is_empty() {
        registered
    } else {
        args.roots
            .iter()
            .map(|path| {
                registered
                    .iter()
                    .find(|root| root.path == *path)
                    .cloned()
                    .ok_or_else(|| anyhow!("root is not registered: {}", path.display()))
            })
            .collect::<Result<Vec<_>>>()?
    };
    let options = ScanOptions {
        max_depth: (args.max_depth > 0).then_some(args.max_depth),
        include_nested: args.include_nested,
        my_orgs: args.my_orgs,
        incremental: !args.full,
        ..ScanOptions::default()
    };
    let reporter = CliReporter::default();
    let report = scan_roots_with_reporter(database, &roots, &options, Some(&reporter))
        .map_err(|error| anyhow!(error.to_string()))?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn list(database: &Database, args: ListArgs) -> Result<()> {
    let query = args.query.as_deref().map(str::to_lowercase);
    let mut repos = database
        .list_repos()
        .map_err(|error| anyhow!(error.to_string()))?
        .into_iter()
        .filter(|repo| {
            query.as_ref().is_none_or(|value| {
                format!(
                    "{}/{} {}",
                    repo.org,
                    repo.name,
                    repo.rel_path.to_string_lossy()
                )
                .to_lowercase()
                .contains(value)
            })
        })
        .filter(|repo| args.root.is_none_or(|root| repo.root_id == root))
        .filter(|repo| {
            args.status
                .as_ref()
                .is_none_or(|status| &repo.clone_status == status)
        })
        .filter(|repo| args.org.as_ref().is_none_or(|org| &repo.org == org))
        .collect::<Vec<_>>();
    if let Some(tag) = args.tag.as_deref() {
        repos = repos
            .into_iter()
            .filter_map(
                |repo| match database.repo_has_tag(repo.id.unwrap_or_default(), tag) {
                    Ok(true) => Some(Ok(repo)),
                    Ok(false) => None,
                    Err(error) => Some(Err(anyhow!(error.to_string()))),
                },
            )
            .collect::<Result<Vec<_>>>()?;
    }
    println!("{}", serde_json::to_string_pretty(&repos)?);
    Ok(())
}

fn tag(database: &Database, args: TagArgs, attach: bool) -> Result<()> {
    let repo = database
        .find_repo(&args.repo)
        .map_err(|error| anyhow!(error.to_string()))?
        .ok_or_else(|| anyhow!("repository not found: {}", args.repo))?;
    let repo_id = repo.id.ok_or_else(|| anyhow!("repository has no id"))?;
    for slug in args.tags {
        let tag_id = if attach {
            database
                .create_tag(&slug, &slug, None)
                .map_err(|error| anyhow!(error.to_string()))?
        } else {
            database
                .tag_id(&slug)
                .map_err(|error| anyhow!(error.to_string()))?
                .ok_or_else(|| anyhow!("tag not found: {slug}"))?
        };
        if attach {
            database
                .attach_tag(repo_id, tag_id)
                .map_err(|error| anyhow!(error.to_string()))?;
        } else {
            database
                .detach_tag(repo_id, tag_id)
                .map_err(|error| anyhow!(error.to_string()))?;
        }
    }
    Ok(())
}

fn kind(database: &Database, args: KindArgs) -> Result<()> {
    let KindCommand::Set {
        repo,
        kind,
        upstream,
    } = args.command;
    let repo = database
        .find_repo(&repo)
        .map_err(|error| anyhow!(error.to_string()))?
        .ok_or_else(|| anyhow!("repository not found"))?;
    let repo_id = repo.id.ok_or_else(|| anyhow!("repository has no id"))?;
    let kind = RepoKind::try_from(kind.as_str()).map_err(|error| anyhow!(error.to_string()))?;
    database
        .set_repo_kind(repo_id, &kind, upstream.as_deref())
        .map_err(|error| anyhow!(error.to_string()))?;
    Ok(())
}

fn policy(database: &Database, args: PolicyArgs) -> Result<()> {
    let PolicyCommand::Set {
        repo,
        tag,
        strategy,
        conflict,
        unattended,
    } = args.command;
    if !matches!(
        strategy.as_str(),
        "fetch-only" | "mirror" | "archive" | "no-update"
    ) {
        return Err(anyhow!("invalid pull strategy: {strategy}"));
    }
    let _ = xingshu_core::policy::PullConflictAction::try_from(conflict.as_str())
        .map_err(|error| anyhow!(error.to_string()))?;
    let _ = xingshu_core::policy::PullConflictAction::try_from(unattended.as_str())
        .map_err(|error| anyhow!(error.to_string()))?;
    let (repo_id, tag_id) = match (repo, tag) {
        (Some(repo), None) => {
            let repository = database
                .find_repo(&repo)
                .map_err(|error| anyhow!(error.to_string()))?
                .ok_or_else(|| anyhow!("repository not found"))?;
            (repository.id, None)
        }
        (None, Some(tag)) => (
            None,
            Some(
                database
                    .tag_id(&tag)
                    .map_err(|error| anyhow!(error.to_string()))?
                    .ok_or_else(|| anyhow!("tag not found: {tag}"))?,
            ),
        ),
        (Some(_), Some(_)) => return Err(anyhow!("choose either a repository or --tag")),
        (None, None) => return Err(anyhow!("a repository or --tag is required")),
    };
    let now = Utc::now();
    let policy = RepoPolicy {
        id: None,
        repo_id,
        tag_id,
        pull_strategy: strategy,
        fetch_schedule: None,
        depth: "full".to_owned(),
        auto_tag: false,
        pull_conflict_policy: conflict,
        unattended_conflict_policy: unattended,
        created_at: now,
        updated_at: now,
    };
    if policy.repo_id.is_some() {
        database
            .set_repo_policy(&policy)
            .map_err(|error| anyhow!(error.to_string()))?;
    } else {
        database
            .set_tag_policy(&policy)
            .map_err(|error| anyhow!(error.to_string()))?;
    }
    Ok(())
}

fn pull(database: &Database, args: PullArgs) -> Result<()> {
    if args.jobs == 0 {
        return Err(anyhow!("jobs must be greater than zero"));
    }
    tracing::info!(
        jobs = args.jobs,
        unattended = args.unattended,
        root = ?args.root,
        dry_run = args.dry_run,
        "pull batch requested"
    );
    let roots = database
        .list_roots()
        .map_err(|error| anyhow!(error.to_string()))?;
    let mut repos = database
        .list_repos()
        .map_err(|error| anyhow!(error.to_string()))?;
    if let Some(root_id) = args.root {
        if !roots.iter().any(|root| root.id == Some(root_id)) {
            return Err(anyhow!("root not found: {root_id}"));
        }
        repos.retain(|repo| repo.root_id == root_id);
    }
    if args.dry_run {
        return dry_run_pull(database, &roots, &repos, args.fetch);
    }
    let mode = if args.unattended {
        PullMode::Unattended
    } else {
        PullMode::Interactive
    };
    let total = repos.len();
    eprintln!("pull started: 0/{total}");
    if matches!(mode, PullMode::Interactive) {
        for (index, repo) in repos.iter().enumerate() {
            report_pull(database, &roots, repo, mode, Some((index + 1, total)))?;
        }
    } else {
        let done = AtomicU64::new(0);
        let roots_ref: &[Root] = &roots;
        for chunk in repos.chunks(args.jobs) {
            std::thread::scope(|scope| {
                let handles = chunk
                    .iter()
                    .map(|repo| {
                        let position = done.fetch_add(1, Ordering::Relaxed) as usize + 1;
                        scope.spawn(move || {
                            report_pull(database, roots_ref, repo, mode, Some((position, total)))
                        })
                    })
                    .collect::<Vec<_>>();
                for handle in handles {
                    match handle.join() {
                        Ok(Ok(())) => {}
                        Ok(Err(error)) => eprintln!("pull failed: {error}"),
                        Err(_) => eprintln!("pull worker panicked"),
                    }
                }
            });
        }
    }
    eprintln!("pull finished: {total} repositories processed");
    Ok(())
}

fn resolve_repo_policy(
    database: &Database,
    repo: &RepoRecord,
) -> Result<xingshu_core::policy::ResolvedPolicy> {
    let stored_policy = database
        .repo_policy(repo.id.unwrap_or_default())
        .map_err(|error| anyhow!(error.to_string()))?;
    let stored_policy = match stored_policy {
        Some(policy) => Some(policy),
        None => {
            let mut tag_policy = None;
            for tag_id in database
                .tag_ids_for_repo(repo.id.unwrap_or_default())
                .map_err(|error| anyhow!(error.to_string()))?
            {
                if let Some(policy) = database
                    .tag_policy(tag_id)
                    .map_err(|error| anyhow!(error.to_string()))?
                {
                    tag_policy = Some(policy);
                    break;
                }
            }
            tag_policy
        }
    };
    match stored_policy {
        Some(stored) => Ok(xingshu_core::policy::ResolvedPolicy {
            pull_strategy: stored.pull_strategy,
            conflict_action: xingshu_core::policy::PullConflictAction::try_from(
                stored.pull_conflict_policy.as_str(),
            )
            .map_err(|error| anyhow!(error.to_string()))?,
            unattended_action: xingshu_core::policy::PullConflictAction::try_from(
                stored.unattended_conflict_policy.as_str(),
            )
            .map_err(|error| anyhow!(error.to_string()))?,
        }),
        None => Ok(default_for_kind(&repo.repo_kind)),
    }
}

/// Preview a pull batch without writing anything (PLAN-297 ISS-04).
///
/// Default mode is strictly zero-write: no fetch, no `fetch_log`, no status
/// updates. With `fetch = true`, local remote refs are refreshed first so the
/// prediction is exact; audit tables are still left untouched.
fn dry_run_pull(
    database: &Database,
    roots: &[Root],
    repos: &[RepoRecord],
    fetch: bool,
) -> Result<()> {
    if fetch {
        eprintln!("pull dry-run: --fetch enabled (local refs updated; no fetch_log/status writes)");
    } else {
        eprintln!(
            "pull dry-run: zero-write mode (remote divergence not verified; add --fetch for exact prediction)"
        );
    }
    let total = repos.len();
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    for (index, repo) in repos.iter().enumerate() {
        let verdict = dry_run_verdict(database, roots, repo, fetch)
            .unwrap_or_else(|error| format!("error: {error}"));
        *counts.entry(verdict.clone()).or_default() += 1;
        println!(
            "[{}/{}] {}/{}: {}",
            index + 1,
            total,
            repo.org,
            repo.name,
            verdict
        );
    }
    let summary = counts
        .iter()
        .map(|(verdict, count)| format!("{verdict}={count}"))
        .collect::<Vec<_>>()
        .join(", ");
    eprintln!("pull dry-run finished: {total} repositories");
    eprintln!("summary: {summary}");
    Ok(())
}

fn dry_run_verdict(
    database: &Database,
    roots: &[Root],
    repo: &RepoRecord,
    fetch: bool,
) -> Result<String> {
    let policy = resolve_repo_policy(database, repo)?;
    if !xingshu_core::policy::is_update_enabled(&policy.pull_strategy)
        || repo.clone_status == "broken"
    {
        return Ok(format!("skipped (strategy={})", policy.pull_strategy));
    }
    let Some(root) = roots.iter().find(|root| root.id == Some(repo.root_id)) else {
        return Err(anyhow!("root {} not found", repo.root_id));
    };
    let path = root.path.join(&repo.rel_path);
    let metadata = xingshu_core::git::inspect(&path).map_err(|error| anyhow!(error.to_string()))?;
    if metadata.dirty {
        return Ok("conflict(dirty) - 需人工决策".to_owned());
    }
    if !fetch {
        return Ok(if metadata.ahead_of_upstream {
            "ahead locally (remote not verified)".to_owned()
        } else {
            "clean (remote not verified; add --fetch)".to_owned()
        });
    }
    if let Err(error) = xingshu_core::git::run_git(&path, &["fetch", "origin", "--prune"]) {
        return Ok(format!("unknown (fetch failed: {error})"));
    }
    Ok(match xingshu_core::git::divergence(&path) {
        Some(info) if info.behind == 0 && info.ahead > 0 => {
            format!("ahead {} (no upstream update)", info.ahead)
        }
        Some(info) if info.ahead == 0 && info.behind == 0 => "clean (up to date)".to_owned(),
        Some(info) => format!(
            "diverged/non-ff (ahead {}, behind {}) - 需决策",
            info.ahead, info.behind
        ),
        None => "clean (remote state unknown)".to_owned(),
    })
}

fn report_pull(
    database: &Database,
    roots: &[Root],
    repo: &RepoRecord,
    mode: PullMode,
    progress: Option<(usize, usize)>,
) -> Result<()> {
    let Some(root) = roots.iter().find(|root| root.id == Some(repo.root_id)) else {
        return Err(anyhow!("root {} not found", repo.root_id));
    };
    let path = root.path.join(&repo.rel_path);
    let policy = resolve_repo_policy(database, repo)?;
    let operation_id = format!("pull-{}", repo.id.unwrap_or_default());
    let started = Instant::now();
    let applicable_action = match mode {
        PullMode::Interactive => policy.conflict_action,
        PullMode::Unattended => policy.unattended_action,
    };
    let outcome = match pull_repo(&path, repo, &policy, mode) {
        Ok(outcome) => Some(outcome),
        Err(error)
            if matches!(error, VcsError::ConflictNeedsDecision(_))
                && matches!(mode, PullMode::Interactive) =>
        {
            let chosen = prompt_conflict_choice(&path);
            if chosen.is_none() {
                eprintln!(
                    "  非交互输入，按 unattended 策略处理：{}",
                    xingshu_core::policy::action_value(policy.unattended_action)
                );
            }
            let action = chosen.unwrap_or(policy.unattended_action);
            if matches!(action, PullConflictAction::Stop) {
                eprintln!("  unattended 策略为 `stop`，保留冲突交由人工处理");
                None
            } else {
                match xingshu_core::puller::resolve_pull_conflict(&path, repo, action) {
                    Ok(outcome) => Some(outcome),
                    Err(error) => {
                        report_pull_failure(
                            database,
                            repo,
                            &path,
                            progress,
                            &operation_id,
                            action,
                            &policy.pull_strategy,
                            started,
                            &error.to_string(),
                        )?;
                        None
                    }
                }
            }
        }
        Err(error) => {
            report_pull_failure(
                database,
                repo,
                &path,
                progress,
                &operation_id,
                applicable_action,
                &policy.pull_strategy,
                started,
                &error.to_string(),
            )?;
            None
        }
    };
    if let Some(outcome) = outcome {
        let log = make_fetch_log(repo.id.unwrap_or_default(), &policy.pull_strategy, &outcome);
        database
            .record_fetch_log(&log)
            .map_err(|error| anyhow!(error.to_string()))?;
        database
            .update_pull_status(repo.id.unwrap_or_default(), &outcome.result)
            .map_err(|error| anyhow!(error.to_string()))?;
        if outcome.result == "ok" {
            let _ = database.set_lock_violation(repo.id.unwrap_or_default(), false);
        }
        tracing::info!(
            operation_id = %operation_id,
            repo_id = repo.id.unwrap_or_default(),
            result = %outcome.result,
            duration_ms = started.elapsed().as_millis() as u64,
            "pull worker completed"
        );
        let duration_ms = started.elapsed().as_millis();
        match progress {
            Some((position, total)) => println!(
                "[{position}/{total}] {}/{}: {} ({duration_ms}ms)",
                repo.org, repo.name, outcome.result
            ),
            None => println!(
                "{}/{}: {} ({duration_ms}ms)",
                repo.org, repo.name, outcome.result
            ),
        }
    }
    Ok(())
}

/// Interactive conflict prompt for CLI pulls (PLAN-297 ISS-01).
///
/// Returns `None` when stdin is not a terminal (or input ends), which makes
/// the caller fall back to the unattended policy instead of hanging.
fn prompt_conflict_choice(path: &std::path::Path) -> Option<PullConflictAction> {
    use std::io::{IsTerminal, Write};
    if !std::io::stdin().is_terminal() {
        return None;
    }
    eprintln!("\n冲突：{}", path.display());
    eprintln!("  请选择处理方式：");
    eprintln!("    [b] 备份后重克隆（零丢失，推荐）");
    eprintln!("    [o] 覆盖本地（丢弃全部本地变更，不可恢复）");
    eprintln!("    [s] 保持现状（跳过本次拉取）");
    loop {
        eprint!("  选择 (b/o/s): ");
        let _ = std::io::stderr().flush();
        let mut line = String::new();
        match std::io::stdin().read_line(&mut line) {
            Ok(0) | Err(_) => return None,
            Ok(_) => {}
        }
        match line.trim().to_ascii_lowercase().as_str() {
            "b" | "backup" => return Some(PullConflictAction::Backup),
            "s" | "skip" | "a" | "abort" => return Some(PullConflictAction::Skip),
            "o" | "overwrite" => {
                eprint!("  覆盖会丢弃本地未提交变更且不可恢复，确认？(y/N): ");
                let _ = std::io::stderr().flush();
                let mut confirm = String::new();
                if std::io::stdin().read_line(&mut confirm).is_ok()
                    && matches!(confirm.trim().to_ascii_lowercase().as_str(), "y" | "yes")
                {
                    return Some(PullConflictAction::Overwrite);
                }
                eprintln!("  已取消覆盖，请重新选择。");
            }
            _ => eprintln!("  无效输入，请输入 b、o 或 s。"),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn report_pull_failure(
    database: &Database,
    repo: &RepoRecord,
    path: &std::path::Path,
    progress: Option<(usize, usize)>,
    operation_id: &str,
    action: PullConflictAction,
    strategy: &str,
    started: Instant,
    error_text: &str,
) -> Result<()> {
    tracing::error!(
        operation_id = %operation_id,
        repo_id = repo.id.unwrap_or_default(),
        duration_ms = started.elapsed().as_millis() as u64,
        error = %error_text,
        "pull worker failed"
    );
    match progress {
        Some((position, total)) => {
            eprintln!("[{position}/{total}] {}: {error_text}", path.display());
        }
        None => eprintln!("{}: {error_text}", path.display()),
    }
    if matches!(action, PullConflictAction::Backup) {
        let log = xingshu_core::puller::make_failed_fetch_log(
            repo.id.unwrap_or_default(),
            strategy,
            "backup_failed",
            error_text,
        );
        database
            .record_fetch_log(&log)
            .map_err(|error| anyhow!(error.to_string()))?;
        database
            .update_pull_status(repo.id.unwrap_or_default(), "backup_failed")
            .map_err(|error| anyhow!(error.to_string()))?;
        eprintln!("  backup 失败：已回滚原仓库并写入 `backup_failed` 审计");
    }
    Ok(())
}

fn move_repo(database: &Database, args: MoveArgs) -> Result<()> {
    let repo = database
        .find_repo(&args.repo)
        .map_err(|error| anyhow!(error.to_string()))?
        .ok_or_else(|| anyhow!("repository not found"))?;
    let roots = database
        .list_roots()
        .map_err(|error| anyhow!(error.to_string()))?;
    let source_root = roots
        .iter()
        .find(|root| root.id == Some(repo.root_id))
        .ok_or_else(|| anyhow!("source root not found"))?;
    let target_root = roots
        .iter()
        .find(|root| root.id == Some(args.root))
        .ok_or_else(|| anyhow!("target root not found"))?;
    let source = source_root.path.join(&repo.rel_path);
    let destination = xingshu_core::mover::move_repo(
        &source_root.path,
        &source,
        &target_root.path,
        &repo.rel_path,
    )
    .map_err(|error| anyhow!(error.to_string()))?;
    database
        .update_repo_location(
            repo.id.ok_or_else(|| anyhow!("repository has no id"))?,
            args.root,
            &repo.rel_path,
        )
        .map_err(|error| anyhow!(error.to_string()))?;
    println!("moved to {}", destination.display());
    Ok(())
}

fn stats(database: &Database, args: StatsArgs) -> Result<()> {
    let repos = database
        .list_repos()
        .map_err(|error| anyhow!(error.to_string()))?;
    let total: u64 = repos.iter().map(|repo| repo.size_bytes).sum();
    println!("repositories: {}\nbytes: {total}", repos.len());
    let Some(by) = args.by else {
        return Ok(());
    };
    let mut groups = std::collections::BTreeMap::<String, (usize, u64)>::new();
    match by.as_str() {
        "root" => {
            let roots = database
                .list_roots()
                .map_err(|error| anyhow!(error.to_string()))?;
            for repo in &repos {
                let label = roots
                    .iter()
                    .find(|root| root.id == Some(repo.root_id))
                    .map(|root| format!("{} ({})", root.name, root.path.display()))
                    .unwrap_or_else(|| format!("root#{}", repo.root_id));
                let entry = groups.entry(label).or_insert((0, 0));
                entry.0 += 1;
                entry.1 += repo.size_bytes;
            }
        }
        "org" => {
            for repo in &repos {
                let entry = groups.entry(repo.org.clone()).or_insert((0, 0));
                entry.0 += 1;
                entry.1 += repo.size_bytes;
            }
        }
        "tag" => {
            println!("(note: repositories with multiple tags are counted in every matching group)");
            for repo in &repos {
                for tag in database
                    .tags_for_repo(repo.id.unwrap_or_default())
                    .map_err(|error| anyhow!(error.to_string()))?
                {
                    let entry = groups.entry(tag.clone()).or_insert((0, 0));
                    entry.0 += 1;
                    entry.1 += repo.size_bytes;
                }
            }
        }
        other => return Err(anyhow!("unsupported dimension: {other}")),
    }
    let mut rows = groups.into_iter().collect::<Vec<_>>();
    rows.sort_by_key(|row| std::cmp::Reverse(row.1.1));
    println!("\nby {by}:");
    for (label, (count, bytes)) in rows {
        println!("  {label}: {count} repos, {bytes} bytes");
    }
    Ok(())
}

/// Read-only status report (PLAN-297 ISS-02B).
///
/// Local columns come from the last scan (no writes, no fetch). `--remote`
/// additionally queries the real remote with `git ls-remote`, which is also
/// strictly read-only and never updates local refs; failures are reported as
/// `unknown` instead of aborting the whole report.
fn status(database: &Database, args: StatusArgs) -> Result<()> {
    let roots = database
        .list_roots()
        .map_err(|error| anyhow!(error.to_string()))?;
    let mut repos = database
        .list_repos()
        .map_err(|error| anyhow!(error.to_string()))?;
    if let Some(root_id) = args.root {
        if !roots.iter().any(|root| root.id == Some(root_id)) {
            return Err(anyhow!("root not found: {root_id}"));
        }
        repos.retain(|repo| repo.root_id == root_id);
    }
    if let Some(query) = args.repo.as_deref() {
        let repo = database
            .find_repo(query)
            .map_err(|error| anyhow!(error.to_string()))?
            .ok_or_else(|| anyhow!("repository not found: {query}"))?;
        repos.retain(|candidate| candidate.id == repo.id);
    }
    if repos.is_empty() {
        println!("no repositories matched");
        return Ok(());
    }
    println!(
        "{:<30} {:<7} {:<7} {:<7} {:<20} remote",
        "repository", "dirty", "ahead", "behind", "checked_at"
    );
    for repo in &repos {
        let checked = repo
            .status_checked_at
            .map(|time| time.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| "-".to_owned());
        let remote_state = if args.remote {
            remote_state(&roots, repo)
        } else {
            "-".to_owned()
        };
        println!(
            "{:<30} {:<7} {:<7} {:<7} {:<20} {}",
            format!("{}/{}", repo.org, repo.name),
            repo.dirty,
            repo.ahead,
            repo.behind,
            checked,
            remote_state
        );
    }
    Ok(())
}

fn remote_state(roots: &[Root], repo: &RepoRecord) -> String {
    let Some(root) = roots.iter().find(|root| root.id == Some(repo.root_id)) else {
        return "unknown (root not found)".to_owned();
    };
    let path = root.path.join(&repo.rel_path);
    match xingshu_core::git::remote_refs(&path, "origin") {
        Ok(refs) => {
            let Some(branch) = repo.default_branch.as_deref() else {
                return format!("{} remote branches (unknown default branch)", refs.len());
            };
            let Some((_, remote_sha)) = refs
                .iter()
                .find(|(name, _)| name == &format!("refs/heads/{branch}"))
            else {
                return format!(
                    "{} remote branches (default branch not advertised)",
                    refs.len()
                );
            };
            let local = xingshu_core::git::run_git(
                &path,
                &[
                    "rev-parse",
                    "--verify",
                    &format!("refs/remotes/origin/{branch}"),
                ],
            )
            .ok()
            .map(|output| output.stdout.trim().to_owned());
            match local {
                Some(local_sha) if &local_sha == remote_sha => {
                    "up-to-date (last fetch matches remote)".to_owned()
                }
                Some(_) => "changed (remote moved; fetch/pull needed)".to_owned(),
                None => "unknown (no local tracking ref; needs fetch)".to_owned(),
            }
        }
        Err(error) => format!("unknown ({error})"),
    }
}

fn repo_absolute_path(database: &Database, repo: &RepoRecord) -> Result<PathBuf> {
    let roots = database
        .list_roots()
        .map_err(|error| anyhow!(error.to_string()))?;
    let root = roots
        .iter()
        .find(|root| root.id == Some(repo.root_id))
        .ok_or_else(|| anyhow!("root {} not found", repo.root_id))?;
    Ok(root.path.join(&repo.rel_path))
}

/// Independent backup create/list/restore (PLAN-297 ISS-03).
fn run_backup(database: &Database, args: BackupArgs) -> Result<()> {
    match args.command {
        BackupCommand::Create {
            repo,
            dest,
            archive,
            keep,
        } => {
            let repo = database
                .find_repo(&repo)
                .map_err(|error| anyhow!(error.to_string()))?
                .ok_or_else(|| anyhow!("repository not found"))?;
            let path = repo_absolute_path(database, &repo)?;
            let created = xingshu_core::backup::create_backup(
                &path,
                dest.as_deref(),
                archive.as_deref(),
                keep,
            )
            .map_err(|error| anyhow!(error.to_string()))?;
            println!("backup created: {}", created.display());
        }
        BackupCommand::List { repo, dest } => {
            let repo = database
                .find_repo(&repo)
                .map_err(|error| anyhow!(error.to_string()))?
                .ok_or_else(|| anyhow!("repository not found"))?;
            let path = repo_absolute_path(database, &repo)?;
            let entries = xingshu_core::backup::list_backups(&path, dest.as_deref())
                .map_err(|error| anyhow!(error.to_string()))?;
            if entries.is_empty() {
                println!("no backups");
                return Ok(());
            }
            for entry in entries {
                println!(
                    "{}  {}  {}",
                    entry.created_at,
                    if entry.is_archive { "archive" } else { "dir" },
                    entry.path.display()
                );
            }
        }
        BackupCommand::Restore { repo, backup } => {
            let repo = database
                .find_repo(&repo)
                .map_err(|error| anyhow!(error.to_string()))?
                .ok_or_else(|| anyhow!("repository not found"))?;
            let path = repo_absolute_path(database, &repo)?;
            xingshu_core::backup::restore_backup(&path, &backup)
                .map_err(|error| anyhow!(error.to_string()))?;
            println!("restored {} from {}", path.display(), backup.display());
        }
        BackupCommand::Prune {
            repo,
            dest,
            keep,
            execute,
        } => {
            let repo = database
                .find_repo(&repo)
                .map_err(|error| anyhow!(error.to_string()))?
                .ok_or_else(|| anyhow!("repository not found"))?;
            let path = repo_absolute_path(database, &repo)?;
            let affected =
                xingshu_core::backup::prune_backups(&path, dest.as_deref(), keep, execute)
                    .map_err(|error| anyhow!(error.to_string()))?;
            if affected.is_empty() {
                println!("no backups to prune (keep={keep})");
                return Ok(());
            }
            for entry in &affected {
                println!(
                    "{} {}",
                    if execute { "deleted" } else { "would delete" },
                    entry.display()
                );
            }
            if !execute {
                eprintln!(
                    "dry-run: {} entries would be deleted; add --execute to remove",
                    affected.len()
                );
            }
        }
    }
    Ok(())
}
