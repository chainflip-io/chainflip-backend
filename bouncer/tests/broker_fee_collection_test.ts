#!/usr/bin/env -S pnpm tsx
import { runWithTimeout } from '../shared/utils';
import { testBrokerFeeCollection } from '../shared/broker_fee_collection';

async function main(): Promise<void> {
  await testBrokerFeeCollection();
  process.exit(0);
}

runWithTimeout(main(), 1200000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
