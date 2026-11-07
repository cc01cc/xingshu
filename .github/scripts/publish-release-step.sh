#!/usr/bin/env bash
# Publish (or resume) the GitHub Release for a stable tag whose commit already passed
# the release-candidate workflow.
#
# Contract (frozen): plans/PLAN-0692-XS-v011-publish-fix-and-firstrun/spec/publish-and-quickstart-contract.md §1
#   - sole state source = REST list endpoint `GET /repos/{repo}/releases?per_page=100` (paginated),
#     filtered by `tag_name`; the by-tag endpoint is never used for state (it is 404 for drafts).
#   - state / asset *inventory* is read by-id (`/releases/{id}`, `/releases/{id}/assets`);
#     upload / download / edit are addressed by tag (gh only speaks tag addressing; its draft
#     support comes from a GraphQL fallback).
#   - every unhandled failure must emit `::error::` (set -Eeuo pipefail + ERR trap + assert()).
#   - every run must emit `::notice::state=absent|draft|published assets=<visible>/<expected> notes=ok|stale`.
#
# Invoked from .github/workflows/release-publish.yml as
#     bash release-source/.github/scripts/publish-release-step.sh
# The publish job's only checkout lands in `release-source/` (release-publish.yml "Checkout the
# tagged source for release notes"), so the repo-root path `.github/scripts/...` does NOT exist
# inside the job. Local drills therefore reproduce that layout — see
# scripts/tests/publish-step-drill.sh and PLAN-0692 review/04-lead-adjudication.md §2.
#
# Modes
#   (no args)    run the state machine
#   --self-check offline reachability + syntax probe (used by the drill's positive control)
#
# Required env: RELEASE_TAG SOURCE_COMMIT REPOSITORY VERSION ASSET_ROOT NOTES_FILE
#               CANDIDATE_RUN_ID CANDIDATE_RUN_ATTEMPT RUNNER_TEMP GH_TOKEN
# Exit: 0 = converged (published, or idempotent no-op); non-zero = fail-closed with `::error::`.

set -Eeuo pipefail

step="publish-release-step"
script_path="${BASH_SOURCE[0]}"

if [[ "${1:-}" == "--self-check" ]]; then
  bash -n "$script_path" || {
    echo "::error::$step self-check: $script_path does not parse"
    exit 1
  }
  echo "self-check ok: $step reachable at $script_path (bash ${BASH_VERSION})"
  exit 0
fi

# --- failure diagnostics (T2.2) ---------------------------------------------------------------
# Any uncaught failure becomes a named `::error::` line instead of a bare "Process completed
# with exit code 1." This is the root-cause fix for the v0.1.0 run that failed with no annotation.
err_trap() {
  local status="$?"
  echo "::error::$step failed (exit $status) at line $1: $2"
  exit "$status"
}
trap 'err_trap "$LINENO" "$BASH_COMMAND"' ERR

fail() {
  echo "::error::$step: $*"
  exit 1
}

# Bare-call asserts. `assert_*` must be invoked directly, never inside `if !` / `&&` / `||`:
# bash disables errexit (and therefore the ERR trap) in condition contexts, which is exactly how
# a failed `[[ ... ]]` in the v0.1.0 step produced no annotation (PLAN-0692 P1 / P2-16).
assert_equal() { # assert_equal <label> <actual> <expected> [<hint>]
  local label="$1" actual="$2" expected="$3" hint="${4:-}"
  if [[ "$actual" != "$expected" ]]; then
    fail "$label mismatch${hint:+ ($hint)}: expected [$expected], got [$actual]"
  fi
}

assert_nonempty() { # assert_nonempty <label> <value>
  local label="$1" value="${2:-}"
  [[ -n "$value" ]] || fail "$label must not be empty"
}

assert_count() { # assert_count <label> <actual> <expected>
  local label="$1" actual="$2" expected="$3"
  if [[ "$actual" != "$expected" ]]; then
    fail "$label count mismatch: expected $expected, got $actual"
  fi
}

# A native Windows `jq` rewrites every "\n" it writes to "\r\n"; strip CR so that values and
# name lists compare byte-identically on a Linux runner and in the local Git Bash drill.
# Spec §1 invariant 6: report each observed state with one `::notice::state=…` line, and do not
# repeat an identical line when the run converges on the state it already reported.
last_notice=""
emit_state_notice() { # emit_state_notice <state> <visible-assets> <notes-state>
  local line="::notice::state=$1 assets=$2/$expected_count notes=$3"
  if [[ "$line" != "$last_notice" ]]; then
    echo "$line"
    last_notice="$line"
  fi
}

json_value() { # json_value <file> <jq filter> -> single-line value without trailing CR/LF
  jq -r "$2" "$1" | tr -d '\r\n'
}
json_lines() { # json_lines <file> <jq filter> -> one value per line, without CR
  jq -r "$2" "$1" | tr -d '\r'
}

# --- inputs -----------------------------------------------------------------------------------
required_env=(
  RELEASE_TAG SOURCE_COMMIT REPOSITORY VERSION ASSET_ROOT NOTES_FILE
  CANDIDATE_RUN_ID CANDIDATE_RUN_ATTEMPT RUNNER_TEMP
)
for name in "${required_env[@]}"; do
  [[ -n "${!name:-}" ]] || fail "required environment variable $name is empty (declare it in the workflow step env:)"
done

repo="$REPOSITORY"
release_tag="$RELEASE_TAG"
version="$VERSION"
asset_root="$ASSET_ROOT"
notes_file="$NOTES_FILE"
release_title="Xingshu $version"
expected_count=7

[[ "$release_tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] ||
  fail "refusing to publish from a non-stable SemVer tag: $release_tag"
[[ "$SOURCE_COMMIT" =~ ^[0-9a-f]{40}$ ]] ||
  fail "source commit must be a full 40-character sha: ${SOURCE_COMMIT:-<empty>}"
[[ "$CANDIDATE_RUN_ID" =~ ^[0-9]+$ ]] ||
  fail "candidate run id must be numeric: $CANDIDATE_RUN_ID"
[[ "$CANDIDATE_RUN_ATTEMPT" =~ ^[0-9]+$ ]] ||
  fail "candidate run attempt must be numeric: $CANDIDATE_RUN_ATTEMPT"

# --- the pushed tag must resolve to the verified candidate commit ------------------------------
tag_ref_json="$RUNNER_TEMP/pushed-tag.json"
gh api "repos/$repo/git/ref/tags/$release_tag" > "$tag_ref_json"
tag_type="$(json_value "$tag_ref_json" '.object.type // empty')"
tag_object="$(json_value "$tag_ref_json" '.object.sha // empty')"
if [[ "$tag_type" == "tag" ]]; then
  # Annotated tag: peel one level to the commit it points at.
  tag_object="$(gh api "repos/$repo/git/tags/$tag_object" | jq -r '.object.sha // empty' | tr -d '\r\n')"
fi
case "$tag_type" in
  tag | commit) ;;
  *) fail "pushed tag $release_tag resolves to unsupported object type [${tag_type:-unknown}]" ;;
esac
assert_equal "pushed tag target" "$tag_object" "$SOURCE_COMMIT" \
  "tag $release_tag must resolve to the verified candidate commit"

# --- the seven verified candidate assets must still be staged locally --------------------------
expected_assets=(
  "xingshu-v${version}-x86_64-pc-windows-msvc.zip"
  "xingshu-v${version}-x86_64-pc-windows-msvc.zip.sha256"
  "xingshu-v${version}-x86_64-pc-windows-msvc.manifest.json"
  "xingshu-v${version}-x86_64-unknown-linux-gnu.tar.gz"
  "xingshu-v${version}-x86_64-unknown-linux-gnu.tar.gz.sha256"
  "xingshu-v${version}-x86_64-unknown-linux-gnu.manifest.json"
  "SHA256SUMS"
)
assert_count "expected asset list (2 archives + 2 sidecars + 2 manifests + SHA256SUMS)" \
  "${#expected_assets[@]}" "$expected_count"
for name in "${expected_assets[@]}"; do
  [[ -f "$asset_root/$name" ]] || fail "missing verified candidate asset: $name (ASSET_ROOT=$asset_root)"
done
expected_names="$(printf '%s\n' "${expected_assets[@]}" | LC_ALL=C sort)"
# Spec §1 invariant 2: SHA256SUMS is part of the set and covers the other six files.
(cd "$asset_root" && sha256sum --check SHA256SUMS > /dev/null) ||
  fail "SHA256SUMS in ASSET_ROOT does not match the assembled candidate assets"

# --- state resolution: list endpoint, filtered by tag_name (0 / 1 / n) ------------------------
# `--paginate` merges the array pages into ONE JSON array document (measured on gh 2.103.3:
# 45 pages -> 1 document); `jq -s 'add // []'` is therefore correct and a hand-rolled page loop
# is forbidden. `per_page=100` is pinned so the page size cannot depend on server defaults.
releases_json="$RUNNER_TEMP/releases-all.json"
matches_json="$RUNNER_TEMP/tag-releases.json"

list_releases() {
  gh api --paginate "repos/$repo/releases?per_page=100" > "$releases_json"
}

select_tag_releases() {
  jq -s --arg t "$release_tag" 'add // [] | map(select(.tag_name == $t))' "$releases_json" \
    > "$matches_json"
  jq 'length' "$matches_json" | tr -d '\r\n'
}

list_releases
release_count="$(select_tag_releases)"

if [[ "$release_count" -eq 0 ]]; then
  # Frozen matrix row 1 (spec §1). There are no stored notes yet, so the notes comparison is
  # `stale` until the draft is created from the tagged CHANGELOG section below.
  emit_state_notice absent 0 stale
  # Read-then-create is not atomic: if create fails (422 already_exists from a concurrent run),
  # re-read the list ONCE and continue from whatever state the repository is actually in (P2-11)
  # instead of dying with a bare error.
  if ! gh release create "$release_tag" --draft --target "$SOURCE_COMMIT" \
    --title "$release_title" --notes-file "$notes_file" --verify-tag --repo "$repo"; then
    echo "::notice::gh release create failed for $release_tag; re-reading the release list once (P2-11)"
  fi
  list_releases
  release_count="$(select_tag_releases)"
  [[ "$release_count" -ge 1 ]] ||
    fail "draft Release $release_tag is absent from the list endpoint before AND after create"
fi

if [[ "$release_count" -gt 1 ]]; then
  jq -r '.[] | "  id=\(.id) draft=\(.draft) name=\(.name // "")"' "$matches_json" >&2
  fail "found $release_count Releases for tag $release_tag; a tag may carry several Releases but this step refuses to guess (spec §1: n>1 is a hard failure)"
fi
release_id="$(json_value "$matches_json" '.[0].id // empty')"
assert_nonempty "Release id for $release_tag" "$release_id"

# --- authoritative state by id (draft-visible) ------------------------------------------------
state_json="$RUNNER_TEMP/release-state.json"
gh api "repos/$repo/releases/$release_id" > "$state_json"
assert_equal "Release tag_name for id $release_id" "$(json_value "$state_json" '.tag_name // empty')" \
  "$release_tag"

is_draft="$(json_value "$state_json" '.draft')"
published="false"
[[ "$is_draft" == "false" ]] && published="true"
actual_name="$(json_value "$state_json" '.name // empty')"
# GitHub stores the body verbatim; normalise CR so a CRLF checkout of CHANGELOG.md cannot cause a
# spurious rewrite of an intact Release.
expected_notes="$(tr -d '\r' < "$notes_file")"
actual_notes="$(jq -r '.body // ""' "$state_json" | tr -d '\r')"

# Title: a draft may be renamed to the canonical title; a published Release is never rewritten.
if [[ "$actual_name" != "$release_title" ]]; then
  [[ "$published" == "true" ]] &&
    fail "published Release $release_tag has title [$actual_name] instead of [$release_title]; refusing to rewrite a published Release"
  echo "::notice::renaming draft Release title from [$actual_name] to [$release_title]"
  gh release edit "$release_tag" --title "$release_title" --repo "$repo"
  actual_name="$release_title"
fi

# Notes: drift on a draft is repaired from the tagged CHANGELOG section; drift on a published
# Release is fail-closed (spec §1 invariant 4) — the fix is a new tag, not an in-place rewrite.
notes_state="ok"
if [[ "$actual_notes" != "$expected_notes" ]]; then
  notes_state="stale"
  [[ "$published" == "true" ]] &&
    fail "published Release $release_tag has notes that differ from the tagged CHANGELOG section; refusing to rewrite a published Release (ship the fix in a new tag)"
  echo "::notice::draft Release notes for $release_tag drifted; refreshing from $notes_file"
  gh release edit "$release_tag" --notes-file "$notes_file" --repo "$repo"
  actual_notes="$expected_notes"
  notes_state="ok"
fi

# --- asset inventory by id --------------------------------------------------------------------
assets_json="$RUNNER_TEMP/release-assets.json"
gh api "repos/$repo/releases/$release_id/assets?per_page=100" > "$assets_json"
visible_count="$(jq 'length' "$assets_json" | tr -d '\r\n')"
existing_names="$(json_lines "$assets_json" '.[].name' | LC_ALL=C sort)"
current_state="draft"
[[ "$published" == "true" ]] && current_state="published"
emit_state_notice "$current_state" "$visible_count" "$notes_state"

# Any asset outside the expected seven is an immediate hard failure: it means the Release was
# produced by something other than this pipeline, and silently overwriting it is not allowed.
while IFS= read -r name; do
  [[ -z "$name" ]] && continue
  grep -Fxq -- "$name" <<< "$expected_names" ||
    fail "Release $release_tag carries unexpected asset '$name'; refusing to overwrite or delete it (expected set: ${expected_assets[*]})"
done <<< "$existing_names"

verify_dir="$RUNNER_TEMP/release-downloads"
mkdir -p "$verify_dir"
missing_assets=()
missing_names=()
for name in "${expected_assets[@]}"; do
  candidate_file="$asset_root/$name"
  if grep -Fxq -- "$name" <<< "$existing_names"; then
    rm -f "$verify_dir/$name"
    gh release download "$release_tag" --repo "$repo" --pattern "$name" --dir "$verify_dir"
    [[ -f "$verify_dir/$name" ]] ||
      fail "gh release download reported success but $name is missing from $verify_dir"
    cmp --silent "$candidate_file" "$verify_dir/$name" ||
      fail "existing Release asset '$name' differs from the verified candidate bytes"
  elif [[ "$published" == "true" ]]; then
    fail "published Release $release_tag is missing candidate asset '$name'; refusing to modify a published Release"
  else
    missing_assets+=("$candidate_file")
    missing_names+=("$name")
  fi
done

# Only a draft may be completed; upload exactly the missing files (never --clobber).
if [[ "$published" != "true" && "${#missing_assets[@]}" -gt 0 ]]; then
  echo "::notice::uploading ${#missing_assets[@]} missing asset(s) to draft Release $release_tag: ${missing_names[*]}"
  gh release upload "$release_tag" "${missing_assets[@]}" --repo "$repo"
fi

# --- final verification: exact set equality, then byte equality for all seven -----------------
gh api "repos/$repo/releases/$release_id/assets?per_page=100" > "$assets_json"
final_count="$(jq 'length' "$assets_json" | tr -d '\r\n')"
final_names="$(json_lines "$assets_json" '.[].name' | LC_ALL=C sort)"
if [[ "$final_names" != "$expected_names" ]]; then
  printf 'Expected:\n%s\nActual:\n%s\n' "$expected_names" "$final_names"
  fail "Release assets do not exactly match the verified candidate set"
fi
assert_count "Release asset count for $release_tag" "$final_count" "$expected_count"

for name in "${expected_assets[@]}"; do
  rm -f "$verify_dir/$name"
  gh release download "$release_tag" --repo "$repo" --pattern "$name" --dir "$verify_dir"
  cmp --silent "$asset_root/$name" "$verify_dir/$name" ||
    fail "uploaded Release asset '$name' differs from the verified candidate bytes"
done

# --- publish (irreversible) -------------------------------------------------------------------
if [[ "$published" != "true" ]]; then
  echo "::notice::publishing Release $release_tag (--draft=false --latest)"
  gh release edit "$release_tag" --draft=false --latest --repo "$repo"
fi

emit_state_notice published "$expected_count" ok
echo "Verified stable Release $release_tag from candidate run $CANDIDATE_RUN_ID (attempt $CANDIDATE_RUN_ATTEMPT) at $SOURCE_COMMIT."
