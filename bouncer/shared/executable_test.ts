import { SwapContext } from './swap_context';
import { ConsoleLogColors, runWithTimeout } from './utils';

export enum TestStatus {
  Ready = 'ready',
  Running = 'running',
  Complete = 'complete',
  Failed = 'failed',
}

/// Returns the file path of the caller
function getCallerFile(stackIndex: number): string {
  const originalFunc = Error.prepareStackTrace;

  try {
    const fakeError = new Error();
    Error.prepareStackTrace = function stackTrace(_err, stack) {
      return stack;
    };
    if (fakeError.stack !== undefined) {
      const stack = fakeError.stack as unknown as NodeJS.CallSite[];
      const callSite: NodeJS.CallSite = stack[stackIndex];
      const fileName = callSite.getFileName();
      Error.prepareStackTrace = originalFunc;
      return fileName ?? '';
    }
  } catch (e) {
    Error.prepareStackTrace = originalFunc;
    return 'error';
  }

  Error.prepareStackTrace = originalFunc;
  return 'error';
}

export class ExecutableTest {
  public status: TestStatus = TestStatus.Ready;

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  private runFunction: (...args: any[]) => Promise<void>;

  timeoutSeconds: number;

  debug = false;

  public swapContext: SwapContext;

  public filePath = '';

  public fileName = '';

  constructor(
    public name: string,
    testFunction: () => Promise<void>,
    timeoutSeconds: number,
  ) {
    this.runFunction = testFunction;
    this.timeoutSeconds = timeoutSeconds;
    this.swapContext = new SwapContext();
    this.filePath = getCallerFile(2);
    this.fileName = this.filePath.split('/').pop() ?? '';
  }

  /// Run the test with the pre-defined timeout and any given arguments
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  async run(...args: any[]) {
    if (this.status === TestStatus.Running) {
      throw new Error(`${this.name} Test is already running`);
    }
    console.log(ConsoleLogColors.LightBlue, `=== Running ${this.name} test ===`);
    this.status = TestStatus.Running;
    await runWithTimeout(this.runFunction(...args), this.timeoutSeconds * 1000).catch((error) => {
      // Print the swap report if the swap context was used
      if (this.swapContext.allSwaps.size > 0) {
        this.swapContext.print_report();
      }
      // Print a timestamped error message with the test name
      const now = new Date();
      const timestamp = `${now.getHours()}:${now.getMinutes()}:${now.getSeconds()}`;
      console.error(ConsoleLogColors.RedSolid, `=== ${this.name} test failed (${timestamp}) ===`);
      this.status = TestStatus.Failed;
      // Rethrow the error to be caught by the caller
      throw error;
    });
    this.status = TestStatus.Complete;
    console.log(ConsoleLogColors.Green, `=== ${this.name} test complete ===`);
  }

  /// Log a message with the test name prefixed
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  log(message: string, ...optionalParams: any[]) {
    console.log(ConsoleLogColors.WhiteBold, `[${this.name}] ${message}`, ...optionalParams);
  }

  /// Only logs a message if the debug flag is enabled for this test
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  debugLog(message: string, ...optionalParams: any[]) {
    if (this.debug) {
      console.log(ConsoleLogColors.Grey, `[${this.name}] ${message}`, ...optionalParams);
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
        ConsoleLogColors.Yellow,
        `Warning: Execution time of ${this.name} test was close to the timeout: ${executionTime}/${this.timeoutSeconds}s`,
      );
    } else {
      this.log(`Execution time: ${executionTime}/${this.timeoutSeconds}s`);
    }
    process.exit(0);
  }
}
