import { arbitrumElectionsUninitializedEvent } from '../../arbitrumElections/uninitialized';
import { assethubElectionsUninitializedEvent } from '../../assethubElections/uninitialized';
import { bitcoinElectionsUninitializedEvent } from '../../bitcoinElections/uninitialized';
import { bscElectionsUninitializedEvent } from '../../bscElections/uninitialized';
import { ethereumElectionsUninitializedEvent } from '../../ethereumElections/uninitialized';
import { genericElectionsUninitializedEvent } from '../../genericElections/uninitialized';
import { solanaElectionsUninitializedEvent } from '../../solanaElections/uninitialized';
import { tronElectionsUninitializedEvent } from '../../tronElections/uninitialized';

export const electionsUninitializedEvent = {
  Arbitrum: arbitrumElectionsUninitializedEvent,
  Assethub: assethubElectionsUninitializedEvent,
  Bitcoin: bitcoinElectionsUninitializedEvent,
  Bsc: bscElectionsUninitializedEvent,
  Ethereum: ethereumElectionsUninitializedEvent,
  Generic: genericElectionsUninitializedEvent,
  Solana: solanaElectionsUninitializedEvent,
  Tron: tronElectionsUninitializedEvent,
} as const;
