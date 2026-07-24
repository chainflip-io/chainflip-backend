import { evmThresholdSignerKeygenSuccessReportedEvent } from '../../evmThresholdSigner/keygenSuccessReported';
import { polkadotThresholdSignerKeygenSuccessReportedEvent } from '../../polkadotThresholdSigner/keygenSuccessReported';
import { bitcoinThresholdSignerKeygenSuccessReportedEvent } from '../../bitcoinThresholdSigner/keygenSuccessReported';
import { solanaThresholdSignerKeygenSuccessReportedEvent } from '../../solanaThresholdSigner/keygenSuccessReported';

export const thresholdSignerKeygenSuccessReportedEvent = {
  Arbitrum: evmThresholdSignerKeygenSuccessReportedEvent,
  Assethub: polkadotThresholdSignerKeygenSuccessReportedEvent,
  Bitcoin: bitcoinThresholdSignerKeygenSuccessReportedEvent,
  Ethereum: evmThresholdSignerKeygenSuccessReportedEvent,
  Polkadot: polkadotThresholdSignerKeygenSuccessReportedEvent,
  Solana: solanaThresholdSignerKeygenSuccessReportedEvent,
} as const;
