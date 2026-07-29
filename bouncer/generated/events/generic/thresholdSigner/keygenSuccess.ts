import { bitcoinThresholdSignerKeygenSuccessEvent } from '../../bitcoinThresholdSigner/keygenSuccess';
import { evmThresholdSignerKeygenSuccessEvent } from '../../evmThresholdSigner/keygenSuccess';
import { polkadotThresholdSignerKeygenSuccessEvent } from '../../polkadotThresholdSigner/keygenSuccess';
import { solanaThresholdSignerKeygenSuccessEvent } from '../../solanaThresholdSigner/keygenSuccess';

export const thresholdSignerKeygenSuccessEvent = {
  Bitcoin: bitcoinThresholdSignerKeygenSuccessEvent,
  Evm: evmThresholdSignerKeygenSuccessEvent,
  Polkadot: polkadotThresholdSignerKeygenSuccessEvent,
  Solana: solanaThresholdSignerKeygenSuccessEvent,
} as const;
