#!/usr/bin/env bash

set -euo pipefail

: "${GITHUB_RUNNER_URL:?GITHUB_RUNNER_URL is required}"
: "${RUNNER_DOWNLOAD_URL:?RUNNER_DOWNLOAD_URL is required}"
: "${RUNNER_LABEL:?RUNNER_LABEL is required}"
: "${RUNNER_NAME:?RUNNER_NAME is required}"
: "${RUNNER_REGISTRATION_TOKEN:?RUNNER_REGISTRATION_TOKEN is required}"

export DEBIAN_FRONTEND=noninteractive

apt-get update
apt-get install --yes \
  build-essential \
  ca-certificates \
  curl \
  docker-compose-v2 \
  docker.io \
  git \
  jq \
  npm \
  python3 \
  sudo \
  tar \
  unzip \
  xz-utils

systemctl enable --now docker

if ! id runner >/dev/null 2>&1; then
  useradd --create-home --shell /bin/bash runner
fi
usermod --append --groups docker,sudo runner
echo "runner ALL=(ALL) NOPASSWD:ALL" >/etc/sudoers.d/actions-runner
chmod 0440 /etc/sudoers.d/actions-runner

npm install --global pnpm@10

# Localnet dependencies are installed here, before the runner registers, so that this work
# happens while the binaries are still building rather than on the bouncer job's critical path.
if [[ -n "${SOLANA_VERSION:-}" ]]; then
  curl --fail --silent --show-error --location https://ngrok-agent.s3.amazonaws.com/ngrok.asc \
    >/etc/apt/trusted.gpg.d/ngrok.asc
  echo "deb https://ngrok-agent.s3.amazonaws.com buster main" >/etc/apt/sources.list.d/ngrok.list
  apt-get update
  apt-get install --yes bc ngrok postgresql-client xxd

  npm install --global wscat

  runuser --user runner -- env HOME=/home/runner sh -c \
    "$(curl --fail --silent --show-error --location "https://release.anza.xyz/$SOLANA_VERSION/install")"
fi

# Best effort: the localnet pulls again on startup, so a failure here only costs time.
if [[ -n "${LOCALNET_COMPOSE_URL:-}" ]]; then
  if curl --fail --silent --show-error --location "$LOCALNET_COMPOSE_URL" \
    --output /tmp/localnet-docker-compose.yml; then
    NODE_COUNT="${LOCALNET_NODE_COUNT:-1-node}" LOCALNET_SPEED_SETTING=default \
      docker compose -f /tmp/localnet-docker-compose.yml pull --quiet --ignore-pull-failures \
      || echo "Pre-pulling localnet images failed" >&2
  else
    echo "Failed to download $LOCALNET_COMPOSE_URL, skipping image pre-pull" >&2
  fi
fi

runner_root=/opt/actions-runner
install --directory --owner runner --group runner "$runner_root"
curl --fail --location --silent --show-error "$RUNNER_DOWNLOAD_URL" \
  --output "$runner_root/actions-runner.tar.gz"
tar --extract --gzip --file "$runner_root/actions-runner.tar.gz" --directory "$runner_root"
rm "$runner_root/actions-runner.tar.gz"

"$runner_root/bin/installdependencies.sh"
chown --recursive runner:runner "$runner_root"

cd "$runner_root"
runuser --user runner -- ./config.sh \
  --ephemeral \
  --labels "$RUNNER_LABEL" \
  --name "$RUNNER_NAME" \
  --token "$RUNNER_REGISTRATION_TOKEN" \
  --unattended \
  --url "$GITHUB_RUNNER_URL" \
  --work _work

rm -f \
  /etc/actions-runner.env \
  /var/lib/cloud/instance/user-data.txt \
  /var/lib/cloud/instance/user-data.txt.i
unset RUNNER_REGISTRATION_TOKEN

exec runuser --user runner -- ./run.sh
