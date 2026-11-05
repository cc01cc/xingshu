use anyhow::{Result, anyhow};
use xingshu_core::db::Database;
use xingshu_core::search_index::{FilesOutput, SearchOutput, SearchRepo};

pub(super) fn filter_repos_by_tag(
    database: &Database,
    repos: Vec<SearchRepo>,
    tag: Option<&str>,
) -> Result<Vec<SearchRepo>> {
    let Some(tag) = tag else { return Ok(repos) };
    if database
        .tag_id(tag)
        .map_err(|e| anyhow!(e.to_string()))?
        .is_none()
    {
        return Err(anyhow!("tag not found: {tag}"));
    }
    repos
        .into_iter()
        .filter_map(
            |repo| match database.repo_has_tag(repo.repo.id.unwrap_or_default(), tag) {
                Ok(true) => Some(Ok(repo)),
                Ok(false) => None,
                Err(error) => Some(Err(anyhow!(error.to_string()))),
            },
        )
        .collect()
}

pub(super) fn print_search_human(output: &SearchOutput) {
    for result in &output.results {
        if output.summary.files_with_matches && !output.summary.count_mode {
            if result.source == "worktree" {
                println!(
                    "{}/{}@worktree(base={},state={}):{}",
                    result.repo,
                    result.root_id,
                    result.base_head.as_deref().unwrap_or("unknown"),
                    result.file_state,
                    result.path
                );
            } else {
                println!(
                    "{}/{}@{}:{}",
                    result.repo,
                    result.root_id,
                    result.revision.as_deref().unwrap_or("unknown"),
                    result.path
                );
            }
            continue;
        }
        if let Some(count) = result.count {
            println!(
                "{}/{}@{}:{}:{count}",
                result.repo,
                result.root_id,
                result.revision.as_deref().unwrap_or("worktree"),
                result.path
            );
            continue;
        }
        if let Some(context) = &result.context {
            for row in &context.before {
                println!("  {}:{}- {}", result.path, row.line, row.text);
            }
        }
        if result.source == "worktree" {
            println!(
                "{}/{}@worktree(base={},state={}):{}:{}:{}: {}",
                result.repo,
                result.root_id,
                result.base_head.as_deref().unwrap_or("unknown"),
                result.file_state,
                result.path,
                result.line,
                result.column,
                result.text
            );
        } else {
            println!(
                "{}/{}@{}:{}:{}:{}: {}",
                result.repo,
                result.root_id,
                result.revision.as_deref().unwrap_or("unknown"),
                result.path,
                result.line,
                result.column,
                result.text
            );
        }
        if let Some(context) = &result.context {
            for row in &context.after {
                println!("  {}:{}- {}", result.path, row.line, row.text);
            }
        }
    }
    if output.summary.truncated {
        eprintln!(
            "results truncated at {}",
            output.summary.max_results.unwrap_or(0)
        );
    }
    for error in &output.errors {
        eprintln!("{}: {}", error.code, error.message);
    }
}

pub(super) fn print_files_human(output: &FilesOutput) {
    for result in &output.results {
        println!(
            "{}/{}@{}:{}",
            result.repo,
            result.root_id,
            result.revision.as_deref().unwrap_or("unknown"),
            result.path
        );
    }
    if output.summary.truncated {
        eprintln!(
            "paths truncated at {}",
            output.summary.max_results.unwrap_or(0)
        );
    }
    for error in &output.errors {
        eprintln!("{}: {}", error.code, error.message);
    }
}
