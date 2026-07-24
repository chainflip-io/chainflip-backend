import { bitcoinThresholdSignerKeyHandoverSuccessEvent } from '../../bitcoinThresholdSigner/keyHandoverSuccess';
import { evmThresholdSignerKeyHandoverSuccessEvent } from '../../evmThresholdSigner/keyHandoverSuccess';
import { polkadotThresholdSignerKeyHandoverSuccessEvent } from '../../polkadotThresholdSigner/keyHandoverSuccess';
import { solanaThresholdSignerKeyHandoverSuccessEvent } from '../../solanaThresholdSigner/keyHandoverSuccess';

export const thresholdSignerKeyHandoverSuccessEvent = {
  Bitcoin: bitcoinThresholdSignerKeyHandoverSuccessEvent,
  Evm: evmThresholdSignerKeyHandoverSuccessEvent,
  Polkadot: polkadotThresholdSignerKeyHandoverSuccessEvent,
  Solana: solanaThresholdSignerKeyHandoverSuccessEvent,
} as const;
