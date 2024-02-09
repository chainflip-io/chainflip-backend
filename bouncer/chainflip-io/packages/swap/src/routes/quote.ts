import express from 'express';
import type { Server } from 'socket.io';
import { quoteQuerySchema } from '@/shared/schemas';
import getConnectionHandler from '../quoting/getConnectionHandler';
import {
  findBestQuote,
  buildQuoteRequest,
  collectQuotes,
} from '../quoting/quotes';
import logger from '../utils/logger';
import ServiceError from '../utils/ServiceError';
import { getBrokerQuote } from '../utils/statechain';
import { asyncHandler } from './common';

const quote = (io: Server) => {
  const router = express.Router();

  const { handler, quotes$ } = getConnectionHandler();

  io.on('connection', handler);

  router.get(
    '/',
    asyncHandler(async (req, res) => {
      const result = quoteQuerySchema.safeParse(req.query);

      if (!result.success) {
        logger.info('received invalid quote request', { query: req.query });
        throw ServiceError.badRequest('invalid request');
      }

      const quoteRequest = buildQuoteRequest(result.data);

      io.emit('quote_request', quoteRequest);

      try {
        const [marketMakerQuotes, brokerQuote] = await Promise.all([
          collectQuotes(quoteRequest.id, io.sockets.sockets.size, quotes$),
          getBrokerQuote(result.data, quoteRequest.id),
        ]);

        res.json(findBestQuote(marketMakerQuotes, brokerQuote));
      } catch (err) {
        const message =
          err instanceof Error
            ? err.message
            : 'unknown error (possibly no liquidity)';

        logger.error('error while collecting quotes:', err);

        res.status(500).json({ error: message });
      }
    }),
  );

  return router;
};

export default quote;
