#!/usr/bin/env bash

set -euo pipefail

readonly HCLOUD_API_URL=https://api.hetzner.cloud/v1
readonly GITHUB_API_URL=https://api.github.com
readonly HETZNER_BOUNCER_FIREWALL_ID=2374657
readonly MANAGED_LABEL_SELECTOR='managed-by%3Dgithub-actions%2Cpurpose%3Dbouncer'
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly SCRIPT_DIR

require_environment() {
  : "${CF_HETZNER_API_TOKEN:?CF_HETZNER_API_TOKEN is required}"
  : "${CF_GITHUB_RUNNERS_MANAGEMENT_TOKEN:?CF_GITHUB_RUNNERS_MANAGEMENT_TOKEN is required}"
  : "${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is required}"
}

# Callers capture the response, so the error body has to go to stderr to reach the job log.
hcloud_api() {
  local body
  if ! body=$(curl --fail-with-body --silent --show-error \
    --header "Authorization: Bearer $CF_HETZNER_API_TOKEN" \
    --header "Content-Type: application/json" \
    "$@"); then
    echo "Hetzner API request failed: $body" >&2
    return 1
  fi
  printf '%s\n' "$body"
}

github_api() {
  local body
  if ! body=$(curl --fail-with-body --silent --show-error \
    --header "Accept: application/vnd.github+json" \
    --header "Authorization: Bearer $CF_GITHUB_RUNNERS_MANAGEMENT_TOKEN" \
    --header "X-GitHub-Api-Version: 2022-11-28" \
    "$@"); then
    echo "GitHub API request failed: $body" >&2
    return 1
  fi
  printf '%s\n' "$body"
}

# Concurrent CI runs can exhaust the project's server quota. Capacity frees up as other runs
# finish, so wait for it instead of failing the run.
create_hcloud_server() {
  local request=$1
  # SECONDS is a bash builtin counting seconds since the shell started, so it keeps advancing.
  local deadline=$((SECONDS + ${HETZNER_CAPACITY_WAIT_SECONDS:-1200}))
  local error_code
  local response
  local status

  while true; do
    if ! response=$(curl --silent --show-error --write-out $'\n%{http_code}' \
      --request POST \
      --header "Authorization: Bearer $CF_HETZNER_API_TOKEN" \
      --header "Content-Type: application/json" \
      --data-binary "$request" \
      "$HCLOUD_API_URL/servers"); then
      echo "Failed to request creation of Hetzner server" >&2
      return 1
    fi
    status=${response##*$'\n'}
    response=${response%$'\n'*}

    if [[ "$status" =~ ^2[0-9][0-9]$ ]]; then
      printf '%s\n' "$response"
      return 0
    fi

    error_code=$(jq --raw-output '.error.code // empty' <<<"$response" 2>/dev/null || true)
    if [[ "$error_code" =~ ^(resource_limit_exceeded|rate_limit_exceeded)$ ]] \
      && ((SECONDS < deadline)); then
      echo "Hetzner $error_code, retrying in 30s" >&2
      sleep 30
      continue
    fi

    echo "Failed to create Hetzner server (HTTP $status): $response" >&2
    return 1
  done
}

# Distinguishes a server that never booted from a bootstrap script that failed.
report_runner_state() {
  local runner_name=$1
  local server_id=$2

  echo "Hetzner server $server_id:" >&2
  hcloud_api "$HCLOUD_API_URL/servers/$server_id" \
    | jq --compact-output '.server | {status, created, ipv4: .public_net.ipv4.ip}' >&2 || true
  hcloud_api "$HCLOUD_API_URL/servers/$server_id/actions?sort=started" \
    | jq --compact-output '.actions[] | {command, status, started, finished, error}' >&2 || true
  echo "GitHub runner $runner_name:" >&2
  github_api "$GITHUB_API_URL/repos/$GITHUB_REPOSITORY/actions/runners?name=$runner_name" \
    | jq --compact-output --arg name "$runner_name" \
      '[.runners[] | select(.name == $name) | {status, busy}]' >&2 || true
}

delete_hcloud_resource() {
  local resource=$1
  local resource_id=$2
  local max_attempts=${3:-1}
  local attempt
  local error
  local response
  local status

  for ((attempt = 1; attempt <= max_attempts; attempt++)); do
    if ! response=$(curl --silent --show-error --write-out $'\n%{http_code}' \
      --request DELETE \
      --header "Authorization: Bearer $CF_HETZNER_API_TOKEN" \
      "$HCLOUD_API_URL/$resource/$resource_id"); then
      echo "Failed to request deletion of Hetzner $resource/$resource_id" >&2
      return 1
    fi
    status=${response##*$'\n'}

    if [[ "$status" =~ ^2[0-9][0-9]$ || "$status" == 404 ]]; then
      return 0
    fi
    if ((attempt < max_attempts)) && [[ "$status" =~ ^(409|422|423)$ ]]; then
      sleep 1
      continue
    fi

    error=$(jq --raw-output \
      'if .error then "\(.error.code): \(.error.message)" else empty end' \
      <<<"${response%$'\n'*}" 2>/dev/null || true)
    echo "Failed to delete Hetzner $resource/$resource_id (HTTP $status${error:+; $error})" >&2
    return 1
  done
}

delete_github_runner() {
  local runner_name=$1
  local runners
  local runner_id
  local runner_is_busy
  local status

  # Hetzner cleanup runs first, so only allow a short window for GitHub to notice
  # the disconnect before reporting the remaining runner record.
  for _ in {1..10}; do
    if ! runners=$(github_api \
      "$GITHUB_API_URL/repos/$GITHUB_REPOSITORY/actions/runners?name=$runner_name&per_page=100"); then
      sleep 1
      continue
    fi

    runner_id=$(jq --raw-output --arg name "$runner_name" \
      '[.runners[] | select(.name == $name)][0].id // empty' <<<"$runners")
    [[ -n "$runner_id" ]] || return 0

    runner_is_busy=$(jq --raw-output --arg name "$runner_name" \
      '[.runners[] | select(.name == $name)][0].busy // false' <<<"$runners")
    if [[ "$runner_is_busy" == true ]]; then
      sleep 1
      continue
    fi

    status=$(curl --silent --show-error --output /dev/null --write-out '%{http_code}' \
      --request DELETE \
      --header "Accept: application/vnd.github+json" \
      --header "Authorization: Bearer $CF_GITHUB_RUNNERS_MANAGEMENT_TOKEN" \
      --header "X-GitHub-Api-Version: 2022-11-28" \
      "$GITHUB_API_URL/repos/$GITHUB_REPOSITORY/actions/runners/$runner_id")
    case "$status" in
      204 | 404) return 0 ;;
      422)
        sleep 1
        ;;
      *)
        echo "Failed to delete GitHub runner $runner_name (HTTP $status)" >&2
        return 1
        ;;
    esac
  done

  echo "Timed out waiting for GitHub runner $runner_name to become idle" >&2
  return 1
}

wait_for_server_deletion() {
  local server_id=$1
  local status

  for _ in {1..30}; do
    status=$(curl --silent --show-error --output /dev/null --write-out '%{http_code}' \
      --header "Authorization: Bearer $CF_HETZNER_API_TOKEN" \
      "$HCLOUD_API_URL/servers/$server_id")
    [[ "$status" == 404 ]] && return 0
    [[ "$status" == 200 ]] || return 1
    sleep 2
  done

  echo "Timed out waiting for Hetzner server $server_id to be deleted" >&2
  return 1
}

destroy_runner() {
  local runner_name=$1
  local server_id=${2:-}
  local response
  local cleanup_status=0

  if [[ -n "$server_id" ]]; then
    if delete_hcloud_resource servers "$server_id"; then
      wait_for_server_deletion "$server_id" || cleanup_status=1
    else
      cleanup_status=1
    fi
  elif response=$(hcloud_api "$HCLOUD_API_URL/servers?name=$runner_name"); then
    while read -r server_id; do
      [[ -n "$server_id" ]] || continue
      if delete_hcloud_resource servers "$server_id"; then
        wait_for_server_deletion "$server_id" || cleanup_status=1
      else
        cleanup_status=1
      fi
    done < <(jq --raw-output --arg name "$runner_name" \
      '.servers[] | select(.name == $name) | .id' <<<"$response")
  else
    echo "Failed to look up Hetzner server $runner_name" >&2
    cleanup_status=1
  fi

  delete_github_runner "$runner_name" || cleanup_status=1

  return "$cleanup_status"
}

create_runner() {
  local runner_name=$1
  local runner_label=$2
  local server_id=''
  local response

  cleanup_failed_creation() {
    local exit_code=$?
    if [[ -n "$server_id" ]]; then
      delete_hcloud_resource servers "$server_id" || true
      wait_for_server_deletion "$server_id" || true
    fi
    exit "$exit_code"
  }
  trap cleanup_failed_creation ERR

  if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
    {
      echo "runner-label=$runner_label"
      echo "runner-name=$runner_name"
    } >>"$GITHUB_OUTPUT"
  fi

  response=$(github_api --request POST \
    "$GITHUB_API_URL/repos/$GITHUB_REPOSITORY/actions/runners/registration-token")
  local registration_token
  registration_token=$(jq --exit-status --raw-output '.token' <<<"$response")
  echo "::add-mask::$registration_token"

  response=$(github_api \
    "$GITHUB_API_URL/repos/$GITHUB_REPOSITORY/actions/runners/downloads")
  local runner_download_url
  runner_download_url=$(jq --exit-status --raw-output \
    '[.[] | select(.os == "linux" and .architecture == "x64")][0].download_url' \
    <<<"$response")

  local resource_labels
  resource_labels=$(jq --null-input \
    --arg run_id "${GITHUB_RUN_ID:-unknown}" \
    '{"managed-by":"github-actions",purpose:"bouncer","github-run-id":$run_id}')

  local startup_script
  local runner_environment
  local runner_service
  startup_script=$(base64 <"$SCRIPT_DIR/start_ephemeral_github_runner.sh" | tr -d '\n')
  runner_environment=$(printf '%s\n' \
    "GITHUB_RUNNER_URL=https://github.com/$GITHUB_REPOSITORY" \
    "RUNNER_DOWNLOAD_URL=$runner_download_url" \
    "RUNNER_LABEL=$runner_label" \
    "RUNNER_NAME=$runner_name" \
    "RUNNER_REGISTRATION_TOKEN=$registration_token" \
    | base64 | tr -d '\n')
  runner_service=$(base64 <<'EOF' | tr -d '\n'
[Unit]
Description=Ephemeral GitHub Actions runner
After=network-online.target
Wants=network-online.target
StartLimitIntervalSec=900
StartLimitBurst=3

[Service]
Type=simple
EnvironmentFile=/etc/actions-runner.env
ExecStart=/usr/local/sbin/start-ephemeral-github-runner
TimeoutStartSec=0
# The bootstrap is idempotent until registration, after which the env file is removed and a
# restart fails fast.
Restart=on-failure
RestartSec=10

[Install]
WantedBy=multi-user.target
EOF
  )

  local user_data
  user_data=$(jq --null-input --raw-output \
    --arg environment "$runner_environment" \
    --arg script "$startup_script" \
    --arg service "$runner_service" \
    '"#cloud-config\nwrite_files:\n  - path: /usr/local/sbin/start-ephemeral-github-runner\n    encoding: b64\n    permissions: '\''0755'\''\n    content: \($script)\n  - path: /etc/actions-runner.env\n    encoding: b64\n    permissions: '\''0600'\''\n    content: \($environment)\n  - path: /etc/systemd/system/actions-runner.service\n    encoding: b64\n    permissions: '\''0644'\''\n    content: \($service)\nruncmd:\n  - [systemctl, daemon-reload]\n  - [systemctl, enable, --now, actions-runner.service]\n"')

  local server_request
  server_request=$(jq --null-input \
    --arg image "${HETZNER_IMAGE:-ubuntu-24.04}" \
    --arg location "${HETZNER_LOCATION:-fsn1}" \
    --arg name "$runner_name" \
    --arg server_type "${HETZNER_SERVER_TYPE:-cpx52}" \
    --arg user_data "$user_data" \
    --argjson firewall_id "$HETZNER_BOUNCER_FIREWALL_ID" \
    --argjson labels "$resource_labels" \
    '{
      name:$name,
      server_type:$server_type,
      image:$image,
      location:$location,
      user_data:$user_data,
      labels:$labels,
      firewalls:[{firewall:$firewall_id}],
      public_net:{enable_ipv4:true,enable_ipv6:false},
      start_after_create:true
    }')
  response=$(create_hcloud_server "$server_request")
  server_id=$(jq --exit-status --raw-output '.server.id' <<<"$response")

  if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
    echo "server-id=$server_id" >>"$GITHUB_OUTPUT"
  fi

  echo "Waiting for ephemeral runner $runner_name to register"
  for _ in {1..60}; do
    if response=$(github_api \
      "$GITHUB_API_URL/repos/$GITHUB_REPOSITORY/actions/runners?name=$runner_name&per_page=100"); then
      if jq --exit-status --arg name "$runner_name" \
        'any(.runners[]; .name == $name and .status == "online")' <<<"$response" >/dev/null; then
        trap - ERR
        echo "Ephemeral runner $runner_name is online"
        return 0
      fi
    fi
    sleep 10
  done

  echo "Timed out waiting for ephemeral runner $runner_name" >&2
  report_runner_state "$runner_name" "$server_id"
  return 1
}

cleanup_stale_runners() {
  local max_age_seconds=${1:-14400}
  local now
  local response
  now=$(date +%s)

  response=$(hcloud_api \
    "$HCLOUD_API_URL/servers?label_selector=$MANAGED_LABEL_SELECTOR&per_page=50")
  while IFS=$'\t' read -r runner_name created_at; do
    [[ -n "$runner_name" ]] || continue
    if ((now - $(date --date="$created_at" +%s) > max_age_seconds)); then
      echo "Deleting stale bouncer runner $runner_name"
      destroy_runner "$runner_name"
    fi
  done < <(jq --raw-output '.servers[] | [.name, .created] | @tsv' <<<"$response")

  response=$(hcloud_api \
    "$HCLOUD_API_URL/firewalls?label_selector=$MANAGED_LABEL_SELECTOR&per_page=50")
  while IFS=$'\t' read -r firewall_id created_at; do
    [[ -n "$firewall_id" ]] || continue
    [[ "$firewall_id" == "$HETZNER_BOUNCER_FIREWALL_ID" ]] && continue
    if ((now - $(date --date="$created_at" +%s) > max_age_seconds)); then
      delete_hcloud_resource firewalls "$firewall_id" 10 || true
    fi
  done < <(jq --raw-output '.firewalls[] | [.id, .created] | @tsv' <<<"$response")
}

main() {
  require_environment

  case "${1:-}" in
    create)
      [[ $# == 3 ]] || { echo "Usage: $0 create RUNNER_NAME RUNNER_LABEL" >&2; return 2; }
      create_runner "$2" "$3"
      ;;
    destroy)
      [[ $# -ge 2 && $# -le 3 ]] || {
        echo "Usage: $0 destroy RUNNER_NAME [SERVER_ID]" >&2
        return 2
      }
      destroy_runner "$2" "${3:-}"
      ;;
    cleanup-stale)
      [[ $# -le 2 ]] || { echo "Usage: $0 cleanup-stale [MAX_AGE_SECONDS]" >&2; return 2; }
      cleanup_stale_runners "${2:-14400}"
      ;;
    *)
      echo "Usage: $0 {create RUNNER_NAME RUNNER_LABEL|destroy RUNNER_NAME [SERVER_ID]|cleanup-stale [MAX_AGE_SECONDS]}" >&2
      return 2
      ;;
  esac
}

main "$@"
