import type { Prisma } from '.prisma/client';
import type { Block, Event } from '../gql/generated/graphql';
import { buildHandlerMap, getDispatcher } from '../utils/handlers';
import networkBatchBroadcastRequested from './networkBatchBroadcastRequested';
import networkBroadcastAborted from './networkBroadcastAborted';
import networkBroadcastSuccess from './networkBroadcastSuccess';
import networkEgressScheduled from './networkEgressScheduled';
import swapEgressScheduled from './swapEgressScheduled';
import swapExecuted from './swapExecuted';
import swapScheduled from './swapScheduled';

const values = Object.values as <T>(o: T) => T[keyof T][];

export const swapping = {
  SwapScheduled: 'Swapping.SwapScheduled',
  SwapExecuted: 'Swapping.SwapExecuted',
  SwapEgressScheduled: 'Swapping.SwapEgressScheduled',
} as const;

export const bitcoinIngressEgress = {
  EgressScheduled: 'BitcoinIngressEgress.EgressScheduled',
  BatchBroadcastRequested: 'BitcoinIngressEgress.BatchBroadcastRequested',
} as const;

export const bitcoinBroadcaster = {
  BroadcastSuccess: 'BitcoinBroadcaster.BroadcastSuccess',
  BroadcastAborted: 'BitcoinBroadcaster.BroadcastAborted',
} as const;

export const ethereumIngressEgress = {
  EgressScheduled: 'EthereumIngressEgress.EgressScheduled',
  BatchBroadcastRequested: 'EthereumIngressEgress.BatchBroadcastRequested',
} as const;

export const ethereumBroadcaster = {
  BroadcastSuccess: 'EthereumBroadcaster.BroadcastSuccess',
  BroadcastAborted: 'EthereumBroadcaster.BroadcastAborted',
} as const;

export const polkadotIngressEgress = {
  EgressScheduled: 'PolkadotIngressEgress.EgressScheduled',
  BatchBroadcastRequested: 'PolkadotIngressEgress.BatchBroadcastRequested',
} as const;

export const polkadotBroadcaster = {
  BroadcastSuccess: 'PolkadotBroadcaster.BroadcastSuccess',
  BroadcastAborted: 'PolkadotBroadcaster.BroadcastAborted',
} as const;

export const swapEventNames = [
  values(swapping),
  values(bitcoinIngressEgress),
  values(bitcoinBroadcaster),
  values(ethereumIngressEgress),
  values(ethereumBroadcaster),
  values(polkadotIngressEgress),
  values(polkadotBroadcaster),
].flat();

export type EventHandlerArgs = {
  prisma: Prisma.TransactionClient;
  event: Pick<Event, 'args' | 'name' | 'indexInBlock'>;
  block: Pick<Block, 'height' | 'timestamp'>;
};

const handlers = [
  {
    spec: 0,
    handlers: [
      { name: swapping.SwapScheduled, handler: swapScheduled },
      { name: swapping.SwapExecuted, handler: swapExecuted },
      { name: swapping.SwapEgressScheduled, handler: swapEgressScheduled },
      {
        name: bitcoinIngressEgress.EgressScheduled,
        handler: networkEgressScheduled,
      },
      {
        name: bitcoinIngressEgress.BatchBroadcastRequested,
        handler: networkBatchBroadcastRequested,
      },
      {
        name: bitcoinBroadcaster.BroadcastSuccess,
        handler: networkBroadcastSuccess('Bitcoin'),
      },
      {
        name: bitcoinBroadcaster.BroadcastAborted,
        handler: networkBroadcastAborted('Bitcoin'),
      },
      {
        name: ethereumIngressEgress.EgressScheduled,
        handler: networkEgressScheduled,
      },
      {
        name: ethereumIngressEgress.BatchBroadcastRequested,
        handler: networkBatchBroadcastRequested,
      },
      {
        name: ethereumBroadcaster.BroadcastSuccess,
        handler: networkBroadcastSuccess('Ethereum'),
      },
      {
        name: ethereumBroadcaster.BroadcastAborted,
        handler: networkBroadcastAborted('Ethereum'),
      },
      {
        name: polkadotIngressEgress.EgressScheduled,
        handler: networkEgressScheduled,
      },
      {
        name: polkadotIngressEgress.BatchBroadcastRequested,
        handler: networkBatchBroadcastRequested,
      },
      {
        name: polkadotBroadcaster.BroadcastSuccess,
        handler: networkBroadcastSuccess('Polkadot'),
      },
      {
        name: polkadotBroadcaster.BroadcastAborted,
        handler: networkBroadcastAborted('Polkadot'),
      },
    ],
  },
];

const eventHandlerMap = buildHandlerMap(handlers);

export const getEventHandler = getDispatcher(eventHandlerMap);
