import { evmThresholdSignerKeyHandoverRequestEvent } from '../../evmThresholdSigner/keyHandoverRequest';
import { polkadotThresholdSignerKeyHandoverRequestEvent } from '../../polkadotThresholdSigner/keyHandoverRequest';
import { bitcoinThresholdSignerKeyHandoverRequestEvent } from '../../bitcoinThresholdSigner/keyHandoverRequest';
import { solanaThresholdSignerKeyHandoverRequestEvent } from '../../solanaThresholdSigner/keyHandoverRequest';

export const thresholdSignerKeyHandoverRequestEvent = {
  Arbitrum: evmThresholdSignerKeyHandoverRequestEvent,
  Assethub: polkadotThresholdSignerKeyHandoverRequestEvent,
  Bitcoin: bitcoinThresholdSignerKeyHandoverRequestEvent,
  Ethereum: evmThresholdSignerKeyHandoverRequestEvent,
  Polkadot: polkadotThresholdSignerKeyHandoverRequestEvent,
  Solana: solanaThresholdSignerKeyHandoverRequestEvent,
} as const;
