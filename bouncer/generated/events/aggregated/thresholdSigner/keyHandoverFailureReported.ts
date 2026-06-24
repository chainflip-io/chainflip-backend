import { evmThresholdSignerKeyHandoverFailureReportedEvent } from '../../evmThresholdSigner/keyHandoverFailureReported';
import { polkadotThresholdSignerKeyHandoverFailureReportedEvent } from '../../polkadotThresholdSigner/keyHandoverFailureReported';
import { bitcoinThresholdSignerKeyHandoverFailureReportedEvent } from '../../bitcoinThresholdSigner/keyHandoverFailureReported';
import { solanaThresholdSignerKeyHandoverFailureReportedEvent } from '../../solanaThresholdSigner/keyHandoverFailureReported';

export const thresholdSignerKeyHandoverFailureReportedEvent = {
  Arbitrum: evmThresholdSignerKeyHandoverFailureReportedEvent,
  Assethub: polkadotThresholdSignerKeyHandoverFailureReportedEvent,
  Bitcoin: bitcoinThresholdSignerKeyHandoverFailureReportedEvent,
  Ethereum: evmThresholdSignerKeyHandoverFailureReportedEvent,
  Polkadot: polkadotThresholdSignerKeyHandoverFailureReportedEvent,
  Solana: solanaThresholdSignerKeyHandoverFailureReportedEvent,
} as const;
