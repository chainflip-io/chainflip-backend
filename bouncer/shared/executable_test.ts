import { SwapContext } from './swap_context';
import { runWithTimeout } from './utils';

export enum TestStatus {
  Ready = 'ready',
  Running = 'running',
  Complete = 'complete',
  Failed = 'failed',
}

export class ExecutableTest {
  public status: TestStatus = TestStatus.Ready;

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  private runFunction: (...args: any[]) => Promise<void>;

  timeoutSeconds: number;

  debug = false;

  public swapContext: SwapContext;

  constructor(
    public name: string,
    testFunction: () => Promise<void>,
    timeoutSeconds: number,
  ) {
    this.runFunction = testFunction;
    this.timeoutSeconds = timeoutSeconds;
    this.swapContext = new SwapContext();
  }

  /// Run the test with the pre-defined timeout and any given arguments
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  async run(...args: any[]) {
    if (this.status === TestStatus.Running) {
      throw new Error(`${this.name} Test is already running`);
    }
    console.log('\x1b[36m%s\x1b[0m', `=== Running ${this.name} test ===`);
    this.status = TestStatus.Running;
    await runWithTimeout(this.runFunction(...args), this.timeoutSeconds * 1000).catch((error) => {
      // Print the swap report if the swap context was used
      if (this.swapContext.allSwaps.size > 0) {
        this.swapContext.print_report();
      }
      // Print a timestamped error message with the test name
      const now = new Date();
      const timestamp = `${now.getHours()}:${now.getMinutes()}:${now.getSeconds()}`;
      console.error('\x1b[41m%s\x1b[0m', `=== ${this.name} test failed (${timestamp}) ===`);
      this.status = TestStatus.Failed;
      // Rethrow the error to be caught by the caller
      throw error;
    });
    this.status = TestStatus.Complete;
    console.log('\x1b[32m%s\x1b[0m', `=== ${this.name} test complete ===`);
  }

  /// Log a message with the test name prefixed
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  log(message: string, ...optionalParams: any[]) {
    console.log('\x1b[1m%s\x1b[0m', `[${this.name}] ${message}`, ...optionalParams);
  }

  /// Only logs a message if the debug flag is enabled for this test
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  debugLog(message: string, ...optionalParams: any[]) {
    if (this.debug) {
      console.log('\x1b[30m%s\x1b[0m', `[${this.name}] ${message}`, ...optionalParams);
    }
  }

  /// Runs the test with debug enabled and handles exiting the process. Used when running the test as a command.
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  async execute(...args: any[]) {
    const start = Date.now();

    this.debug = true;
    await this.run(...args).catch((error) => {
      console.error(error);
      process.exit(-1);
    });

    const executionTime = (Date.now() - start) / 1000;
    if (executionTime > this.timeoutSeconds * 0.9) {
      console.warn(
        `\x1b[33m%s\x1b[0m`,
        `Warning: Execution time of ${this.name} test was close to the timeout: ${executionTime}/${this.timeoutSeconds}s`,
      );
    } else {
      this.log(`Execution time: ${executionTime}/${this.timeoutSeconds}s`);
    }
    process.exit(0);
  }
}
