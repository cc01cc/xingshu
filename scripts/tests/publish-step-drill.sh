#!/usr/bin/env bash
# Local drill for .github/scripts/publish-release-step.sh.
#
# Runs entirely offline: mktemp workspace, `gh` replaced by scripts/tests/publish-step-gh-shim.sh,
# no token, no repository writes. Three groups of checks:
#
#   1. static guards  — the workflow must address the script with the `release-source/` prefix and
#                       must promote CANDIDATE_RUN_ID/CANDIDATE_RUN_ATTEMPT into the step env.
#   2. layout controls — positive control: the adopted invocation path works inside a workspace that
#                       has ONLY `release-source/`; negative controls: the old repo-root path must be
#                       unreachable both as a `[[ -f ]]` expression and as an executing `bash` call
#                       (PLAN-0692 P0-2 / review/04-lead-adjudication.md §2).
#   3. shim cases     — the frozen state matrix (spec §1): absent / one-draft-partial / two pages
#                       with a single tag hit / n>1 hard failure / published-complete no-op /
#                       create-failure re-read (P2-11).
#
# Usage (Git Bash or any bash 5.x):
#   bash scripts/tests/publish-step-drill.sh
# Exit 0 = every control and case behaved as specified; non-zero = the first violated expectation
# (the raw failing log is printed).

set -Eeuo pipefail

die() {
  echo "::error::publish-step-drill: $*"
  exit 1
}
note() { echo "-- $*"; }

repo_root="$(git rev-parse --show-toplevel)"
step_rel="release-source/.github/scripts/publish-release-step.sh"
step_src="$repo_root/.github/scripts/publish-release-step.sh"
shim_src="$repo_root/scripts/tests/publish-step-gh-shim.sh"
workflow="$repo_root/.github/workflows/release-publish.yml"

[[ -f "$step_src" ]] || die "missing $step_src"
[[ -f "$shim_src" ]] || die "missing $shim_src"
[[ -f "$workflow" ]] || die "missing $workflow"
for tool in jq sha256sum cmp mktemp; do
  command -v "$tool" > /dev/null || die "required tool not found on PATH: $tool"
done

# --- 1. static guards -------------------------------------------------------------------------
note "static guards: workflow addresses the step script only through release-source/"
while IFS= read -r hit; do
  line_text="${hit#*:}"
  trimmed="${line_text#"${line_text%%[![:space:]]*}"}"
  [[ "$trimmed" == \#* ]] && continue
  [[ "$hit" == *release-source/* ]] ||
    die "workflow references the step script without the release-source/ prefix: $hit"
done < <(grep -n '\.github/scripts/publish-release-step\.sh' "$workflow")
if grep -Eq '(bash|[[:space:]]-f)[[:space:]]+\.github/scripts/publish-release-step\.sh' "$workflow"; then
  die "workflow contains a repo-root invocation of the step script (P0-2: unreachable in the publish job)"
fi
grep -Fq 'CANDIDATE_RUN_ID: ${{ steps.candidate.outputs.candidate_run_id }}' "$workflow" ||
  die "workflow does not promote CANDIDATE_RUN_ID into the publish step env (T2.3)"
grep -Fq 'CANDIDATE_RUN_ATTEMPT: ${{ steps.candidate.outputs.candidate_run_attempt }}' "$workflow" ||
  die "workflow does not promote CANDIDATE_RUN_ATTEMPT into the publish step env (T2.3)"

work="$(mktemp -d "${TMPDIR:-/tmp}/xs-drill.XXXXXX")"
cleanup() {
  case "$work" in
    */xs-drill.*) ;;
    *) return 0 ;;
  esac
  if [[ "${XS_DRILL_KEEP:-0}" == "1" ]]; then
    echo "-- kept drill workspace: $work"
    return 0
  fi
  rm -rf -- "$work"
}
trap cleanup EXIT

fake_ws="$work/ws"
mkdir -p "$fake_ws/release-source/.github/scripts" "$fake_ws/bin"
cp -- "$step_src" "$fake_ws/$step_rel"
cp -- "$shim_src" "$fake_ws/bin/gh"
chmod +x "$fake_ws/bin/gh"

# --- 2. layout controls -----------------------------------------------------------------------
note "positive control: $step_rel --self-check inside a release-source/-only workspace"
( cd "$fake_ws" && bash "$step_rel" --self-check ) ||
  die "positive control failed: $step_rel is not reachable/parseable in the publish-job layout"

note "negative control 1: [[ -f .github/scripts/publish-release-step.sh ]] must be false"
if ( cd "$fake_ws" && [[ -f .github/scripts/publish-release-step.sh ]] ); then
  die "negative control failed: the repo-root path must not exist in the publish-job layout"
fi
echo "   [[ -f .github/scripts/publish-release-step.sh ]] -> false"

note "negative control 2: bash .github/scripts/publish-release-step.sh must fail with 'No such file or directory'"
neg_log="$work/negative.log"
if ( cd "$fake_ws" && bash .github/scripts/publish-release-step.sh --self-check ) 2> "$neg_log"; then
  die "negative control failed: the step script must not be executable from the repo root"
fi
grep -q 'No such file or directory' "$neg_log" ||
  die "negative control produced unexpected stderr: $(head -n 3 "$neg_log")"
echo "   raw stderr: $(head -n 1 "$neg_log")"

# --- fixture: seven verified candidate assets + tagged notes ----------------------------------
VERSION_LABEL="0.1.1"
tag_sha="140ead7f0170314caeaacf46dc3ef8c70981cb03"
assets="$work/assets"
notes="$work/notes.md"
mkdir -p "$assets"
win="xingshu-v${VERSION_LABEL}-x86_64-pc-windows-msvc.zip"
win_sum="$win.sha256"
win_manifest="xingshu-v${VERSION_LABEL}-x86_64-pc-windows-msvc.manifest.json"
linux="xingshu-v${VERSION_LABEL}-x86_64-unknown-linux-gnu.tar.gz"
linux_sum="$linux.sha256"
linux_manifest="xingshu-v${VERSION_LABEL}-x86_64-unknown-linux-gnu.manifest.json"
printf 'drill fixture: windows archive\n' > "$assets/$win"
printf 'drill fixture: linux archive\n' > "$assets/$linux"
printf '{"drill":true}\n' > "$assets/$win_manifest"
printf '{"drill":true}\n' > "$assets/$linux_manifest"
( cd "$assets" && sha256sum "$win" > "$win_sum" && sha256sum "$linux" > "$linux_sum" )
cat "$assets/$win_sum" "$assets/$linux_sum" | LC_ALL=C sort > "$assets/SHA256SUMS"
printf '## [%s] - 2026-11-11\n\ndrill notes for the publish step.\n' "$VERSION_LABEL" > "$notes"

expected_assets=(
  "$win" "$win_sum" "$win_manifest" "$linux" "$linux_sum" "$linux_manifest" "SHA256SUMS"
)
[[ "$(ls -A "$assets" | wc -l | tr -d ' ')" == "7" ]] ||
  die "asset fixture must hold exactly 7 files, found $(ls -A "$assets" | wc -l)"

# --- 3. shim cases ----------------------------------------------------------------------------
LAST_STATE_DIR=""
LAST_LOG=""
LAST_RC=0

write_release() { # write_release <state-dir> <json-array>
  printf '%s\n' "$2" | jq -c '.' > "$1/releases.json"
}

release_obj() { # release_obj <id> <tag> <draft:true|false> <body-file-or-empty>
  local body=""
  if [[ -n "${4:-}" ]]; then body="$(jq -Rs . < "$4")"; else body='""'; fi
  jq -cn --argjson id "$1" --arg tag "$2" --argjson draft "$3" --argjson body "$body" \
    '{id:$id, tag_name:$tag, name:"Xingshu 0.1.1", draft:$draft, prerelease:false, body:$body}'
}

assets_json_for() { # assets_json_for <names...>
  local out='[]' name
  for name in "$@"; do
    out="$(printf '%s' "$out" | jq -c --arg n "$name" '. + [{name:$n, size:1}]')"
  done
  printf '%s' "$out"
}

setup_case() { # setup_case <label> <state-dir>
  local label="$1" state_dir="$2"
  mkdir -p "$state_dir/assets"
  printf '1\n' > "$state_dir/page-plan"
  printf '900001\n' > "$state_dir/next-id"
  case "$label" in
    absent)
      write_release "$state_dir" '[]'
      ;;
    draft-zero)
      write_release "$state_dir" "[$(release_obj 900100 "v${VERSION_LABEL}" true "$notes")]"
      printf '[]' > "$state_dir/assets/900100.json"
      ;;
    one-draft-partial)
      write_release "$state_dir" "[$(release_obj 900200 "v${VERSION_LABEL}" true "$notes")]"
      printf '%s' "$(assets_json_for "${expected_assets[0]}" "${expected_assets[1]}" "${expected_assets[6]}")" \
        > "$state_dir/assets/900200.json"
      ;;
    two-pages-one-hit)
      write_release "$state_dir" "[$(release_obj 900300 'v0.0.9' false '') , $(release_obj 900301 "v${VERSION_LABEL}" true "$notes")]"
      printf '2\n' > "$state_dir/page-plan"
      printf '%s' "$(assets_json_for "${expected_assets[0]}" "${expected_assets[1]}" "${expected_assets[2]}")" \
        > "$state_dir/assets/900301.json"
      ;;
    multiple)
      write_release "$state_dir" "[$(release_obj 900400 "v${VERSION_LABEL}" true "$notes") , $(release_obj 900401 "v${VERSION_LABEL}" true "$notes")]"
      printf '[]' > "$state_dir/assets/900400.json"
      printf '[]' > "$state_dir/assets/900401.json"
      ;;
    published-complete)
      write_release "$state_dir" "[$(release_obj 900500 "v${VERSION_LABEL}" false "$notes")]"
      printf '%s' "$(assets_json_for "${expected_assets[@]}")" > "$state_dir/assets/900500.json"
      ;;
    draft-complete)
      write_release "$state_dir" "[$(release_obj 900550 "v${VERSION_LABEL}" true "$notes")]"
      printf '%s' "$(assets_json_for "${expected_assets[@]}")" > "$state_dir/assets/900550.json"
      ;;
    published-missing)
      write_release "$state_dir" "[$(release_obj 900700 "v${VERSION_LABEL}" false "$notes")]"
      printf '%s' "$(assets_json_for "${expected_assets[@]:0:6}")" > "$state_dir/assets/900700.json"
      ;;
    create-fails)
      write_release "$state_dir" '[]'
      : > "$state_dir/create-fails"
      ;;
    download-fails)
      write_release "$state_dir" "[$(release_obj 900600 "v${VERSION_LABEL}" false "$notes")]"
      printf '%s' "$(assets_json_for "${expected_assets[@]}")" > "$state_dir/assets/900600.json"
      : > "$state_dir/download-fails"
      ;;
    create-appears)
      write_release "$state_dir" '[]'
      : > "$state_dir/create-appears"
      ;;
    *) die "unknown case: $label" ;;
  esac
}

run_step() { # run_step <label> <state-dir> <log> <runner-temp>
  local label="$1" state_dir="$2" log="$3" runner_temp="$4"
  LAST_STATE_DIR="$state_dir"
  LAST_LOG="$log"
  mkdir -p "$runner_temp"
  set +e
  (
    cd "$fake_ws" &&
      PATH="$fake_ws/bin:$PATH" \
        XS_SHIM_STATE_DIR="$state_dir" \
        XS_SHIM_SCENARIO="$label" \
        XS_SHIM_ASSET_ROOT="$assets" \
        XS_SHIM_TAG_SHA="$tag_sha" \
        XS_SHIM_REPO="cc01cc/xingshu" \
        RELEASE_TAG="v${VERSION_LABEL}" \
        SOURCE_COMMIT="$tag_sha" \
        REPOSITORY="cc01cc/xingshu" \
        VERSION="$VERSION_LABEL" \
        ASSET_ROOT="$assets" \
        NOTES_FILE="$notes" \
        CANDIDATE_RUN_ID="42090426264" \
        CANDIDATE_RUN_ATTEMPT="1" \
        RUNNER_TEMP="$runner_temp" \
        GH_TOKEN="shim-token" \
        bash "$step_rel"
  ) > "$log" 2>&1
  LAST_RC=$?
  set -e
  note "case $label -> exit $LAST_RC"
}

run_case() { # run_case <label> — fresh shim state
  local label="$1"
  setup_case "$label" "$work/state-$label"
  run_step "$label" "$work/state-$label" "$work/log-$label.txt" "$work/runner-temp-$label"
}

rerun_case() { # rerun_case <label> — same shim state, second pass (T2.5: every case is re-run)
  local label="$1"
  run_step "$label-rerun" "$work/state-$label" "$work/log-$label-rerun.txt" "$work/runner-temp-$label"
}

expect_rc() { # expect_rc <expected>
  [[ "$LAST_RC" == "$1" ]] || {
    echo "--- raw log ($LAST_LOG) ---"
    cat "$LAST_LOG"
    die "case expected exit $1, got $LAST_RC"
  }
}

expect_log() { # expect_log <fixed string>
  grep -Fq -- "$1" "$LAST_LOG" || {
    echo "--- raw log ($LAST_LOG) ---"
    cat "$LAST_LOG"
    die "case log is missing expected text: $1"
  }
}

expect_no_log() { # expect_no_log <fixed string>
  if grep -Fq -- "$1" "$LAST_LOG"; then
    echo "--- raw log ($LAST_LOG) ---"
    cat "$LAST_LOG"
    die "case log must not contain: $1"
  fi
}

expect_no_by_tag_api() { # the by-tag REST endpoint cannot see drafts: never allowed as a state source
  if [[ -s "$LAST_STATE_DIR/by-tag-api.log" ]]; then
    cat "$LAST_STATE_DIR/by-tag-api.log"
    die "the state machine called GET /releases/tags/{tag}, which is 404 for drafts"
  fi
}

# T2.5: every case is re-run on the state left behind by the first pass. Convergence means the
# second pass is a byte-identical no-op, not a second publish/upload.
expect_rerun_noop() { # expect_rerun_noop <label>
  rerun_case "$1"
  expect_rc 0
  expect_log '::notice::state=published assets=7/7 notes=ok'
  expect_no_log 'uploading'
  expect_no_log 'publishing Release'
  expect_no_log 'drifted'
  expect_no_by_tag_api
}

expect_rerun_failure() { # expect_rerun_failure <label>
  rerun_case "$1"
  expect_rc 1
  expect_log '::error::'
  expect_no_by_tag_api
}

note "case absent: no Release -> create draft -> upload 7 -> publish"
run_case absent
expect_rc 0
expect_log '::notice::state=absent'
expect_log '::notice::state=draft assets=0/7'
expect_log 'uploading 7 missing asset(s)'
expect_log '::notice::state=published assets=7/7'
expect_log "Verified stable Release v${VERSION_LABEL} from candidate run 42090426264"
expect_no_by_tag_api
expect_rerun_noop absent

note "case draft-zero: draft Release with 0/7 assets -> upload 7 -> publish (matrix row 2)"
run_case draft-zero
expect_rc 0
expect_log '::notice::state=draft assets=0/7 notes=ok'
expect_log 'uploading 7 missing asset(s)'
expect_log '::notice::state=published assets=7/7'
expect_no_by_tag_api
expect_rerun_noop draft-zero

note "case one-draft-partial: draft with 3/7 assets -> upload only the 4 missing -> publish"
run_case one-draft-partial
expect_rc 0
expect_log '::notice::state=draft assets=3/7'
expect_log 'uploading 4 missing asset(s)'
expect_log '::notice::state=published assets=7/7'
expect_no_by_tag_api
expect_rerun_noop one-draft-partial

note "case two-pages-one-hit: two list pages, one tag hit -> converge on that single Release"
run_case two-pages-one-hit
expect_rc 0
expect_log '::notice::state=draft assets=3/7'
expect_log '::notice::state=published assets=7/7'
expect_no_by_tag_api
grep -Fq 'v0.0.9' "$LAST_STATE_DIR/gh-calls.log" && true
[[ "$(jq -c '[.[] | select(.tag_name == "v'"$VERSION_LABEL"'")] | length' "$LAST_STATE_DIR/releases.json")" == "1" ]] ||
  die "two-pages-one-hit case must still hold exactly one Release for the tag"
expect_rerun_noop two-pages-one-hit

note "case multiple: two Releases on the same tag -> hard failure, no writes"
run_case multiple
expect_rc 1
expect_log 'found 2 Releases for tag'
expect_log '::error::publish-release-step:'
[[ ! -s "$LAST_STATE_DIR/assets-at-publish.json" ]] ||
  die "multiple case must not publish anything"
expect_no_by_tag_api
expect_rerun_failure multiple

note "case published-complete: published Release with all 7 assets -> idempotent no-op"
run_case published-complete
expect_rc 0
expect_log '::notice::state=published assets=7/7 notes=ok'
expect_no_log 'uploading'
expect_no_log '--draft=false'
expect_no_by_tag_api
expect_rerun_noop published-complete

note "case draft-complete: draft with 7/7 assets and matching notes -> publish only (matrix row 4)"
run_case draft-complete
expect_rc 0
expect_log '::notice::state=draft assets=7/7 notes=ok'
expect_no_log 'uploading'
expect_log '::notice::publishing Release'
expect_log '::notice::state=published assets=7/7'
expect_no_by_tag_api
expect_rerun_noop draft-complete

note "case published-missing: published Release missing one asset -> fail closed, never repair (matrix row 5)"
run_case published-missing
expect_rc 1
expect_log 'is missing candidate asset'
expect_log '::error::publish-release-step:'
expect_no_log 'uploading'
expect_no_log 'publishing Release'
expect_no_by_tag_api
expect_rerun_failure published-missing

note "case download-fails: a verification download blows up -> ERR trap emits a named ::error:: (T2.2)"
run_case download-fails
expect_rc 1
expect_log '::error::publish-release-step failed (exit 1) at line'
expect_log 'gh: Not Found (HTTP 404) (shim download-fails)'
expect_no_by_tag_api
expect_rerun_failure download-fails

note "case create-fails: create always 422 and the list stays empty -> named ::error::, no crash"
run_case create-fails
expect_rc 1
expect_log 'gh release create failed'
expect_log 'absent from the list endpoint before AND after create'
expect_log '::error::publish-release-step:'
expect_no_by_tag_api
expect_rerun_failure create-fails

note "case create-appears: create fails but the Release exists on re-read (P2-11) -> converge"
run_case create-appears
expect_rc 0
expect_log 're-reading the release list once (P2-11)'
expect_log '::notice::state=published assets=7/7'
expect_no_by_tag_api
expect_rerun_noop create-appears

echo
echo "drill ok: static guards, positive/negative layout controls, 11 shim cases, and every case re-run (T2.5)"
echo "negative-control raw stderr: $(head -n 1 "$neg_log")"
