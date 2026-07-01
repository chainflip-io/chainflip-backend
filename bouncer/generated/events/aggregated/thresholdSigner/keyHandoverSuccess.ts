import { evmThresholdSignerKeyHandoverSuccessEvent } from '../../evmThresholdSigner/keyHandoverSuccess';
import { polkadotThresholdSignerKeyHandoverSuccessEvent } from '../../polkadotThresholdSigner/keyHandoverSuccess';
import { bitcoinThresholdSignerKeyHandoverSuccessEvent } from '../../bitcoinThresholdSigner/keyHandoverSuccess';
import { solanaThresholdSignerKeyHandoverSuccessEvent } from '../../solanaThresholdSigner/keyHandoverSuccess';

export const thresholdSignerKeyHandoverSuccessEvent = {
  Arbitrum: evmThresholdSignerKeyHandoverSuccessEvent,
  Assethub: polkadotThresholdSignerKeyHandoverSuccessEvent,
  Bitcoin: bitcoinThresholdSignerKeyHandoverSuccessEvent,
  Ethereum: evmThresholdSignerKeyHandoverSuccessEvent,
  Polkadot: polkadotThresholdSignerKeyHandoverSuccessEvent,
  Solana: solanaThresholdSignerKeyHandoverSuccessEvent,
} as const;
