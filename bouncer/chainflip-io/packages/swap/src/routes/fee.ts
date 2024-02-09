import express from 'express';
import { Assets } from '@/shared/enums';

const router = express.Router();

router.get('/', (req, res) => {
  const { USDC, ...rest } = Assets;
  const assets = Object.fromEntries(
    Object.values(rest).map((asset) => [asset, '0.0015']),
  );

  res.json({ assets, network: '0.001' });
});

export default router;
