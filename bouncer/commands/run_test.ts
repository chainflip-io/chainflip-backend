#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes one argument: either a test file or a swap number.
//
// To run a test file:
//   ./commands/run_test.ts ./tests/boost.ts
//   This is the equivalent of running:
//  `BOUNCER_LOG_LEVEL=debug pnpm vitest --maxConcurrency=100 --hideSkippedTests run -t "BoostingForAsset"`
//
// To run a single swap test by number:
//   ./commands/run_test.ts 287
//   This is the equivalent of running:
// `BOUNCER_LOG_LEVEL=debug pnpm vitest --maxConcurrency=100 --hideSkippedTests run tests/fast_bouncer.test.ts -t "Swap 287:"`

import { execSync } from 'child_process';
import { existsSync, readFileSync } from 'fs';
import { testInfoFile } from 'shared/utils';

const arg = process.argv[2];
if (!arg) {
  console.error('Usage: ./commands/run_test.ts ./tests/<test_file>');
  console.error('       ./commands/run_test.ts <swap_number>');
  process.exit(1);
}

// If a swap number is given, run just that single swap test directly.
if (/^\d+$/.test(arg)) {
  const testFilter = `Swap ${arg}:`;
  try {
    execSync(
      `BOUNCER_LOG_LEVEL=debug pnpm vitest --maxConcurrency=100 --hideSkippedTests run tests/fast_bouncer.test.ts -t "${testFilter}"`,
      { stdio: 'inherit' },
    );
  } catch (err) {
    console.error(`Swap test ${arg} failed:`, err);
    process.exit(1);
  }
  process.exit(0);
}

const testFile = arg;
if (!existsSync(testFile)) {
  console.error(`Test file ${testFile} not found`);
  process.exit(1);
}

// Delete the old test info file
try {
  execSync(`rm -f ${testInfoFile}`);
} catch (err) {
  console.error(`Error deleting the ${testInfoFile} file:`, err);
  process.exit(1);
}

// Run the vitest list command, this will cause the test info to be written to the file.
try {
  execSync('pnpm vitest list').toString();
} catch (err) {
  console.error('Error running the vitest list command:', err);
  process.exit(1);
}

// Get the test info that was saved to the file
let testNamesAndFunctions;
try {
  testNamesAndFunctions = readFileSync(testInfoFile, 'utf8')
    .split('\n')
    .filter((row) => row.includes(','))
    .map((row) => {
      const [testName, functionName] = row.split(',').map((col) => col.trim());
      return { testName, functionName };
    });
} catch (err) {
  console.error(`Error reading the ${testInfoFile} file:`, err);
  process.exit(1);
}

// Find a matching function in the given test file
if (!testFile) {
  console.error('Please provide a test file as an argument.');
  process.exit(1);
}
let matchingTestName;
try {
  const data = readFileSync(testFile, 'utf8');
  for (const { testName, functionName } of testNamesAndFunctions) {
    if (functionName !== '' && data.includes(`function ${functionName}`)) {
      // We found a match, this must be the test we want to run
      matchingTestName = testName;
      break;
    }
  }
} catch (err) {
  console.error(`Error reading the test file ${testFile}:`, err);
  process.exit(1);
}

// Run the test using vitest
if (!matchingTestName) {
  console.error('No matching test function found');
  process.exit(1);
} else {
  try {
    execSync(
      `BOUNCER_LOG_LEVEL=debug pnpm vitest --maxConcurrency=100 --hideSkippedTests run -t "${matchingTestName}"`,
      { stdio: 'inherit' },
    );
  } catch (err) {
    console.error(`Test "${matchingTestName}" failed:`, err);
    process.exit(1);
  }
}
