import { evmThresholdSignerSignersUnavailableEvent } from '../../evmThresholdSigner/signersUnavailable';
import { polkadotThresholdSignerSignersUnavailableEvent } from '../../polkadotThresholdSigner/signersUnavailable';
import { bitcoinThresholdSignerSignersUnavailableEvent } from '../../bitcoinThresholdSigner/signersUnavailable';
import { solanaThresholdSignerSignersUnavailableEvent } from '../../solanaThresholdSigner/signersUnavailable';

export const thresholdSignerSignersUnavailableEvent = {
  Arbitrum: evmThresholdSignerSignersUnavailableEvent,
  Assethub: polkadotThresholdSignerSignersUnavailableEvent,
  Bitcoin: bitcoinThresholdSignerSignersUnavailableEvent,
  Ethereum: evmThresholdSignerSignersUnavailableEvent,
  Polkadot: polkadotThresholdSignerSignersUnavailableEvent,
  Solana: solanaThresholdSignerSignersUnavailableEvent,
} as const;
