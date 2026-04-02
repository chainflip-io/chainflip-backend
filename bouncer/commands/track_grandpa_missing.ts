#!/usr/bin/env -S pnpm tsx

import { jsonRpc } from 'shared/json_rpc';
import { globalLogger as logger } from 'shared/utils/logger';
import { sleep } from 'shared/utils';

type VoteState = {
  missing?: string[];
};

type ReportedRoundState = {
  round: number;
  prevotes?: VoteState;
  precommits?: VoteState;
};

type RoundStateResponse = {
  setId: number;
  best: ReportedRoundState;
  background: ReportedRoundState[];
};

type MissingStats = {
  prevotes: number;
  precommits: number;
};

const DEFAULT_ENDPOINT = 'http://127.0.0.1:9944';
const DEFAULT_INTERVAL_MS = 2_000;

function printUsage(): void {
  console.error(
    'Usage: ./commands/track_grandpa_missing.ts [http-endpoint] [interval-ms]\n' +
      `Defaults: endpoint=${DEFAULT_ENDPOINT}, interval=${DEFAULT_INTERVAL_MS}`,
  );
}

function getRoundStates(response: RoundStateResponse): ReportedRoundState[] {
  return [response.best, ...response.background];
}

function incrementStats(
  statsByValidator: Map<string, MissingStats>,
  validatorId: string,
  voteType: keyof MissingStats,
): void {
  const existing = statsByValidator.get(validatorId) ?? { prevotes: 0, precommits: 0 };
  existing[voteType] += 1;
  statsByValidator.set(validatorId, existing);
}

function printSummary(
  statsByValidator: Map<string, MissingStats>,
  response: RoundStateResponse,
  pollCount: number,
): void {
  const rows = [...statsByValidator.entries()]
    .map(([validatorId, stats]) => ({
      validatorId,
      prevotesMissing: stats.prevotes,
      precommitsMissing: stats.precommits,
      totalMissing: stats.prevotes + stats.precommits,
    }))
    .sort(
      (left, right) =>
        right.totalMissing - left.totalMissing || left.validatorId.localeCompare(right.validatorId),
    );

  console.clear();
  console.log(
    `[${new Date().toISOString()}] polled ${pollCount} times | setId=${response.setId} | bestRound=${response.best.round} | backgroundRounds=${response.background.length}`,
  );

  if (rows.length === 0) {
    console.log('No missing validators observed yet.');
    return;
  }

  console.table(rows);
}

async function main(): Promise<void> {
  const endpoint = process.argv[2] ?? DEFAULT_ENDPOINT;
  const intervalMs = Number(process.argv[3] ?? DEFAULT_INTERVAL_MS);

  if (process.argv[2] === '--help' || process.argv[2] === '-h') {
    printUsage();
    process.exit(0);
  }

  if (!Number.isFinite(intervalMs) || intervalMs <= 0) {
    printUsage();
    throw new Error(`Invalid interval: ${process.argv[3]}`);
  }

  const statsByValidator = new Map<string, MissingStats>();
  let pollCount = 0;

  process.on('SIGINT', () => {
    logger.info('Stopping GRANDPA missing tracker');
    process.exit(0);
  });

  logger.info(`Tracking grandpa_roundState on ${endpoint} every ${intervalMs}ms`);

  for (;;) {
    const response = (await jsonRpc(
      logger,
      'grandpa_roundState',
      [],
      endpoint,
    )) as unknown as RoundStateResponse;

    for (const roundState of getRoundStates(response)) {
      for (const validatorId of roundState.prevotes?.missing ?? []) {
        incrementStats(statsByValidator, validatorId, 'prevotes');
      }
      for (const validatorId of roundState.precommits?.missing ?? []) {
        incrementStats(statsByValidator, validatorId, 'precommits');
      }
    }

    pollCount += 1;
    printSummary(statsByValidator, response, pollCount);
    await sleep(intervalMs);
  }
}

main().catch((error) => {
  logger.error(error);
  process.exit(1);
});