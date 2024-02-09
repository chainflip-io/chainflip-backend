import express from 'express';
import prisma from '../client';
import logger from '../utils/logger';
import { thirdPartySwapSchema } from '../utils/parsers';
import ServiceError from '../utils/ServiceError';
import { asyncHandler } from './common';

const router = express.Router();

router.post(
  '/',
  asyncHandler(async (req, res) => {
    const result = thirdPartySwapSchema.safeParse(req.body);

    if (!result.success) {
      logger.info('received bad request for new third party swap', {
        body: req.body,
      });
      throw ServiceError.badRequest('invalid request body');
    }
    try {
      await prisma.thirdPartySwap.create({
        data: {
          uuid: result.data.uuid,
          protocol: result.data.routeResponse.integration,
          txHash: result.data.txHash,
          txLink: result.data.txLink,
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          routeResponse: result.data.routeResponse as Record<string, any>,
        },
      });
      res.sendStatus(201);
    } catch (err) {
      if (err instanceof Error) throw ServiceError.internalError(err.message);
      throw ServiceError.internalError();
    }
  }),
);

router.get(
  '/:uuid',
  asyncHandler(async (req, res) => {
    const { uuid } = req.params;

    try {
      const thirdPartySwap = await prisma.thirdPartySwap.findFirst({
        where: {
          uuid,
        },
      });
      if (!thirdPartySwap) throw ServiceError.notFound();

      const { id, ...swap } = thirdPartySwap;
      res.json({ ...swap });
    } catch (err) {
      if (err instanceof ServiceError) throw err;
      if (err instanceof Error) throw ServiceError.internalError(err.message);
      throw ServiceError.internalError();
    }
  }),
);

export default router;
