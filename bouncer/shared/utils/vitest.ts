import { afterEach, beforeEach, it } from 'vitest';
import { TestContext } from './test_context';

// Create a new SwapContext for each test
beforeEach<{ testContext: TestContext }>((context) => {
  context.testContext = new TestContext();
});
// Print the SwapContext report after each test finishes
afterEach<{ testContext: TestContext }>((context) => {
  context.testContext.printReport();
});

export function concurrentTest(
  name: string,
  testFunction: (context: TestContext) => Promise<void>,
  timeoutSeconds: number,
) {
  it.concurrent<{ testContext: TestContext }>(
    name,
    async (context) => {
      // Attach the test name to the logger
      context.testContext.logger = context.testContext.logger.child({ test: name });
      // Run the test with the test context
      await testFunction(context.testContext).catch((error) => {
        // We must catch the error here to be able to log it
        context.testContext.error(error);
        throw error;
      });
    },
    timeoutSeconds * 1000,
  );
}

export function serialTest(
  name: string,
  testFunction: (context: TestContext) => Promise<void>,
  timeoutSeconds: number,
) {
  it<{ testContext: TestContext }>(
    name,
    async (context) => {
      // Attach the test name to the logger
      context.testContext.logger = context.testContext.logger.child({ test: name });
      // Run the test with the test context
      await testFunction(context.testContext).catch((error) => {
        // We must catch the error here to be able to log it
        context.testContext.error(error);
        throw error;
      });
    },
    timeoutSeconds * 1000,
  );
}
