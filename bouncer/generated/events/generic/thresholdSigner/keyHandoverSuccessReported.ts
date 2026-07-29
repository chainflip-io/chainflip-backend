import { bitcoinThresholdSignerKeyHandoverSuccessReportedEvent } from '../../bitcoinThresholdSigner/keyHandoverSuccessReported';
import { evmThresholdSignerKeyHandoverSuccessReportedEvent } from '../../evmThresholdSigner/keyHandoverSuccessReported';
import { polkadotThresholdSignerKeyHandoverSuccessReportedEvent } from '../../polkadotThresholdSigner/keyHandoverSuccessReported';
import { solanaThresholdSignerKeyHandoverSuccessReportedEvent } from '../../solanaThresholdSigner/keyHandoverSuccessReported';

export const thresholdSignerKeyHandoverSuccessReportedEvent = {
  Bitcoin: bitcoinThresholdSignerKeyHandoverSuccessReportedEvent,
  Evm: evmThresholdSignerKeyHandoverSuccessReportedEvent,
  Polkadot: polkadotThresholdSignerKeyHandoverSuccessReportedEvent,
  Solana: solanaThresholdSignerKeyHandoverSuccessReportedEvent,
} as const;
