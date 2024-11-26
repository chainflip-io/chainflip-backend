import assert from 'assert';

export enum SwapStatus {
  Initiated,
  Funded,
  VaultSwapInitiated,
  VaultSwapScheduled,
  SwapScheduled,
  Success,
  Failure,
}

export class SwapContext {
  allSwaps: Map<string, SwapStatus>;

  constructor() {
    this.allSwaps = new Map();
  }

  updateStatus(tag: string, status: SwapStatus) {
    const currentStatus = this.allSwaps.get(tag);

    // Sanity checks:
    switch (status) {
      case SwapStatus.Initiated: {
        assert(
          currentStatus === undefined,
          `Unexpected status transition for ${tag}. Transitioning from ${currentStatus} to ${status}`,
        );
        break;
      }
      case SwapStatus.Funded: {
        assert(
          currentStatus === SwapStatus.Initiated,
          `Unexpected status transition for ${tag}. Transitioning from ${currentStatus} to ${status}`,
        );
        break;
      }
      case SwapStatus.VaultSwapInitiated: {
        assert(
          currentStatus === SwapStatus.Initiated,
          `Unexpected status transition for ${tag}. Transitioning from ${currentStatus} to ${status}`,
        );
        break;
      }
      case SwapStatus.VaultSwapScheduled: {
        assert(
          currentStatus === SwapStatus.VaultSwapInitiated,
          `Unexpected status transition for ${tag}. Transitioning from ${currentStatus} to ${status}`,
        );
        break;
      }
      case SwapStatus.SwapScheduled: {
        assert(
          currentStatus === SwapStatus.Funded,
          `Unexpected status transition for ${tag}. Transitioning from ${currentStatus} to ${status}`,
        );
        break;
      }
      case SwapStatus.Success: {
        assert(
          currentStatus === SwapStatus.SwapScheduled ||
            currentStatus === SwapStatus.VaultSwapScheduled,
          `Unexpected status transition for ${tag}. Transitioning from ${currentStatus} to ${status}`,
        );
        break;
      }
      default:
        // nothing to do
        break;
    }

    this.allSwaps.set(tag, status);
  }

  print_report() {
    const unsuccessfulSwapsEntries: string[] = [];
    this.allSwaps.forEach((status, tag) => {
      if (status !== SwapStatus.Success) {
        unsuccessfulSwapsEntries.push(`${tag}: ${SwapStatus[status]}`);
      }
    });

    if (unsuccessfulSwapsEntries.length === 0) {
      console.log('All swaps are successful!');
    } else {
      let report = `Unsuccessful swaps:\n`;
      report += unsuccessfulSwapsEntries.join('\n');
      console.error(report);
    }
  }
}
