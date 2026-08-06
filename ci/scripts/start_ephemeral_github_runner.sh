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
