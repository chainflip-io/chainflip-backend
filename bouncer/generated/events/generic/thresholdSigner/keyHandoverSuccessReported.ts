import { evmThresholdSignerKeyHandoverSuccessReportedEvent } from '../../evmThresholdSigner/keyHandoverSuccessReported';
import { polkadotThresholdSignerKeyHandoverSuccessReportedEvent } from '../../polkadotThresholdSigner/keyHandoverSuccessReported';
import { bitcoinThresholdSignerKeyHandoverSuccessReportedEvent } from '../../bitcoinThresholdSigner/keyHandoverSuccessReported';
import { solanaThresholdSignerKeyHandoverSuccessReportedEvent } from '../../solanaThresholdSigner/keyHandoverSuccessReported';

export const thresholdSignerKeyHandoverSuccessReportedEvent = {
  Arbitrum: evmThresholdSignerKeyHandoverSuccessReportedEvent,
  Assethub: polkadotThresholdSignerKeyHandoverSuccessReportedEvent,
  Bitcoin: bitcoinThresholdSignerKeyHandoverSuccessReportedEvent,
  Ethereum: evmThresholdSignerKeyHandoverSuccessReportedEvent,
  Polkadot: polkadotThresholdSignerKeyHandoverSuccessReportedEvent,
  Solana: solanaThresholdSignerKeyHandoverSuccessReportedEvent,
} as const;
