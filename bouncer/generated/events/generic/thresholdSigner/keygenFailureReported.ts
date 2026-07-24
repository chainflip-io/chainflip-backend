import { bitcoinThresholdSignerKeygenFailureReportedEvent } from '../../bitcoinThresholdSigner/keygenFailureReported';
import { evmThresholdSignerKeygenFailureReportedEvent } from '../../evmThresholdSigner/keygenFailureReported';
import { polkadotThresholdSignerKeygenFailureReportedEvent } from '../../polkadotThresholdSigner/keygenFailureReported';
import { solanaThresholdSignerKeygenFailureReportedEvent } from '../../solanaThresholdSigner/keygenFailureReported';

export const thresholdSignerKeygenFailureReportedEvent = {
  Bitcoin: bitcoinThresholdSignerKeygenFailureReportedEvent,
  Evm: evmThresholdSignerKeygenFailureReportedEvent,
  Polkadot: polkadotThresholdSignerKeygenFailureReportedEvent,
  Solana: solanaThresholdSignerKeygenFailureReportedEvent,
} as const;
