import { bitcoinThresholdSignerKeyHandoverFailureReportedEvent } from '../../bitcoinThresholdSigner/keyHandoverFailureReported';
import { evmThresholdSignerKeyHandoverFailureReportedEvent } from '../../evmThresholdSigner/keyHandoverFailureReported';
import { polkadotThresholdSignerKeyHandoverFailureReportedEvent } from '../../polkadotThresholdSigner/keyHandoverFailureReported';
import { solanaThresholdSignerKeyHandoverFailureReportedEvent } from '../../solanaThresholdSigner/keyHandoverFailureReported';

export const thresholdSignerKeyHandoverFailureReportedEvent = {
  Bitcoin: bitcoinThresholdSignerKeyHandoverFailureReportedEvent,
  Evm: evmThresholdSignerKeyHandoverFailureReportedEvent,
  Polkadot: polkadotThresholdSignerKeyHandoverFailureReportedEvent,
  Solana: solanaThresholdSignerKeyHandoverFailureReportedEvent,
} as const;
