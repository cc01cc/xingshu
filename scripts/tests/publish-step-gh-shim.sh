#!/usr/bin/env bash
# Offline `gh` stand-in for scripts/tests/publish-step-drill.sh.
#
# It replays canned REST responses out of a state directory so that
# .github/scripts/publish-release-step.sh can be exercised end-to-end with **no network, no
# credentials and no repository writes**. It is NOT a general gh emulator: it implements exactly
# the calls the publish step makes, and it deliberately fails on anything else.
#
# Deliberate asymmetry that the drill asserts on:
#   * `GET /releases?per_page=100` (list)        -> served, paginated from `page-plan`
#   * `GET /releases/{id}` / `.../{id}/assets`   -> served (draft-visible, by-id inventory)
#   * `GET /releases/tags/{tag}` (by-tag REST)   -> 404 + recorded in `by-tag-api.log`
#     The by-tag endpoint cannot see drafts (that was the v0.1.0 root cause), so the state machine
#     must never depend on it; any call is logged and the drill fails if the log is non-empty.
#   * `gh release download|upload|edit` by tag   -> served for drafts too, which replays what gh
#     does through its GraphQL fallback. This is a *replay*, not a real draft: PLAN-0692 review/04
#     §4③ (真 draft 不可测) stays unverified.
#
# Environment
#   XS_SHIM_STATE_DIR   required; holds releases.json, assets/<id>.json, page-plan, next-id, logs
#   XS_SHIM_SCENARIO    required; free-form label recorded in the call log
#   XS_SHIM_ASSET_ROOT  required; directory `gh release download` copies files from
#
# State files written by the drill before each case
#   releases.json  array of release objects {"id":N,"tag_name":"v…","name":"…","draft":bool,"body":"…"}
#   page-plan      how many list pages to split releases.json into (default 1)
#   next-id        next numeric release id to allocate (default 900001)
#   create-fails   if present, `gh release create` exits 1 with a 422 (read→create race, P2-11)
#   create-appears if present, `gh release create` appends the Release *and* exits 1 (create
#                  actually succeeded elsewhere: the step must converge on the re-read)

set -Eeuo pipefail

state_dir="${XS_SHIM_STATE_DIR:?shim: XS_SHIM_STATE_DIR is required}"
: "${XS_SHIM_SCENARIO:?shim: XS_SHIM_SCENARIO is required}"
asset_source="${XS_SHIM_ASSET_ROOT:?shim: XS_SHIM_ASSET_ROOT is required}"

releases_json="$state_dir/releases.json"
calls_log="$state_dir/gh-calls.log"
by_tag_log="$state_dir/by-tag-api.log"
mkdir -p "$state_dir/assets"
printf '%s\n' "scenario=$XS_SHIM_SCENARIO	$*" >> "$calls_log"

unsupported() {
  echo "shim: unsupported gh invocation: $*" >&2
  exit 97
}

read_releases() { # -> compact JSON array
  if [[ -s "$releases_json" ]]; then jq -c '.' "$releases_json"; else echo '[]'; fi
}

write_releases() { # <compact-or-pretty JSON array>
  printf '%s\n' "$1" | jq -c '.' > "$releases_json"
}

read_assets() { # <id> -> compact JSON array
  local f="$state_dir/assets/$1.json"
  if [[ -s "$f" ]]; then jq -c '.' "$f"; else echo '[]'; fi
}

write_assets() { # <id> <json-array>
  printf '%s\n' "$2" | jq -c '.' > "$state_dir/assets/$1.json"
}

id_for_tag() {
  read_releases | jq -r --arg t "$1" '[.[] | select(.tag_name == $t)][0].id // empty'
}

next_id() {
  local raw
  raw="$(cat "$state_dir/next-id" 2> /dev/null || echo 900001)"
  printf '%s\n' "$((raw + 1))" > "$state_dir/next-id"
  printf '%s\n' "$raw"
}

emit() { # emit <body> [<jq-filter>]
  local body="$1" filter="${2:-}"
  if [[ -n "$filter" ]]; then
    printf '%s' "$body" | jq -r "$filter"
  else
    printf '%s\n' "$body"
  fi
}

# --- gh api -----------------------------------------------------------------------------------
handle_api() {
  local paginate=0 filter="" path=""
  while (($#)); do
    case "$1" in
      --paginate) paginate=1; shift ;;
      --jq) filter="$2"; shift 2 ;;
      -X | --method) shift 2 ;;
      -f | --raw-field | -F | --field) shift 2 ;;
      --input) shift 2 ;;
      --include) shift ;;
      --silent) shift ;;
      -*) shift ;;
      *)
        path="$1"
        shift
        ;;
    esac
  done
  [[ -n "$path" ]] || unsupported "api (no path)"

  case "$path" in
    repos/*/git/ref/tags/*)
      emit "{\"object\":{\"type\":\"commit\",\"sha\":\"$XS_SHIM_TAG_SHA\"}}" "$filter"
      ;;
    repos/*/git/tags/*)
      emit "{\"object\":{\"sha\":\"$XS_SHIM_TAG_SHA\"}}" "$filter"
      ;;
    repos/*/releases/tags/*)
      # Never expected: the by-tag endpoint cannot see drafts (v0.1.0 root cause).
      printf '%s\n' "$path" >> "$by_tag_log"
      echo "gh: Not Found (HTTP 404)" >&2
      exit 1
      ;;
    *'/releases?'*|*'/releases')
      local pages_json
      pages_json="$(read_releases | jq -c --argjson n "$(cat "$state_dir/page-plan" 2> /dev/null || echo 1)" '
        . as $all | ($n | if . < 1 then 1 else . end) as $pages
        | [ range(0; $pages) as $p | [ $all | to_entries[] | select(.key % $pages == $p) | .value ] ]
      ')"
      if [[ "$paginate" -eq 1 ]]; then
        emit "$(printf '%s' "$pages_json" | jq -c '.[]' | tr -d '\n')" "$filter"
      else
        emit "$(printf '%s' "$pages_json" | jq -c '.[0]')" "$filter"
      fi
      ;;
    repos/*/releases/*/assets*)
      local id="${path#repos/*/releases/}"
      id="${id%%/*}"
      emit "$(read_assets "$id")" "$filter"
      ;;
    repos/*/releases/*)
      local id="${path##*/}"
      emit "$(read_releases | jq -c --argjson id "${id:-0}" '[.[] | select(.id == $id)][0] // {}')" "$filter"
      ;;
    *)
      unsupported "api $path"
      ;;
  esac
}

# --- gh release -------------------------------------------------------------------------------
handle_release() {
  local sub="${1:-}"
  shift || true
  case "$sub" in
    create) release_create "$@" ;;
    download) release_download "$@" ;;
    upload) release_upload "$@" ;;
    edit) release_edit "$@" ;;
    *) unsupported "release $sub" ;;
  esac
}

release_create() {
  local tag="$1" target="" title="" notes_file=""
  shift
  while (($#)); do
    case "$1" in
      --target) target="$2"; shift 2 ;;
      --title) title="$2"; shift 2 ;;
      --notes-file) notes_file="$2"; shift 2 ;;
      --repo) shift 2 ;;
      --draft | --verify-tag | --prerelease | --latest) shift ;;
      *) shift ;;
    esac
  done

  local body=""
  if [[ -n "$notes_file" && -f "$notes_file" ]]; then body="$(cat "$notes_file")"; fi
  local new_id entry
  new_id="$(next_id)"
  entry="$(jq -cn --argjson id "$new_id" --arg t "$tag" --arg n "$title" \
    --arg b "$body" --arg c "$target" \
    '{id:$id, tag_name:$t, name:$n, draft:true, prerelease:false, body:$b, target_commitish:$c}')"

  if [[ -f "$state_dir/create-appears" || -f "$state_dir/create-fails" ]]; then
    if [[ -f "$state_dir/create-appears" ]]; then
      # The Release was created by someone else in the read→create window: it must be visible on
      # the re-read even though this call "fails" (P2-11 idempotent recovery).
      write_releases "$(read_releases | jq -c --argjson e "$entry" '. + [$e]')"
      write_assets "$new_id" '[]'
    fi
    echo "gh: Validation Failed (HTTP 422) — already_exists (shim)" >&2
    exit 1
  fi

  write_releases "$(read_releases | jq -c --argjson e "$entry" '. + [$e]')"
  write_assets "$new_id" '[]'
  echo "https://github.com/${XS_SHIM_REPO:-cc01cc/xingshu}/releases/tag/untagged-shim-$new_id"
}

release_download() {
  local tag="$1" pattern="" dir="."
  shift
  while (($#)); do
    case "$1" in
      --pattern) pattern="$2"; shift 2 ;;
      --dir) dir="$2"; shift 2 ;;
      --repo) shift 2 ;;
      *) shift ;;
    esac
  done
  if [[ -f "$state_dir/download-fails" ]]; then
    # Drives the uncaught-failure path so the drill can prove the ERR trap emits ::error:: (T2.2).
    echo "gh: Not Found (HTTP 404) (shim download-fails)" >&2
    exit 1
  fi
  [[ -n "$pattern" ]] || unsupported "release download (no --pattern)"
  [[ -f "$asset_source/$pattern" ]] || {
    echo "gh: asset not found in shim source: $pattern" >&2
    exit 1
  }
  mkdir -p "$dir"
  cp -- "$asset_source/$pattern" "$dir/$pattern"
  echo "downloaded $pattern"
}

release_upload() {
  local tag="$1"
  shift
  local id files=()
  id="$(id_for_tag "$tag")"
  [[ -n "$id" ]] || {
    echo "gh: release not found: $tag (shim)" >&2
    exit 1
  }
  while (($#)); do
    case "$1" in
      --repo) shift 2 ;;
      --clobber) shift ;;
      *) files+=("$1"); shift ;;
    esac
  done
  ((${#files[@]})) || unsupported "release upload (no files)"
  local current name size
  current="$(read_assets "$id")"
  for path in "${files[@]}"; do
    [[ -f "$path" ]] || {
      echo "gh: no such file: $path" >&2
      exit 1
    }
    name="$(basename "$path")"
    size="$(wc -c < "$path" | tr -d ' ')"
    current="$(printf '%s' "$current" | jq -c --arg n "$name" --argjson s "$size" '. + [{name:$n, size:$s}]')"
  done
  write_assets "$id" "$current"
  echo "uploaded ${#files[@]} asset(s)"
}

release_edit() {
  local tag="$1" title="" notes_file="" publish=0
  shift
  while (($#)); do
    case "$1" in
      --title) title="$2"; shift 2 ;;
      --notes-file) notes_file="$2"; shift 2 ;;
      --repo) shift 2 ;;
      --draft=false) publish=1; shift ;;
      --latest | --draft | --prerelease=true) shift ;;
      *) shift ;;
    esac
  done

  local releases updated
  releases="$(read_releases)"
  updated="$(printf '%s' "$releases" | jq -c --arg t "$tag" --arg title "$title" \
    --argjson publish "$publish" '
      map(if .tag_name == $t
          then (if $title != "" then .name = $title else . end)
               | (if $publish == 1 then .draft = false else . end)
          else . end)
    ')"
  write_releases "$updated"

  if [[ -n "$notes_file" ]]; then
    [[ -f "$notes_file" ]] || {
      echo "gh: no such file: $notes_file" >&2
      exit 1
    }
    local body
    body="$(cat "$notes_file")"
    updated="$(read_releases | jq -c --arg t "$tag" --arg b "$body" '
      map(if .tag_name == $t then .body = $b else . end)')"
    write_releases "$updated"
    printf '%s' "$body" > "$state_dir/notes-applied.md"
  fi
  if [[ "$publish" -eq 1 ]]; then
    local id
    id="$(id_for_tag "$tag")"
    read_assets "$id" > "$state_dir/assets-at-publish.json"
    printf '%s\n' "$id" > "$state_dir/published-id"
  fi
  echo "edited release $tag"
}

# --- entry point ------------------------------------------------------------------------------
[[ -f "$releases_json" ]] || : > "$releases_json"

cmd="${1:-}"
shift || true
case "$cmd" in
  api) handle_api "$@" ;;
  release) handle_release "$@" ;;
  version) echo "gh version shim (scripts/tests/publish-step-gh-shim.sh)" ;;
  *) unsupported "$cmd $*" ;;
esac
