import { bitcoinThresholdSignerKeyHandoverRequestEvent } from '../../bitcoinThresholdSigner/keyHandoverRequest';
import { evmThresholdSignerKeyHandoverRequestEvent } from '../../evmThresholdSigner/keyHandoverRequest';
import { polkadotThresholdSignerKeyHandoverRequestEvent } from '../../polkadotThresholdSigner/keyHandoverRequest';
import { solanaThresholdSignerKeyHandoverRequestEvent } from '../../solanaThresholdSigner/keyHandoverRequest';

export const thresholdSignerKeyHandoverRequestEvent = {
  Bitcoin: bitcoinThresholdSignerKeyHandoverRequestEvent,
  Evm: evmThresholdSignerKeyHandoverRequestEvent,
  Polkadot: polkadotThresholdSignerKeyHandoverRequestEvent,
  Solana: solanaThresholdSignerKeyHandoverRequestEvent,
} as const;
