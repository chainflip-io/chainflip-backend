import { bitcoinThresholdSignerSignersUnavailableEvent } from '../../bitcoinThresholdSigner/signersUnavailable';
import { evmThresholdSignerSignersUnavailableEvent } from '../../evmThresholdSigner/signersUnavailable';
import { polkadotThresholdSignerSignersUnavailableEvent } from '../../polkadotThresholdSigner/signersUnavailable';
import { solanaThresholdSignerSignersUnavailableEvent } from '../../solanaThresholdSigner/signersUnavailable';

export const thresholdSignerSignersUnavailableEvent = {
  Bitcoin: bitcoinThresholdSignerSignersUnavailableEvent,
  Evm: evmThresholdSignerSignersUnavailableEvent,
  Polkadot: polkadotThresholdSignerSignersUnavailableEvent,
  Solana: solanaThresholdSignerSignersUnavailableEvent,
} as const;
