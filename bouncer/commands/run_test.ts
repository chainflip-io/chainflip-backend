#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes one argument.
// It will find and run the test function (using vitest) in the test file provided as an argument.
//
// For example: ./commands/run_test.ts ./tests/boost.ts
// This is the equivalent of running `pnpm vitest run -t "BoostingForAsset"`

import { execSync } from 'child_process';
import { existsSync, readFileSync } from 'fs';

// Note: This must be the same path as the one in shared/utils/vitest.ts. Importing it here breaks vitest.
export const testInfoFile = '/tmp/chainflip/test_info.csv';

// Check that a test file was provided as an argument
const testFile = process.argv[2];
if (!testFile) {
  console.error('Usage: ./commands/run_test.ts ./tests/<test_file>');
  process.exit(1);
} else if (!existsSync(testFile)) {
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
const testNamesAndFunctions: { testName: string; functionName: string }[] = [];
try {
  const data = readFileSync(testInfoFile, 'utf8');
  const rows = data.split('\n');
  for (const row of rows) {
    const columns = row.split(',');
    if (columns.length >= 2) {
      testNamesAndFunctions.push({
        testName: columns[0].trim(),
        functionName: columns[1].trim(),
      });
    }
  }
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
    if (data.includes(`function ${functionName}`)) {
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
  execSync(`pnpm vitest run -t "${matchingTestName}"`, { stdio: 'inherit' });
}
