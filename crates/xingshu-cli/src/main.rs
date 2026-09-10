use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use anyhow::{Result, anyhow};
use chrono::Utc;
use clap::{Args, Parser, Subcommand};
use xingshu_core::ProgressReporter;
use xingshu_core::db::Database;
use xingshu_core::policy::default_for_kind;
use xingshu_core::puller::{PullMode, make_fetch_log, pull_repo};
use xingshu_core::scanner::scan_roots_with_reporter;
use xingshu_core::types::{Policy as RepoPolicy, RepoKind, RepoRecord, Root, ScanOptions};

#[derive(Debug, Parser)]
#[command(
    name = "xingshu",
    version,
    about = "Local Git repository index and workspace manager"
)]
struct Cli {
    #[arg(long, env = "XINGSHU_DB", global = true)]
    db: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
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
    Stats,
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
        #[arg(long, default_value = "abort")]
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

fn main() -> Result<()> {
    init_logging();
    let cli = Cli::parse();
    let db_path = xingshu_core::config::resolve_db_path(cli.db).map_err(|error| anyhow!(error))?;
    xingshu_core::config::check_db_allowed(&db_path).map_err(|error| anyhow!(error))?;
    let database = Database::open(&db_path).map_err(|error| anyhow!(error.to_string()))?;
    match cli.command {
        Command::Roots { command } => roots(&database, command),
        Command::Scan(args) => scan(&database, args),
        Command::List(args) => list(&database, args),
        Command::Tag(args) => tag(&database, args, true),
        Command::Untag(args) => tag(&database, args, false),
        Command::Kind(args) => kind(&database, args),
        Command::Policy(args) => policy(&database, args),
        Command::Pull(args) => pull(&database, args),
        Command::Move(args) => move_repo(&database, args),
        Command::Stats => stats(&database),
    }
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
            .with_target(true)
            .with_env_filter(filter)
            .try_init();
    }
}

fn roots(database: &Database, command: RootCommand) -> Result<()> {
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
            database
                .remove_root(id)
                .map_err(|error| anyhow!(error.to_string()))?;
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
        "pull batch requested"
    );
    let roots = database
        .list_roots()
        .map_err(|error| anyhow!(error.to_string()))?;
    let repos = database
        .list_repos()
        .map_err(|error| anyhow!(error.to_string()))?;
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
    let policy = match stored_policy {
        Some(stored) => xingshu_core::policy::ResolvedPolicy {
            pull_strategy: stored.pull_strategy,
            conflict_action: xingshu_core::policy::PullConflictAction::try_from(
                stored.pull_conflict_policy.as_str(),
            )
            .map_err(|error| anyhow!(error.to_string()))?,
            unattended_action: xingshu_core::policy::PullConflictAction::try_from(
                stored.unattended_conflict_policy.as_str(),
            )
            .map_err(|error| anyhow!(error.to_string()))?,
        },
        None => default_for_kind(&repo.repo_kind),
    };
    let operation_id = format!("pull-{}", repo.id.unwrap_or_default());
    let started = Instant::now();
    match pull_repo(&path, repo, &policy, mode) {
        Ok(outcome) => {
            let log = make_fetch_log(repo.id.unwrap_or_default(), &policy.pull_strategy, &outcome);
            database
                .record_fetch_log(&log)
                .map_err(|error| anyhow!(error.to_string()))?;
            database
                .update_pull_status(repo.id.unwrap_or_default(), &outcome.result)
                .map_err(|error| anyhow!(error.to_string()))?;
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
                None => println!("{}/{}: {} ({duration_ms}ms)", repo.org, repo.name, outcome.result),
            }
        }
        Err(error) => {
            tracing::error!(
                operation_id = %operation_id,
                repo_id = repo.id.unwrap_or_default(),
                duration_ms = started.elapsed().as_millis() as u64,
                error = %error,
                "pull worker failed"
            );
            match progress {
                Some((position, total)) => {
                    eprintln!("[{position}/{total}] {}: {error}", path.display());
                }
                None => eprintln!("{}: {error}", path.display()),
            }
        }
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

fn stats(database: &Database) -> Result<()> {
    let repos = database
        .list_repos()
        .map_err(|error| anyhow!(error.to_string()))?;
    let total: u64 = repos.iter().map(|repo| repo.size_bytes).sum();
    println!("repositories: {}\nbytes: {total}", repos.len());
    Ok(())
}
