import { evmThresholdSignerFailureReportProcessedEvent } from '../../evmThresholdSigner/failureReportProcessed';
import { polkadotThresholdSignerFailureReportProcessedEvent } from '../../polkadotThresholdSigner/failureReportProcessed';
import { bitcoinThresholdSignerFailureReportProcessedEvent } from '../../bitcoinThresholdSigner/failureReportProcessed';
import { solanaThresholdSignerFailureReportProcessedEvent } from '../../solanaThresholdSigner/failureReportProcessed';

export const thresholdSignerFailureReportProcessedEvent = {
  Arbitrum: evmThresholdSignerFailureReportProcessedEvent,
  Assethub: polkadotThresholdSignerFailureReportProcessedEvent,
  Bitcoin: bitcoinThresholdSignerFailureReportProcessedEvent,
  Ethereum: evmThresholdSignerFailureReportProcessedEvent,
  Polkadot: polkadotThresholdSignerFailureReportProcessedEvent,
  Solana: solanaThresholdSignerFailureReportProcessedEvent,
} as const;
