import { bitcoinThresholdSignerKeygenSuccessReportedEvent } from '../../bitcoinThresholdSigner/keygenSuccessReported';
import { evmThresholdSignerKeygenSuccessReportedEvent } from '../../evmThresholdSigner/keygenSuccessReported';
import { polkadotThresholdSignerKeygenSuccessReportedEvent } from '../../polkadotThresholdSigner/keygenSuccessReported';
import { solanaThresholdSignerKeygenSuccessReportedEvent } from '../../solanaThresholdSigner/keygenSuccessReported';

export const thresholdSignerKeygenSuccessReportedEvent = {
  Bitcoin: bitcoinThresholdSignerKeygenSuccessReportedEvent,
  Evm: evmThresholdSignerKeygenSuccessReportedEvent,
  Polkadot: polkadotThresholdSignerKeygenSuccessReportedEvent,
  Solana: solanaThresholdSignerKeygenSuccessReportedEvent,
} as const;
