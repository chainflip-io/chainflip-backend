import { existsSync, readFileSync, writeFileSync } from 'fs';
import { afterEach, beforeEach, it } from 'vitest';
import { TestContext } from 'shared/utils/test_context';
import { runWithTimeout, testInfoFile } from 'shared/utils';
import { getTestLogFile } from './logger';

// Write the test name and function name to a file to be used by the `run_test.ts` command
function writeTestInfoFile(name: string, functionName: string) {
  try {
    const existingContent = existsSync(testInfoFile) ? readFileSync(testInfoFile, 'utf-8') : '';
    const newEntry = `${name},${functionName}\n`;
    if (!existingContent.includes(newEntry)) {
      writeFileSync(testInfoFile, existingContent + newEntry);
    }
  } catch (e) {
    // This file is not needed for tests to run, so we just log and continue
    console.log('Unable to write test info', e);
  }
}
// Associate a test name with a function name to be used by the `run_test.ts` command.
export function manuallyAddTestToList(name: string, functionName: string) {
  writeTestInfoFile(name, functionName);
}

// Create a new SwapContext for each test
beforeEach<{ testContext: TestContext }>((context) => {
  context.testContext = new TestContext();
});
// Print the SwapContext report after each test finishes
afterEach<{ testContext: TestContext }>((context) => {
  context.testContext.printReport();
});

function createTestFunction(
  name: string,
  timeoutSeconds: number,
  testFunction: (context: TestContext) => Promise<void>,
) {
  return async (context: { testContext: TestContext }) => {
    // Attach the test name to the logger
    context.testContext.logger = context.testContext.logger.child({ test: name });
    context.testContext.logger.info(`🧪 Starting test ${name}`);

    // Run the test with the test context
    await runWithTimeout(testFunction(context.testContext), timeoutSeconds).catch((error) => {
      // We must catch the error here to be able to log it
      context.testContext.error(error);

      // get local logs from file and append them to the error
      const testLogFileName = getTestLogFile(context.testContext.logger);
      const logs = readFileSync(testLogFileName);

      // re-throw error with logs
      throw new Error(`${error}\n\nhistory:\n${logs}`);
    });
    context.testContext.logger.info(`✅ Finished test ${name}`);
  };
}
export function concurrentTest(
  name: string,
  testFunction: (context: TestContext) => Promise<void>,
  timeoutSeconds: number,
  // Only affects the being able to run via the`run_test` command.
  excludeFromList: boolean = false,
) {
  it.concurrent<{ testContext: TestContext }>(
    name,
    createTestFunction(name, timeoutSeconds, testFunction),
    // we catch the timeout manually inside `createTestFunction` so that we can print the test logs.
    // the timeout here is a fallback and should never trigger:
    (timeoutSeconds + 5) * 1000,
  );

  if (!excludeFromList) {
    writeTestInfoFile(name, testFunction.name);
  }
}
export function serialTest(
  name: string,
  testFunction: (context: TestContext) => Promise<void>,
  timeoutSeconds: number,
  // Only affects the being able to run via the`run_test` command.
  excludeFromList: boolean = false,
) {
  it.sequential<{ testContext: TestContext }>(
    name,
    createTestFunction(name, timeoutSeconds, testFunction),
    // we catch the timeout manually inside `createTestFunction` so that we can print the test logs.
    // the timeout here is a fallback and should never trigger:
    (timeoutSeconds + 5) * 1000,
  );

  if (!excludeFromList) {
    writeTestInfoFile(name, testFunction.name);
  }
}
