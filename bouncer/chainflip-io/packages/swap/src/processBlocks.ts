import assert from 'assert';
import { GraphQLClient } from 'graphql-request';
import { performance } from 'perf_hooks';
import { setTimeout as sleep } from 'timers/promises';
import prisma from './client';
import { getEventHandler, swapEventNames } from './event-handlers';
import { GetBatchQuery } from './gql/generated/graphql';
import { GET_BATCH } from './gql/query';
import { handleExit } from './utils/function';
import logger from './utils/logger';

const { INGEST_GATEWAY_URL } = process.env;

assert(INGEST_GATEWAY_URL, 'INGEST_GATEWAY_URL is not defined');

const client = new GraphQLClient(INGEST_GATEWAY_URL);

const BATCH_SIZE = Number(process.env.PROCESSOR_BATCH_SIZE) || 50;
const TRANSACTION_TIMEOUT =
  Number(process.env.PROCESSOR_TRANSACTION_TIMEOUT) || 10000;

export type Block = NonNullable<GetBatchQuery['blocks']>['nodes'][number];
export type Event = Block['events']['nodes'][number];

const fetchBlocks = async (height: number): Promise<Block[]> => {
  const start = performance.now();
  for (let i = 0; i < 5; i += 1) {
    try {
      const batch = await client.request(GET_BATCH, {
        height,
        limit: BATCH_SIZE,
        swapEvents: swapEventNames,
      });

      const blocks = batch.blocks?.nodes;

      assert(blocks !== undefined, 'blocks is undefined');

      logger.info('blocks fetched', {
        height,
        duration: performance.now() - start,
        attempt: i,
        blocksFetched: blocks.length,
      });

      return blocks;
    } catch (error) {
      logger.error('failed to fetch batch', { error });
      if (i === 4) throw error;
    }
  }

  throw new Error('failed to fetch batch');
};

export default async function processBlocks() {
  logger.info('processing blocks');

  let run = true;

  handleExit(() => {
    logger.info('stopping processing of blocks');
    run = false;
  });

  logger.info('getting latest state');
  let { height: lastBlock } = await prisma.state.upsert({
    where: { id: 1 },
    create: { id: 1, height: -1 },
    update: {},
  });
  logger.info(`resuming processing from block ${lastBlock}`);

  let nextBatch: Promise<Block[]> | undefined;

  while (run) {
    const blocks = await (nextBatch ?? fetchBlocks(lastBlock + 1));

    const start = performance.now();

    if (blocks.length === 0) {
      nextBatch = undefined;

      await sleep(5000);

      // eslint-disable-next-line no-continue
      continue;
    }

    nextBatch =
      blocks.length === BATCH_SIZE
        ? fetchBlocks(lastBlock + blocks.length + 1)
        : undefined;

    logger.info(
      `processing blocks from ${lastBlock + 1} to ${
        lastBlock + blocks.length
      }...`,
    );

    for (const block of blocks) {
      const state = await prisma.state.findUniqueOrThrow({ where: { id: 1 } });

      assert(
        state.height === lastBlock,
        'state height is not equal to lastBlock maybe another process is running',
      );

      assert(
        lastBlock + 1 === block.height,
        'block height is not monotonically increasing',
      );

      await prisma.$transaction(
        async (txClient) => {
          for (const event of block.events.nodes) {
            const eventHandler = getEventHandler(event.name, block.specId);
            if (!eventHandler) {
              throw new Error(
                `unexpected event: "${event.name}" for specId: "${block.specId}"`,
              );
            }
            try {
              await eventHandler({ prisma: txClient, event, block });
            } catch (error) {
              logger.customError(
                `processBlock error: Error handling event ${event.name}`,
                { alertCode: 'EventHandlerError' },
                {
                  error,
                  eventName: event.name,
                  indexInBlock: event.indexInBlock,
                  blockHeight: block.height,
                  specId: block.specId,
                },
              );
              throw error;
            }
          }
          const result = await txClient.state.updateMany({
            where: { id: 1, height: block.height - 1 },
            data: { height: block.height },
          });
          assert(
            result.count === 1,
            'failed to update state, maybe another process is running',
          );
        },
        { timeout: TRANSACTION_TIMEOUT },
      );
      lastBlock = block.height;
    }

    const end = performance.now();
    logger.info(
      `processed ${blocks.length} blocks in ${
        end - start
      } milliseconds, last block: ${lastBlock}`,
    );
  }
}
