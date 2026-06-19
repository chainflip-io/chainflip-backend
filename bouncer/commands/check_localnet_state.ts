#!/usr/bin/env -S pnpm tsx
// Reports the current state of the localnet in one shot: liveness, whether the
// running binary matches the current git HEAD, and whether setup_for_test.sh
// has run against it.
//
// Usage (from bouncer/):
//   ./commands/check_localnet_state.ts
//
// Output (last line is always `State: <STATE>`):
//   State: DOWN     — no localnet reachable on the RPC port
//   State: STALE    — running, but commit hash ≠ current git HEAD
//   State: UNREADY  — running and on HEAD, but setup_for_test.sh hasn't run
//   State: READY    — running, on HEAD, setup has run
//
// Exit codes:
//   0 — READY (safe to run tests against it)
//   1 — any other state
//
// NOTE — false STALE after a rebuild: the commit hash is baked into the node binary
// by a build script that's cache-keyed on Rust source. A commit that changes only
// non-binary files (docs, bouncer/** TypeScript, .github/**) won't trigger a rebuild,
// so the binary keeps the *previous* commit hash and this reports STALE even though
// the running code is effectively current. If the only commits since the running hash
// are non-binary changes, a rebuild won't help and it's safe to proceed.

import { execSync } from 'child_process';

const RPC_URL = 'http://127.0.0.1:9944';

function gitHeadHash(): string {
  return execSync('git rev-parse HEAD', { cwd: '..', encoding: 'utf-8' }).trim();
}

function extractCommitHash(versionString: string): string {
  // Version strings look like "2.2.0-12a36d00e37" — take everything after the last '-'
  const parts = versionString.split('-');
  return parts[parts.length - 1];
}

function commitMatches(buildHash: string, headHash: string): boolean {
  // buildHash is a short hash (e.g. 11 chars); headHash is the full 40-char SHA
  return headHash.startsWith(buildHash);
}

// Minimal fetch-based JSON-RPC call. We avoid `shared/json_rpc` because it
// imports the global pino logger, whose async transport prints noisy
// sonic-boom warnings on process.exit.
async function rpc(method: string): Promise<unknown> {
  const res = await fetch(RPC_URL, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params: [] }),
  });
  const body = (await res.json()) as { result?: unknown; error?: { message: string } };
  if (body.error) throw new Error(body.error.message);
  return body.result;
}

type State = 'DOWN' | 'STALE' | 'UNREADY' | 'READY';

async function main() {
  const head = gitHeadHash();

  // 1. Liveness via system_version (also gives us the commit hash for free).
  let version: string;
  try {
    version = (await rpc('system_version')) as string;
  } catch {
    console.log('Liveness: DOWN (no response on 127.0.0.1:9944)');
    console.log('Commit:   n/a');
    console.log('Setup:    n/a');
    console.log('State:    DOWN');
    process.exit(1);
  }

  // 2. Commit match.
  const runningHash = extractCommitHash(version);
  const onHead = commitMatches(runningHash, head);
  console.log('Liveness: UP');
  console.log(
    `Commit:   ${onHead ? `MATCH (${runningHash})` : `STALE (running ${runningHash} ≠ HEAD ${head.slice(0, runningHash.length)})`}`,
  );

  if (!onHead) {
    console.log('Setup:    skipped (commit stale)');
    console.log('State:    STALE');
    process.exit(1);
  }

  // 3. Setup status — BTC lending pool exists only after setup_concurrent.ts.
  // Deferred import: shared/utils/substrate has top-level RPC side effects that
  // would crash the script if the localnet were down.
  const { getChainflipPolkadotApi } = await import('shared/utils/substrate');
  const api = await getChainflipPolkadotApi();
  const btcPool = (await api.query.lendingPools.generalLendingPools('Btc')).toJSON();
  const setupReady = btcPool !== null;
  console.log(`Setup:    ${setupReady ? 'READY (btc lending pool present)' : 'NOT_SET_UP'}`);

  const state: State = setupReady ? 'READY' : 'UNREADY';
  console.log(`State:    ${state}`);
  process.exit(state === 'READY' ? 0 : 1);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
