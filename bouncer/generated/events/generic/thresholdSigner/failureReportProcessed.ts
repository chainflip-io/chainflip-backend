import { bitcoinThresholdSignerFailureReportProcessedEvent } from '../../bitcoinThresholdSigner/failureReportProcessed';
import { evmThresholdSignerFailureReportProcessedEvent } from '../../evmThresholdSigner/failureReportProcessed';
import { polkadotThresholdSignerFailureReportProcessedEvent } from '../../polkadotThresholdSigner/failureReportProcessed';
import { solanaThresholdSignerFailureReportProcessedEvent } from '../../solanaThresholdSigner/failureReportProcessed';

export const thresholdSignerFailureReportProcessedEvent = {
  Bitcoin: bitcoinThresholdSignerFailureReportProcessedEvent,
  Evm: evmThresholdSignerFailureReportProcessedEvent,
  Polkadot: polkadotThresholdSignerFailureReportProcessedEvent,
  Solana: solanaThresholdSignerFailureReportProcessedEvent,
} as const;
