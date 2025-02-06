import { afterEach, beforeEach, it } from 'vitest';
import { SwapContext, TestContext } from '../swap_context';
import { logger } from './logger';

// Create a new SwapContext for each test
beforeEach<{ testContext: TestContext }>((context) => {
  context.testContext = {
    swapContext: new SwapContext(),
    logger,
  };
});
// Print the SwapContext report after the test finishes
afterEach<{ testContext: TestContext }>((context) => {
  context.testContext.swapContext.print_report(context.testContext.logger);
});

export function concurrentTest(
  name: string,
  testFunction: (context: TestContext) => Promise<void>,
  timeoutSeconds: number,
) {
  it.concurrent<{ testContext: TestContext }>(
    name,
    async (context) => {
      context.testContext.logger = context.testContext.logger.child({ test: name });
      await testFunction(context.testContext).catch((error) => {
        context.testContext.logger.error(error);
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
      context.testContext.logger = context.testContext.logger.child({ test: name });
      await testFunction(context.testContext).catch((error) => {
        context.testContext.logger.error(error);
        throw error;
      });
    },
    timeoutSeconds * 1000,
  );
}
