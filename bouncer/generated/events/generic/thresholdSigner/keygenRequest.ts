import { bitcoinThresholdSignerKeygenRequestEvent } from '../../bitcoinThresholdSigner/keygenRequest';
import { evmThresholdSignerKeygenRequestEvent } from '../../evmThresholdSigner/keygenRequest';
import { polkadotThresholdSignerKeygenRequestEvent } from '../../polkadotThresholdSigner/keygenRequest';
import { solanaThresholdSignerKeygenRequestEvent } from '../../solanaThresholdSigner/keygenRequest';

export const thresholdSignerKeygenRequestEvent = {
  Bitcoin: bitcoinThresholdSignerKeygenRequestEvent,
  Evm: evmThresholdSignerKeygenRequestEvent,
  Polkadot: polkadotThresholdSignerKeygenRequestEvent,
  Solana: solanaThresholdSignerKeygenRequestEvent,
} as const;
