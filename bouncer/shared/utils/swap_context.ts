import assert from 'assert';
import { globalLogger, Logger, loggerError, throwError } from 'shared/utils/logger';

export enum SwapStatus {
  Initiated,
  Funded,
  VaultSwapInitiated,
  VaultSwapScheduled,
  SwapScheduled,
  SwapCompleted,
  EgressScheduled,
  Success,
  Failure,
}

export class SwapContext {
  allSwaps: Map<string, SwapStatus>;

  constructor() {
    this.allSwaps = new Map();
  }

  updateStatus(logger: Logger, status: SwapStatus) {
    // Get the tag from the logger
    const tag = logger.bindings().tag;
    if (!tag) {
      throwError(
        logger,
        new Error(`No tag found on logger when trying to update swap status to ${status}`),
      );
    }
    const currentStatus = this.allSwaps.get(tag);

    const errorMessage = (from: SwapStatus | undefined, to: SwapStatus) => new Error(
        `Unexpected status transition from ${from !== undefined ? SwapStatus[from] : 'undefined'} to ${SwapStatus[to]}`,
      );

    // State transition checks:
    switch (status) {
      case SwapStatus.Initiated: {
        assert(
          currentStatus === undefined,
          loggerError(logger, errorMessage(currentStatus, status)),
        );
        break;
      }
      case SwapStatus.Funded: {
        assert(
          currentStatus === SwapStatus.Initiated,
          loggerError(logger, errorMessage(currentStatus, status)),
        );
        break;
      }
      case SwapStatus.VaultSwapInitiated: {
        assert(
          currentStatus === SwapStatus.Initiated,
          loggerError(logger, errorMessage(currentStatus, status)),
        );
        break;
      }
      case SwapStatus.VaultSwapScheduled: {
        assert(
          currentStatus === SwapStatus.VaultSwapInitiated,
          loggerError(logger, errorMessage(currentStatus, status)),
        );
        break;
      }
      case SwapStatus.SwapScheduled: {
        assert(
          currentStatus === SwapStatus.Funded,
          loggerError(logger, errorMessage(currentStatus, status)),
        );
        break;
      }
      case SwapStatus.SwapCompleted: {
        assert(
          currentStatus === SwapStatus.SwapScheduled ||
            currentStatus === SwapStatus.VaultSwapScheduled,
          loggerError(logger, errorMessage(currentStatus, status)),
        );
        break;
      }
      case SwapStatus.EgressScheduled: {
        assert(
          currentStatus === SwapStatus.SwapCompleted ||
            currentStatus === SwapStatus.VaultSwapScheduled,
          loggerError(logger, errorMessage(currentStatus, status)),
        );
        break;
      }
      case SwapStatus.Success: {
        assert(
          currentStatus === SwapStatus.EgressScheduled ||
            currentStatus === SwapStatus.VaultSwapScheduled,
          loggerError(logger, errorMessage(currentStatus, status)),
        );
        break;
      }
      default:
        // nothing to do
        break;
    }

    this.allSwaps.set(tag, status);
  }

  printReport(logger: Logger = globalLogger) {
    const unsuccessfulSwapsEntries: string[] = [];
    this.allSwaps.forEach((status, tag) => {
      if (status !== SwapStatus.Success) {
        unsuccessfulSwapsEntries.push(`${tag}: ${SwapStatus[status]}`);
      }
    });

    if (this.allSwaps.size > 0) {
      if (unsuccessfulSwapsEntries.length === 0) {
        logger.info(`✅ All ${this.allSwaps.size} swaps were successful`);
      } else {
        let report = `❌ Unsuccessful swaps:\n`;
        report += unsuccessfulSwapsEntries.join('\n');
        logger.error(report);
      }
    }
  }
}
