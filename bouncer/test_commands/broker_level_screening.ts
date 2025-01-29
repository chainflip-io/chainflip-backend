#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command will run the broker-level-screening-test.
// Note that the deposit monitor has to be running. Takes an
// optional parameter, deciding whether it should test boosted
// deposits or not.
//
// For example: ./test_commands/broker_level_screening.ts
// will run a single test to reject a non-boosted deposit
//
// For example: ./test_commands/broker_level_screening.ts testBoostedDeposits
// will run three tests:
//  - reject a non-boosted deposit
//  - reject a boosted deposit
//  - don't reject a boosted deposit which was reported too late


import { testBrokerLevelScreening } from '../tests/broker_level_screening';

let testBoostedDeposits = false;
if (process.argv.length > 1) {
    testBoostedDeposits = process.argv[2] == 'testBoostedDeposits';
}

await testBrokerLevelScreening.run(testBoostedDeposits);
