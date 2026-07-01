import { evmThresholdSignerKeygenRequestEvent } from '../../evmThresholdSigner/keygenRequest';
import { polkadotThresholdSignerKeygenRequestEvent } from '../../polkadotThresholdSigner/keygenRequest';
import { bitcoinThresholdSignerKeygenRequestEvent } from '../../bitcoinThresholdSigner/keygenRequest';
import { solanaThresholdSignerKeygenRequestEvent } from '../../solanaThresholdSigner/keygenRequest';

export const thresholdSignerKeygenRequestEvent = {
  Arbitrum: evmThresholdSignerKeygenRequestEvent,
  Assethub: polkadotThresholdSignerKeygenRequestEvent,
  Bitcoin: bitcoinThresholdSignerKeygenRequestEvent,
  Ethereum: evmThresholdSignerKeygenRequestEvent,
  Polkadot: polkadotThresholdSignerKeygenRequestEvent,
  Solana: solanaThresholdSignerKeygenRequestEvent,
} as const;
