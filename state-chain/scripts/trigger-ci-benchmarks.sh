#!/bin/bash
# Trigger the "Run Benchmarks" GitHub Actions workflow (_100_run_benchmarks.yml)
# via the gh CLI. The workflow runs benchmark-all.sh on a dedicated runner and
# opens a PR with the regenerated weights against the base branch.
#
# The target commit must have a completed binary build from ci-benchmarks-main.yml
# (pushes to main) or ci-benchmarks-release.yml (pushes to release/*) — the
# workflow downloads the chainflip-node-ubuntu-benchmarks-<profile> artifact from
# that build. This script verifies the artifact build exists before dispatching.
set -euo pipefail

WORKFLOW=_100_run_benchmarks.yml

usage() {
    cat <<EOF
Usage: $(basename "$0") [options]

Options:
  --commit <ref>       Commit to benchmark (default: latest commit on the base
                       branch with a successful benchmark-binary build)
  --base <branch>      Branch to run the workflow on; the results PR is opened
                       against it (default: main)
  --profile <p>        Binary build profile: production | release (default: production)
  --steps <n>          Benchmark steps (default: 20)
  --repetitions <n>    Repetitions per step (default: 10)
  --machine <spec>     Runner spec: 4vCPU-8GB | 4vCPU-16GB (default: 4vCPU-8GB)
  --force              Dispatch even if no binary build is found for the commit
  --dry-run            Print the gh command instead of running it
  --yes                Skip the confirmation prompt
  --no-watch           Don't watch the dispatched run until it completes
  -h, --help           Show this help

Examples:
  $(basename "$0")                          # benchmark latest built commit on main
  $(basename "$0") --commit 71b1d5ac97      # benchmark a specific main commit
  $(basename "$0") --base release/1.11 --commit abc1234
EOF
}

commit=""
base=main
profile=production
steps=20
repetitions=10
machine=4vCPU-8GB
force=false
dry_run=false
confirm=true
watch=true

while [ $# -gt 0 ]; do
    case "$1" in
        --commit) commit="$2"; shift 2 ;;
        --base) base="$2"; shift 2 ;;
        --profile) profile="$2"; shift 2 ;;
        --steps) steps="$2"; shift 2 ;;
        --repetitions) repetitions="$2"; shift 2 ;;
        --machine) machine="$2"; shift 2 ;;
        --force) force=true; shift ;;
        --dry-run) dry_run=true; shift ;;
        --yes) confirm=false; shift ;;
        --no-watch) watch=false; shift ;;
        -h|--help) usage; exit 0 ;;
        *) echo "Unknown option: $1" >&2; usage >&2; exit 1 ;;
    esac
done

# Which workflow builds the benchmark binaries for this branch (must match the
# derivation in _100_run_benchmarks.yml).
case "$base" in
    release/*) binary_workflow=ci-benchmarks-release.yml ;;
    *) binary_workflow=ci-benchmarks-main.yml ;;
esac

if [ -z "$commit" ]; then
    commit=$(gh run list --workflow "$binary_workflow" --branch "$base" \
        --status success --limit 1 --json headSha --jq '.[0].headSha // empty')
    if [ -z "$commit" ]; then
        echo "No successful $binary_workflow run found on $base." >&2
        exit 1
    fi
    echo "Defaulting to latest built commit on $base: $commit"
fi

full_sha=$(gh api "repos/{owner}/{repo}/commits/${commit}" --jq .sha)

if [ "$force" != true ]; then
    build_url=$(gh run list --workflow "$binary_workflow" --commit "$full_sha" \
        --status success --limit 1 --json url --jq '.[0].url // empty')
    if [ -z "$build_url" ]; then
        echo "No successful $binary_workflow run found for $full_sha." >&2
        echo "The benchmark workflow would fail downloading the binary artifact." >&2
        echo "Pick a commit pushed to ${base} (or use --force to dispatch anyway)." >&2
        exit 1
    fi
    echo "Binary build found: $build_url"
fi

cmd=(gh workflow run "$WORKFLOW" --ref "$base"
    -f "commit_sha=$full_sha"
    -f "steps=$steps"
    -f "repetitions=$repetitions"
    -f "benchmark_machine_spec=$machine"
    -f "profile=$profile")

if [ "$dry_run" = true ]; then
    echo "Dry run. Would execute:"
    printf ' %q' "${cmd[@]}"
    echo
    exit 0
fi

echo
echo "This will run benchmarks (steps=$steps, repetitions=$repetitions, $profile profile, $machine)"
echo "against commit $full_sha on branch $base,"
echo "and open a PR with the results against $base."
if [ "$confirm" = true ]; then
    read -r -p "Ok? [y/N] " answer || answer=""
    case "$answer" in
        y|Y|yes|Yes) ;;
        *) echo "Aborted."; exit 1 ;;
    esac
fi

"${cmd[@]}"
echo "Dispatched. Waiting for the run to appear..."
sleep 5

run_id=$(gh run list --workflow "$WORKFLOW" --limit 1 --json databaseId --jq '.[0].databaseId // empty')
if [ -n "$run_id" ]; then
    gh run view "$run_id" --json url --jq .url
    if [ "$watch" = true ]; then
        gh run watch "$run_id"
    fi
else
    echo "Run not visible yet. Check: gh run list --workflow $WORKFLOW"
fi
