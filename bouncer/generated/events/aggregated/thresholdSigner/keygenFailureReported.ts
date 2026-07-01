import { evmThresholdSignerKeygenFailureReportedEvent } from '../../evmThresholdSigner/keygenFailureReported';
import { polkadotThresholdSignerKeygenFailureReportedEvent } from '../../polkadotThresholdSigner/keygenFailureReported';
import { bitcoinThresholdSignerKeygenFailureReportedEvent } from '../../bitcoinThresholdSigner/keygenFailureReported';
import { solanaThresholdSignerKeygenFailureReportedEvent } from '../../solanaThresholdSigner/keygenFailureReported';

export const thresholdSignerKeygenFailureReportedEvent = {
  Arbitrum: evmThresholdSignerKeygenFailureReportedEvent,
  Assethub: polkadotThresholdSignerKeygenFailureReportedEvent,
  Bitcoin: bitcoinThresholdSignerKeygenFailureReportedEvent,
  Ethereum: evmThresholdSignerKeygenFailureReportedEvent,
  Polkadot: polkadotThresholdSignerKeygenFailureReportedEvent,
  Solana: solanaThresholdSignerKeygenFailureReportedEvent,
} as const;
