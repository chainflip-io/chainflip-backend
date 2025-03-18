import { globalLogger, Logger } from './logger';
import { SwapContext } from './swap_context';

export class TestContext {
  public swapContext: SwapContext;

  public logger: Logger;

  constructor() {
    this.swapContext = new SwapContext();
    this.logger = globalLogger;
  }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  trace(msg: string, ...args: any[]) {
    this.logger.trace(msg, ...args);
  }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  debug(msg: string, ...args: any[]) {
    this.logger.debug(msg, ...args);
  }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  info(msg: string, ...args: any[]) {
    this.logger.info(msg, ...args);
  }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  warn(msg: string, ...args: any[]) {
    this.logger.warn(msg, ...args);
  }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  error(msg: string, ...args: any[]) {
    this.logger.error(msg, ...args);
  }

  printReport() {
    this.swapContext.printReport(this.logger);
  }
}
