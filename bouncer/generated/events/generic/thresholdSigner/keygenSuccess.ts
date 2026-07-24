import { evmThresholdSignerKeygenSuccessEvent } from '../../evmThresholdSigner/keygenSuccess';
import { polkadotThresholdSignerKeygenSuccessEvent } from '../../polkadotThresholdSigner/keygenSuccess';
import { bitcoinThresholdSignerKeygenSuccessEvent } from '../../bitcoinThresholdSigner/keygenSuccess';
import { solanaThresholdSignerKeygenSuccessEvent } from '../../solanaThresholdSigner/keygenSuccess';

export const thresholdSignerKeygenSuccessEvent = {
  Arbitrum: evmThresholdSignerKeygenSuccessEvent,
  Assethub: polkadotThresholdSignerKeygenSuccessEvent,
  Bitcoin: bitcoinThresholdSignerKeygenSuccessEvent,
  Ethereum: evmThresholdSignerKeygenSuccessEvent,
  Polkadot: polkadotThresholdSignerKeygenSuccessEvent,
  Solana: solanaThresholdSignerKeygenSuccessEvent,
} as const;
