use crate::{type_extensions, types::VcsError};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use regex::{Regex, RegexBuilder};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{self, Read},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
};

use super::search_index::{
    ContextRow, FilePathResult, FilesOptions, FilesOutput, FilesSummary, IndexService, ReadOptions,
    ReadOutput, SearchContext, SearchError, SearchOptions, SearchOutput, SearchRepo, SearchResult,
    SearchSummary, current_head, repo_path, run_git, text_path_allowed,
};

#[derive(Debug)]
struct PathGlobs {
    include: GlobSet,
    exclude: GlobSet,
    has_include: bool,
}

impl PathGlobs {
    fn compile(patterns: &[String]) -> Result<Self, VcsError> {
        let mut include = GlobSetBuilder::new();
        let mut exclude = GlobSetBuilder::new();
        let mut has_include = false;
        for raw in patterns {
            let (is_exclude, pattern) = raw
                .strip_prefix('!')
                .map(|value| (true, value))
                .unwrap_or((false, raw.as_str()));
            if pattern.is_empty() {
                return Err(VcsError::GitCommand("empty path glob".into()));
            }
            let glob = GlobBuilder::new(pattern)
                .literal_separator(true)
                .case_insensitive(false)
                .build()
                .map_err(|error| VcsError::GitCommand(format!("invalid glob {raw:?}: {error}")))?;
            if is_exclude {
                exclude.add(glob);
            } else {
                has_include = true;
                include.add(glob);
            }
        }
        Ok(Self {
            include: include
                .build()
                .map_err(|error| VcsError::GitCommand(error.to_string()))?,
            exclude: exclude
                .build()
                .map_err(|error| VcsError::GitCommand(error.to_string()))?,
            has_include,
        })
    }

    fn matches(&self, path: &str) -> bool {
        let path = path.replace('\\', "/");
        (!self.has_include || self.include.is_match(&path)) && !self.exclude.is_match(&path)
    }
}

#[derive(Debug)]
enum ContentError {
    TooLarge,
    ContainsNul,
    InvalidUtf8,
    Io(io::Error),
}

impl IndexService {
    pub fn search_indexed(
        &self,
        repos: &[SearchRepo],
        options: &SearchOptions,
    ) -> Result<SearchOutput, VcsError> {
        validate_types(&options.types)?;
        if repos.is_empty() {
            return Ok(SearchOutput {
                summary: SearchSummary {
                    source: "index".into(),
                    complete: true,
                    max_results: Some(options.max_results.unwrap_or(1_000)),
                    ..SearchSummary::default()
                },
                ..SearchOutput::default()
            });
        }
        let statuses = self.status(repos)?;
        let mut indexed_repos = Vec::with_capacity(repos.len());
        for (repo, status) in repos.iter().zip(statuses) {
            if status.state != "ready" {
                let code = if status.state == "missing" {
                    "index_missing"
                } else {
                    "index_stale"
                };
                return Err(VcsError::Database(format!(
                    "{code}: repo {} is {}; run `xs index update --repo {}/{} --root {}`",
                    repo_name(repo),
                    status.state,
                    repo.repo.org,
                    repo.repo.name,
                    repo.repo.root_id
                )));
            }
            let mut indexed = repo.clone();
            indexed.repo.head_commit = status.indexed_commit;
            indexed_repos.push(indexed);
        }
        let candidates = if is_safe_fts_literal(options) {
            self.indexed_docs_for_literal(&indexed_repos, &options.pattern)?
        } else {
            self.indexed_docs(&indexed_repos)?
        };
        collect_hits(candidates, options, "index")
    }

    pub fn files_indexed(
        &self,
        repos: &[SearchRepo],
        options: &FilesOptions,
    ) -> Result<FilesOutput, VcsError> {
        validate_types(&options.types)?;
        let mut patterns = options.globs.clone();
        if let Some(pattern) = options.pattern.as_ref() {
            patterns.push(pattern.clone());
        }
        let globs = PathGlobs::compile(&patterns)?;
        let max_results = options.max_results.unwrap_or(10_000);
        let mut output = FilesOutput {
            summary: FilesSummary {
                source: "index".into(),
                complete: true,
                max_results: Some(max_results),
                ..FilesSummary::default()
            },
            ..FilesOutput::default()
        };
        let statuses = self.status(repos)?;
        let mut seen_repos = HashSet::new();
        for (repo, status) in repos.iter().zip(statuses) {
            if status.state != "ready" {
                output.summary.complete = false;
                let code = if status.state == "missing" {
                    "index_missing"
                } else {
                    "index_stale"
                };
                output.errors.push(SearchError {
                    code: code.into(),
                    message: format!("index is {} for {}", status.state, repo_name(repo)),
                    repo: Some(repo_name(repo)),
                    root_id: Some(repo.root.id.unwrap_or(repo.repo.root_id)),
                });
                continue;
            }
            let paths = match self.paths_for_repo(repo) {
                Ok(paths) => paths,
                Err(error) => {
                    output.summary.complete = false;
                    output.errors.push(SearchError {
                        code: "index_read_failed".into(),
                        message: error.to_string(),
                        repo: Some(repo_name(repo)),
                        root_id: Some(repo.root.id.unwrap_or(repo.repo.root_id)),
                    });
                    continue;
                }
            };
            for path in paths {
                if !path_matches(&path, &options.types, &globs) {
                    continue;
                }
                if output.results.len() >= max_results {
                    output.summary.truncated = true;
                    output.summary.complete = false;
                    break;
                }
                seen_repos.insert(repo.repo.id);
                output.results.push(FilePathResult {
                    repo_id: repo.repo.id.unwrap_or_default(),
                    repo_name: repo.repo.name.clone(),
                    repo: repo_name(repo),
                    root_id: repo.root.id.unwrap_or(repo.repo.root_id),
                    path,
                    revision: status.indexed_commit.clone(),
                });
            }
            if output.summary.truncated {
                break;
            }
        }
        output.summary.files = output.results.len() as u64;
        output.summary.repos = seen_repos.len() as u64;
        Ok(output)
    }

    pub fn grep_worktree(
        &self,
        repos: &[SearchRepo],
        options: &SearchOptions,
    ) -> Result<SearchOutput, VcsError> {
        validate_types(&options.types)?;
        let globs = PathGlobs::compile(&options.globs)?;
        let mut output = SearchOutput {
            summary: SearchSummary {
                source: "worktree".into(),
                complete: true,
                max_results: Some(options.max_results.unwrap_or(1000)),
                files_with_matches: options.files_with_matches,
                count_mode: options.count,
                ..SearchSummary::default()
            },
            ..SearchOutput::default()
        };
        for repo in repos {
            let root = repo_path(repo);
            let states = match worktree_states(&root) {
                Ok(states) => states,
                Err(error) => {
                    push_service_error(
                        &mut output,
                        repo,
                        "worktree_scan_failed",
                        error.to_string(),
                    );
                    continue;
                }
            };
            let mut scan_repo = repo.clone();
            scan_repo.repo.head_commit = current_head(&root).ok();
            let root_canonical = match fs::canonicalize(&root) {
                Ok(path) => path,
                Err(error) => {
                    record_scan_error(&mut output, repo, ".", error.kind());
                    continue;
                }
            };
            for (path, file_state) in states {
                if !path_matches(&path, &options.types, &globs) {
                    skip_file(&mut output);
                    continue;
                }
                if !text_path_allowed(&path) {
                    skip_file(&mut output);
                    continue;
                }
                let full_path = root.join(&path);
                let metadata = match fs::symlink_metadata(&full_path) {
                    Ok(metadata) => metadata,
                    Err(error) => {
                        record_scan_error(&mut output, repo, &path, error.kind());
                        continue;
                    }
                };
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    skip_file(&mut output);
                    continue;
                }
                if metadata.len() > type_extensions::MAX_FILE_BYTES as u64 {
                    skip_file(&mut output);
                    continue;
                }
                let canonical = match fs::canonicalize(&full_path) {
                    Ok(path) => path,
                    Err(error) => {
                        record_scan_error(&mut output, repo, &path, error.kind());
                        continue;
                    }
                };
                if !canonical.starts_with(&root_canonical) {
                    skip_file(&mut output);
                    continue;
                }
                let content = match fs::read(&canonical) {
                    Ok(content) => content,
                    Err(error) => {
                        record_scan_error(&mut output, repo, &path, error.kind());
                        continue;
                    }
                };
                if content.len() > type_extensions::MAX_FILE_BYTES || content.contains(&0) {
                    skip_file(&mut output);
                    continue;
                }
                let content = match String::from_utf8(content) {
                    Ok(content) => content,
                    Err(_) => {
                        skip_file(&mut output);
                        continue;
                    }
                };
                collect_file_hits(
                    &mut output,
                    &scan_repo,
                    &path,
                    &content,
                    options,
                    "worktree",
                    file_state,
                )?;
                if output.summary.truncated {
                    break;
                }
            }
            if output.summary.truncated {
                break;
            }
        }
        finalize_summary(&mut output);
        Ok(output)
    }

    pub fn grep_ref(
        &self,
        repo: &SearchRepo,
        git_ref: &str,
        options: &SearchOptions,
    ) -> Result<SearchOutput, VcsError> {
        validate_types(&options.types)?;
        let globs = PathGlobs::compile(&options.globs)?;
        let root = repo_path(repo);
        let resolved = String::from_utf8(run_git(
            &root,
            &["rev-parse", "--verify", &format!("{git_ref}^{{commit}}")],
        )?)
        .map_err(|error| VcsError::GitCommand(error.to_string()))?
        .trim()
        .to_owned();
        let tree = run_git(
            &root,
            &["ls-tree", "-r", "-l", "-z", "--full-tree", &resolved],
        )?;
        let mut output = SearchOutput {
            summary: SearchSummary {
                source: "git-ref".into(),
                complete: true,
                max_results: Some(options.max_results.unwrap_or(1000)),
                files_with_matches: options.files_with_matches,
                count_mode: options.count,
                ..SearchSummary::default()
            },
            ..SearchOutput::default()
        };
        for entry in tree
            .split(|byte| *byte == 0)
            .filter(|entry| !entry.is_empty())
        {
            let Some(tab) = entry.iter().position(|byte| *byte == b'\t') else {
                continue;
            };
            let meta = String::from_utf8_lossy(&entry[..tab]);
            let mut fields = meta.split_whitespace();
            let mode = fields.next().unwrap_or_default();
            let kind = fields.next().unwrap_or_default();
            let oid = fields.next().unwrap_or_default();
            let size = fields.next().and_then(|value| value.parse::<u64>().ok());
            let path = String::from_utf8_lossy(&entry[tab + 1..]).into_owned();
            if kind != "blob" || mode == "120000" || !path_matches(&path, &options.types, &globs) {
                skip_file(&mut output);
                continue;
            }
            if !text_path_allowed(&path)
                || size.is_none_or(|size| size > type_extensions::MAX_FILE_BYTES as u64)
            {
                skip_file(&mut output);
                continue;
            }
            let oid = oid.to_owned();
            let content = match read_blob_content(&root, &oid) {
                Ok(content) => content,
                Err(
                    ContentError::TooLarge | ContentError::ContainsNul | ContentError::InvalidUtf8,
                ) => {
                    skip_file(&mut output);
                    continue;
                }
                Err(ContentError::Io(error)) => {
                    record_scan_error(&mut output, repo, &path, error.kind());
                    continue;
                }
            };
            collect_file_hits(
                &mut output,
                repo,
                &path,
                &content,
                options,
                "git-ref",
                "committed",
            )?;
            for result in output
                .results
                .iter_mut()
                .rev()
                .take_while(|result| result.path == path)
            {
                result.revision = Some(resolved.clone());
            }
            if output.summary.truncated {
                break;
            }
        }
        finalize_summary(&mut output);
        Ok(output)
    }

    pub fn read(&self, repo: &SearchRepo, options: &ReadOptions) -> Result<ReadOutput, VcsError> {
        let root = repo_path(repo);
        let path = validate_read_path(&options.path)?;
        let path_text = path.to_string_lossy().replace('\\', "/");
        if !text_path_allowed(&path_text) {
            let extension = Path::new(&path_text)
                .extension()
                .and_then(|extension| extension.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            let message = if type_extensions::is_binary_extension(&extension) {
                "binary file extension is not readable"
            } else {
                "generated path is not readable"
            };
            return Err(VcsError::Filesystem(message.into()));
        }
        let source = if options.revision.is_some() {
            "git-ref"
        } else {
            "head"
        };
        let requested = options.revision.as_deref().unwrap_or("HEAD");
        let revision_expr = format!("{requested}^{{commit}}");
        let revision = String::from_utf8(run_git(
            &root,
            &["rev-parse", "--verify", "--end-of-options", &revision_expr],
        )?)
        .map_err(|error| VcsError::GitCommand(error.to_string()))?
        .trim()
        .to_owned();
        let content = read_revision_blob(&root, &revision, &path_text)?;
        let all_lines: Vec<&str> = content.lines().collect();
        let start_line = options.start_line.unwrap_or(1).max(1);
        let max_lines = options.max_lines.unwrap_or(200);
        let selected: Vec<&str> = all_lines
            .iter()
            .skip(start_line - 1)
            .take(max_lines)
            .copied()
            .collect();
        let end_line = if selected.is_empty() {
            start_line.saturating_sub(1)
        } else {
            start_line + selected.len() - 1
        };
        Ok(ReadOutput {
            repo_id: repo.repo.id.unwrap_or_default(),
            repo_name: repo.repo.name.clone(),
            repo: repo_name(repo),
            root_id: repo.root.id.unwrap_or(repo.repo.root_id),
            revision: Some(revision),
            path: path.to_string_lossy().replace('\\', "/"),
            start_line,
            end_line,
            content: selected.join("\n"),
            truncated: start_line.saturating_sub(1) + selected.len() < all_lines.len(),
            source: source.into(),
        })
    }
}

fn collect_hits(
    docs: Vec<(SearchRepo, String, Option<String>)>,
    options: &SearchOptions,
    source: &str,
) -> Result<SearchOutput, VcsError> {
    let globs = PathGlobs::compile(&options.globs)?;
    let mut output = SearchOutput {
        summary: SearchSummary {
            source: source.into(),
            complete: true,
            max_results: Some(options.max_results.unwrap_or(1000)),
            files_with_matches: options.files_with_matches,
            count_mode: options.count,
            ..SearchSummary::default()
        },
        ..SearchOutput::default()
    };
    for (repo, path, body) in docs {
        if !path_matches(&path, &options.types, &globs) {
            continue;
        }
        let Some(body) = body else {
            skip_file(&mut output);
            continue;
        };
        collect_file_hits(&mut output, &repo, &path, &body, options, source, "tracked")?;
        if output.summary.truncated {
            break;
        }
    }
    finalize_summary(&mut output);
    Ok(output)
}

fn collect_file_hits(
    output: &mut SearchOutput,
    repo: &SearchRepo,
    path: &str,
    body: &str,
    options: &SearchOptions,
    source: &str,
    file_state: &str,
) -> Result<(), VcsError> {
    if !text_path_allowed(path) {
        skip_file(output);
        return Ok(());
    }
    let matcher = make_matcher(options)?;
    let lines: Vec<&str> = body.lines().collect();
    let mut hits = Vec::new();
    for (index, text) in lines.iter().enumerate() {
        if let Some(column) = find_match(text, options, &matcher) {
            hits.push((index, column));
        }
    }
    let max_count = options.max_count.unwrap_or(hits.len()).min(hits.len());
    let hits = &hits[..max_count];
    if hits.is_empty() {
        return Ok(());
    }
    let result_limit = options.max_results.unwrap_or(1000);
    if options.count {
        if output.results.len() >= result_limit {
            output.summary.truncated = true;
            output.summary.complete = false;
            return Ok(());
        }
        output.summary.matched_lines += hits.len() as u64;
        output.results.push(make_result(
            repo,
            path,
            source,
            file_state,
            hits[0].0 + 1,
            1,
            String::new(),
            Some(hits.len()),
            None,
        ));
    } else if options.files_with_matches {
        if output.results.len() >= result_limit {
            output.summary.truncated = true;
            output.summary.complete = false;
            return Ok(());
        }
        output.summary.matched_lines += hits.len() as u64;
        let (line_index, column) = hits[0];
        output.results.push(make_result(
            repo,
            path,
            source,
            file_state,
            line_index + 1,
            column,
            lines[line_index].to_owned(),
            Some(hits.len()),
            None,
        ));
    } else {
        for (line_index, column) in hits {
            if output.results.len() >= result_limit {
                output.summary.truncated = true;
                output.summary.complete = false;
                break;
            }
            let context = make_context(&lines, *line_index, options.context);
            output.results.push(make_result(
                repo,
                path,
                source,
                file_state,
                line_index + 1,
                *column,
                lines[*line_index].to_owned(),
                None,
                context,
            ));
            output.summary.matched_lines += 1;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn make_result(
    repo: &SearchRepo,
    path: &str,
    source: &str,
    file_state: &str,
    line: usize,
    column: usize,
    text: String,
    count: Option<usize>,
    context: Option<SearchContext>,
) -> SearchResult {
    let truncated = text.chars().count() > 4096;
    let text = if truncated {
        format!(
            "{}…[truncated]",
            text.chars().take(4084).collect::<String>()
        )
    } else {
        text
    };
    SearchResult {
        repo_id: repo.repo.id.unwrap_or_default(),
        repo_name: repo.repo.name.clone(),
        repo: repo_name(repo),
        root_id: repo.root.id.unwrap_or(repo.repo.root_id),
        path: path.to_owned(),
        revision: if source == "index" {
            repo.repo.head_commit.clone()
        } else {
            None
        },
        line,
        column,
        text,
        count,
        source: source.to_owned(),
        base_head: if source == "worktree" {
            repo.repo.head_commit.clone()
        } else {
            None
        },
        file_state: file_state.to_owned(),
        truncated,
        context,
    }
}

fn make_context(lines: &[&str], hit: usize, context: usize) -> Option<SearchContext> {
    if context == 0 {
        return None;
    }
    let before = (hit.saturating_sub(context)..hit)
        .map(|index| ContextRow {
            line: index + 1,
            text: lines[index].to_owned(),
        })
        .collect();
    let after = (hit + 1..(hit + context + 1).min(lines.len()))
        .map(|index| ContextRow {
            line: index + 1,
            text: lines[index].to_owned(),
        })
        .collect();
    Some(SearchContext { before, after })
}

fn make_matcher(options: &SearchOptions) -> Result<Option<Regex>, VcsError> {
    if options.fixed_strings {
        return Ok(None);
    }
    let pattern = if options.word_regexp {
        format!(r"\b(?:{})\b", options.pattern)
    } else {
        options.pattern.clone()
    };
    RegexBuilder::new(&pattern)
        .case_insensitive(options.ignore_case)
        .build()
        .map(Some)
        .map_err(|error| VcsError::GitCommand(format!("invalid regex: {error}")))
}

fn find_match(text: &str, options: &SearchOptions, matcher: &Option<Regex>) -> Option<usize> {
    if let Some(regex) = matcher {
        return regex
            .find(text)
            .map(|found| text[..found.start()].chars().count() + 1);
    }
    let (haystack, needle) = if options.ignore_case {
        (text.to_lowercase(), options.pattern.to_lowercase())
    } else {
        (text.to_owned(), options.pattern.clone())
    };
    haystack
        .find(&needle)
        .map(|byte| haystack[..byte].chars().count() + 1)
}

fn is_safe_fts_literal(options: &SearchOptions) -> bool {
    let pattern = options.pattern.as_str();
    pattern.chars().count() >= 3
        && !pattern
            .chars()
            .any(|character| matches!(character, '%' | '_' | '\\'))
        && (!options.ignore_case || pattern.is_ascii())
        && (options.fixed_strings
            || !pattern
                .chars()
                .any(|character| ".*+?()[]{}^$|\\".contains(character)))
}

fn validate_types(types: &[String]) -> Result<(), VcsError> {
    for kind in types {
        if type_extensions::extensions_for(kind).is_none() {
            return Err(VcsError::GitCommand(format!(
                "unsupported file type: {kind}"
            )));
        }
    }
    Ok(())
}

fn path_matches(path: &str, types: &[String], globs: &PathGlobs) -> bool {
    let extension = Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let matches_type = types.is_empty()
        || types.iter().any(|kind| {
            type_extensions::extensions_for(kind)
                .is_some_and(|extensions| extensions.contains(&extension.as_str()))
        });
    matches_type && globs.matches(path)
}

fn worktree_states(root: &Path) -> Result<Vec<(String, &'static str)>, VcsError> {
    let cached = run_git(root, &["ls-files", "--cached", "-z"])?;
    let untracked = run_git(root, &["ls-files", "--others", "--exclude-standard", "-z"])?;
    let modified = run_git(root, &["diff", "--name-only", "HEAD", "-z"])?;
    let mut states = HashMap::new();
    for raw_path in cached
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        states.insert(String::from_utf8_lossy(raw_path).into_owned(), "clean");
    }
    for raw_path in untracked
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        states.insert(String::from_utf8_lossy(raw_path).into_owned(), "untracked");
    }
    for raw_path in modified
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        let path = String::from_utf8_lossy(raw_path).into_owned();
        if let Some(state) = states.get_mut(&path) {
            *state = "modified";
        }
    }
    let mut states: Vec<_> = states.into_iter().collect();
    states.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(states)
}

fn validate_read_path(path: &str) -> Result<PathBuf, VcsError> {
    let path = Path::new(path);
    if path.as_os_str().is_empty()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(VcsError::PathOutsideRoot);
    }
    Ok(path.to_owned())
}

fn read_revision_blob(root: &Path, revision: &str, path: &str) -> Result<String, VcsError> {
    let listing = run_git(
        root,
        &[
            "--literal-pathspecs",
            "ls-tree",
            "-z",
            "--full-tree",
            revision,
            "--",
            path,
        ],
    )?;
    let record = listing
        .split(|byte| *byte == 0)
        .find(|record| !record.is_empty())
        .ok_or_else(|| VcsError::Filesystem(format!("file not found in revision: {path}")))?;
    let tab = record
        .iter()
        .position(|byte| *byte == b'\t')
        .ok_or_else(|| VcsError::GitCommand("invalid git ls-tree record".into()))?;
    let metadata = String::from_utf8_lossy(&record[..tab]);
    let mut fields = metadata.split_whitespace();
    let mode = fields.next().unwrap_or_default();
    let kind = fields.next().unwrap_or_default();
    let oid = fields.next().unwrap_or_default();
    let found_path = String::from_utf8_lossy(&record[tab + 1..]);
    if found_path != path {
        return Err(VcsError::Filesystem(format!(
            "file not found in revision: {path}"
        )));
    }
    if kind != "blob" || !matches!(mode, "100644" | "100755") {
        return Err(VcsError::Filesystem(
            "path is not a regular file in revision".into(),
        ));
    }
    read_blob_content(root, oid).map_err(|error| match error {
        ContentError::TooLarge => VcsError::Filesystem("file exceeds 5 MiB".into()),
        ContentError::ContainsNul => VcsError::Filesystem("binary content contains NUL".into()),
        ContentError::InvalidUtf8 => VcsError::Filesystem("file content is not valid UTF-8".into()),
        ContentError::Io(error) => VcsError::Filesystem(error.to_string()),
    })
}

fn read_blob_content(root: &Path, oid: &str) -> Result<String, ContentError> {
    let mut child = Command::new("git")
        .args(["cat-file", "blob", oid])
        .current_dir(root)
        .env("GIT_NO_LAZY_FETCH", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(ContentError::Io)?;
    let mut output = Vec::new();
    let stdout = child
        .stdout
        .as_mut()
        .ok_or_else(|| ContentError::Io(io::Error::other("git stdout unavailable")))?;
    stdout
        .take((type_extensions::MAX_FILE_BYTES + 1) as u64)
        .read_to_end(&mut output)
        .map_err(ContentError::Io)?;
    let too_large = output.len() > type_extensions::MAX_FILE_BYTES;
    if too_large {
        let mut drain = std::io::sink();
        if let Some(stdout) = child.stdout.as_mut() {
            io::copy(stdout, &mut drain).map_err(ContentError::Io)?;
        }
    }
    let status = child.wait().map_err(ContentError::Io)?;
    if !status.success() {
        return Err(ContentError::Io(io::Error::other("git cat-file failed")));
    }
    if too_large {
        return Err(ContentError::TooLarge);
    }
    if output.contains(&0) {
        return Err(ContentError::ContainsNul);
    }
    String::from_utf8(output).map_err(|_| ContentError::InvalidUtf8)
}

fn skip_file(output: &mut SearchOutput) {
    output.summary.skipped_files = output.summary.skipped_files.saturating_add(1);
}

fn record_scan_error(
    output: &mut SearchOutput,
    repo: &SearchRepo,
    path: &str,
    kind: io::ErrorKind,
) {
    let code = match kind {
        io::ErrorKind::PermissionDenied => "permission_denied",
        io::ErrorKind::NotFound => "file_not_found",
        _ => "file_read_failed",
    };
    output.summary.complete = false;
    output.errors.push(SearchError {
        code: code.into(),
        message: format!("{}: {path}", io::Error::from(kind)),
        repo: Some(repo_name(repo)),
        root_id: Some(repo.root.id.unwrap_or(repo.repo.root_id)),
    });
}

fn push_service_error(output: &mut SearchOutput, repo: &SearchRepo, code: &str, message: String) {
    output.summary.complete = false;
    output.errors.push(SearchError {
        code: code.into(),
        message,
        repo: Some(repo_name(repo)),
        root_id: Some(repo.root.id.unwrap_or(repo.repo.root_id)),
    });
}

fn finalize_summary(output: &mut SearchOutput) {
    output.summary.matches = output.summary.matched_lines;
    output.summary.files = output
        .results
        .iter()
        .map(|result| (result.repo_id, &result.path))
        .collect::<HashSet<_>>()
        .len() as u64;
    output.summary.repos = output
        .results
        .iter()
        .map(|result| result.repo_id)
        .collect::<HashSet<_>>()
        .len() as u64;
    if !output.errors.is_empty() {
        output.summary.complete = false;
    }
}

fn repo_name(repo: &SearchRepo) -> String {
    format!("{}/{}", repo.repo.org, repo.repo.name)
}

#[cfg(test)]
mod scan_error_tests {
    use super::{SearchOutput, SearchRepo, record_scan_error};
    use crate::types::{RepoKind, RepoRecord, Root};
    use std::{io::ErrorKind, path::PathBuf};

    #[test]
    fn permission_denied_is_a_partial_scan_error() {
        let now = chrono::Utc::now();
        let repo = SearchRepo {
            repo: RepoRecord {
                id: Some(5),
                root_id: 3,
                org: "synthetic".into(),
                name: "repo".into(),
                rel_path: PathBuf::from("synthetic/repo"),
                remote_url: None,
                remote_host: None,
                remote_namespace: None,
                org_source: None,
                default_branch: None,
                head_commit: None,
                last_fetch_at: None,
                last_pull_status: None,
                dirty: false,
                ahead: 0,
                behind: 0,
                status_checked_at: None,
                size_bytes: 0,
                is_bare: false,
                clone_status: "ready".into(),
                repo_kind: RepoKind::Own,
                modify_lock: false,
                lock_violation: false,
                push_protect_remote: None,
                scan_fingerprint: None,
                created_at: now,
                updated_at: now,
            },
            root: Root {
                id: Some(3),
                name: "root".into(),
                path: PathBuf::from("/synthetic"),
                disk_label: None,
                mount_point: None,
                priority: 0,
                created_at: now,
                updated_at: now,
            },
        };
        let mut output = SearchOutput::default();
        output.summary.complete = true;
        record_scan_error(
            &mut output,
            &repo,
            "private.txt",
            ErrorKind::PermissionDenied,
        );
        assert!(!output.summary.complete);
        assert_eq!(output.errors[0].code, "permission_denied");
        assert_eq!(output.errors[0].repo.as_deref(), Some("synthetic/repo"));
        assert_eq!(output.errors[0].root_id, Some(3));
        assert!(output.errors[0].message.contains("private.txt"));
    }
}
