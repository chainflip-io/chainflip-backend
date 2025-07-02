#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes no arguments.
// It will update all election properties to be suitable for localnet.
// For example: ./commands/setup_elections.ts

import { setupElections } from 'shared/setup_elections';
import { globalLogger, loggerChild } from 'shared/utils/logger';

async function main(): Promise<void> {
  const logger = loggerChild(globalLogger, 'setup_elections');
  await setupElections(logger);
  process.exit(0);
}

main().catch((error) => {
  console.error(error);
  process.exit(-1);
});
