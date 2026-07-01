import { arbitrumElectionsCorruptStorageEvent } from '../../arbitrumElections/corruptStorage';
import { bitcoinElectionsCorruptStorageEvent } from '../../bitcoinElections/corruptStorage';
import { bscElectionsCorruptStorageEvent } from '../../bscElections/corruptStorage';
import { ethereumElectionsCorruptStorageEvent } from '../../ethereumElections/corruptStorage';
import { solanaElectionsCorruptStorageEvent } from '../../solanaElections/corruptStorage';
import { tronElectionsCorruptStorageEvent } from '../../tronElections/corruptStorage';

export const electionsCorruptStorageEvent = {
  Arbitrum: arbitrumElectionsCorruptStorageEvent,
  Bitcoin: bitcoinElectionsCorruptStorageEvent,
  Bsc: bscElectionsCorruptStorageEvent,
  Ethereum: ethereumElectionsCorruptStorageEvent,
  Solana: solanaElectionsCorruptStorageEvent,
  Tron: tronElectionsCorruptStorageEvent,
} as const;
