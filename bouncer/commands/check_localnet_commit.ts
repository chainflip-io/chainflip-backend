#!/usr/bin/env -S pnpm tsx
// Checks whether the running localnet and the local debug binary are built from the current git HEAD.
//
// Usage (from bouncer/):
//   ./commands/check_localnet_commit.ts
//
// Exit codes:
//   0 — both running localnet and binary match HEAD
//   1 — mismatch or localnet not running

import { execSync } from 'child_process';
import { jsonRpc } from 'shared/json_rpc';
import { globalLogger as logger } from 'shared/utils/logger';

function gitHeadHash(): string {
  return execSync('git rev-parse HEAD', { cwd: '..', encoding: 'utf-8' }).trim();
}

function extractCommitHash(versionString: string): string {
  // Version strings look like "2.2.0-12a36d00e37" — take everything after the last '-'
  const parts = versionString.split('-');
  return parts[parts.length - 1];
}

function matches(buildHash: string, headHash: string): boolean {
  // buildHash is a short hash (e.g. 11 chars); headHash is the full 40-char SHA
  return headHash.startsWith(buildHash);
}

async function main() {
  const head = gitHeadHash();
  logger.info(`Current HEAD: ${head}`);

  // Check running localnet
  let localnetOk = false;
  try {
    const version = (await jsonRpc(
      logger,
      'system_version',
      [],
      'http://127.0.0.1:9944',
    )) as unknown as string;
    const runningHash = extractCommitHash(version);
    localnetOk = matches(runningHash, head);
    logger.info(
      `Running localnet: ${version} → ${localnetOk ? '✓ matches HEAD' : `✗ STALE (${runningHash} ≠ ${head.slice(0, runningHash.length)})`}`,
    );
  } catch {
    logger.warn('Running localnet: not reachable (is it running?)');
  }

  // Check debug binary
  let binaryOk = false;
  try {
    const binaryVersion = execSync('../target/debug/chainflip-node -V', {
      encoding: 'utf-8',
    }).trim();
    const binaryHash = extractCommitHash(binaryVersion);
    binaryOk = matches(binaryHash, head);
    logger.info(
      `Debug binary:    ${binaryVersion} → ${binaryOk ? '✓ matches HEAD' : `✗ STALE (${binaryHash} ≠ ${head.slice(0, binaryHash.length)})`}`,
    );
  } catch {
    logger.warn('Debug binary: not found at ../target/debug/chainflip-node');
  }

  if (!localnetOk || !binaryOk) {
    logger.error(
      'One or more components are not on the current HEAD — rebuild with ./localnet/build_and_run.sh',
    );
    process.exit(1);
  }

  logger.info('All good — localnet and binary are on the current HEAD.');
  process.exit(0);
}

main().catch((err) => {
  logger.error(err);
  process.exit(1);
});
