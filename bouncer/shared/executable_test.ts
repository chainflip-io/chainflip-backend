import { SwapContext } from './swap_context';
import { ConsoleLogColors, getTimeStamp, runWithTimeout } from './utils';

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

  private runFunction: (...args: unknown[]) => Promise<void>;

  timeoutSeconds: number;

  public debug = false;

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
  async run(...args: unknown[]) {
    if (this.status === TestStatus.Running) {
      throw new Error(`${this.name} Test is already running`);
    }
    console.log(ConsoleLogColors.LightBlue, `=== Running ${this.name} test ===`);
    this.status = TestStatus.Running;
    await runWithTimeout(this.runFunction(...args), this.timeoutSeconds).catch((error) => {
      // Print the swap report if the swap context was used
      if (this.swapContext.allSwaps.size > 0) {
        this.swapContext.print_report();
      }
      // Print a timestamped error message with the test name
      console.error(
        ConsoleLogColors.RedSolid,
        `=== ${this.name} test failed (${getTimeStamp()}) ===`,
      );
      this.status = TestStatus.Failed;
      // Rethrow the error to be caught by the caller
      throw error;
    });
    this.status = TestStatus.Complete;
    console.log(ConsoleLogColors.Green, `=== ${this.name} test complete ===`);
  }

  private logWithName(color: string, message: string, ...optionalParams: unknown[]) {
    console.log(color, `[${this.name}] ${message}`, ...optionalParams);
  }

  /// Log a message with the test name prefixed
  log(message: string, ...optionalParams: unknown[]) {
    this.logWithName(ConsoleLogColors.WhiteBold, message, ...optionalParams);
  }

  /// Only logs a message if the debug flag is enabled for this test
  debugLog(message: string, ...optionalParams: unknown[]) {
    if (this.debug) {
      this.logWithName(ConsoleLogColors.Grey, message, ...optionalParams);
    }
  }

  /// Runs the test with debug enabled and handles exiting the process. Used when running the test as a command.
  async runAndExit(...args: unknown[]) {
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
