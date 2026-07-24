import { arbitrumElectionsAllVotesClearedEvent } from '../../arbitrumElections/allVotesCleared';
import { bitcoinElectionsAllVotesClearedEvent } from '../../bitcoinElections/allVotesCleared';
import { bscElectionsAllVotesClearedEvent } from '../../bscElections/allVotesCleared';
import { ethereumElectionsAllVotesClearedEvent } from '../../ethereumElections/allVotesCleared';
import { genericElectionsAllVotesClearedEvent } from '../../genericElections/allVotesCleared';
import { solanaElectionsAllVotesClearedEvent } from '../../solanaElections/allVotesCleared';
import { tronElectionsAllVotesClearedEvent } from '../../tronElections/allVotesCleared';

export const electionsAllVotesClearedEvent = {
  Arbitrum: arbitrumElectionsAllVotesClearedEvent,
  Bitcoin: bitcoinElectionsAllVotesClearedEvent,
  Bsc: bscElectionsAllVotesClearedEvent,
  Ethereum: ethereumElectionsAllVotesClearedEvent,
  Generic: genericElectionsAllVotesClearedEvent,
  Solana: solanaElectionsAllVotesClearedEvent,
  Tron: tronElectionsAllVotesClearedEvent,
} as const;
