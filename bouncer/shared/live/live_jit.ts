import * as ss58 from '@chainflip/utils/ss58';
import { bytesToHex } from '@chainflip/utils/bytes';
import type { FrameSystemEventRecord } from 'generated/chaintypes/chainflip-node/types';
import {
  Asset,
  assetDecimals,
  assetPriceToInternalAssetPrice,
  chainFromAsset,
  chainGasAsset,
  encodedAddress,
  fineAmountToAmount,
  getFreeBalance,
  stateChainAssetFromAsset,
  amountToFineAmountBigInt,
} from 'shared/utils';
import { getChainflipApi } from 'shared/utils/substrate';
import { ChainflipIO, WithLpAccount } from 'shared/utils/chainflip_io';
import { depositLiquidity } from 'shared/deposit_liquidity';
import { accountRolesAccountRoleRegisteredEvent } from 'generated/events/accountRoles/accountRoleRegistered';
import { liquidityProviderWithdrawalEgressScheduledEvent } from 'generated/events/liquidityProvider/withdrawalEgressScheduled';
import { priceX128ToTick, sqrtPriceX96ToPriceX128, winningTick } from 'shared/live/tick_math';
import { isLiveNetwork, maxAllowedSwapAmount, minimumDepositAmount } from 'shared/live/live_config';
import { ourExternalWallet } from 'shared/live/live_swap';
import { JitFillReport } from 'shared/live/report';

// Just-in-time LP for a single swap.
// Given the swap details, it tries to fill the swap with a winning order on the execution block and close it the next block.

export type OrderSide = 'Buy' | 'Sell';

export type swapLeg = { baseAsset: Asset; side: OrderSide };

// How far ahead of the current block cf-pools lets a limit order be scheduled
const SCHEDULE_OPEN_LIMIT_BLOCKS = 2;

// Maps a base asset to its oracle price feed (a Usd-denominated PriceAsset). Assets absent here
// have no oracle, so their orders are priced off the pool instead. Usdc is the quote, never a base.
const ORACLE_PRICE_ASSET: Partial<Record<Asset, string>> = {
  Eth: 'Eth',
  ArbEth: 'Eth',
  Btc: 'Btc',
  Wbtc: 'Btc',
  Sol: 'Sol',
  Usdt: 'Usdt',
  ArbUsdt: 'Usdt',
  SolUsdt: 'Usdt',
  HubUsdt: 'Usdt',
};

/**
 * Checks that the account is a registered LP, optionally registering it. Never funds the
 * account: on a live network it must already hold enough FLIP to register.
 */
export async function ensureLpRole(cf: ChainflipIO<WithLpAccount>, registerIfNeeded: boolean) {
  const address = cf.requirements.account.keypair.address;
  await using client = await getChainflipApi();
  const role = (await client.query.accountRoles.accountRoles(address)) ?? 'Unregistered';

  if (role === 'LiquidityProvider') {
    return;
  }
  if (role !== 'Unregistered') {
    throw new Error(`Account ${address} has role '${role}', expected 'LiquidityProvider'`);
  }
  if (!registerIfNeeded) {
    throw new Error(
      `Account ${address} is not registered as an LP. ` +
        `Fund it with FLIP and re-run with --register-lp.`,
    );
  }
  cf.info(`Registering ${address} as a liquidity provider`);
  await cf.submitExtrinsic({
    extrinsic: (api) => api.tx.liquidityProvider.registerLpAccount(),
    expectedEvent: accountRolesAccountRoleRegisteredEvent.refine(
      (event) => event.accountId === address,
    ),
  });
}

/** Current pool sqrt price (Q64.96) for the base asset's USDC pool. */
export async function getPoolSqrtPrice(baseAsset: Asset): Promise<bigint> {
  await using client = await getChainflipApi();
  const pool = await client.query.liquidityPools.pools({
    assets: { base: baseAsset, quote: 'Usdc' },
  });
  const sqrtPrice = pool?.poolState.rangeOrders.currentSqrtPrice;
  if (sqrtPrice === undefined) {
    throw new Error(`No ${baseAsset}/Usdc pool found`);
  }
  return BigInt(sqrtPrice);
}

/**
 * A `cf_subscribe_scheduled_swaps` update: every pending swap in one pool, pushed once per block.
 * Fields are the RPC's snake_case, with `swaps` flattened in from `BlockUpdate.data`. Amounts are
 * U256 hex strings; for a Sell leg `amount` is the input in the base asset, for a Buy leg it is the
 * pallet's oracle-based estimate of the stable amount arriving from the first hop, net of fees.
 */
type ScheduledSwapsUpdate = {
  block_number: number;
  swaps: {
    swap_id: number | string;
    swap_request_id: number | string;
    /** The runtime serializes `Side` snake_case, so this is lower-case, unlike our `OrderSide`. */
    side: 'buy' | 'sell';
    amount: string;
    execute_at: number;
  }[];
};

export type LimitOrderSpec = {
  baseAsset: Asset;
  side: OrderSide;
  id: bigint;
  tick: number;
  /** Fine units of the asset being sold (USDC for Buy orders, the base asset for Sell). */
  sellAmount: bigint;
};

/**
 * Schedules a one-shot JIT limit order: it materializes on-chain at `dispatchAt` (undefined =
 * immediately) and auto-closes at `closeOrderAt`, so it exists only for the block(s) of the
 * swap it is meant to fill. The pallet only allows scheduling up to 2 blocks ahead
 * (SCHEDULE_OPEN_LIMIT_BLOCKS), which exactly fits SWAP_DELAY_BLOCKS.
 */
export async function scheduleJitOrder(
  cf: ChainflipIO<WithLpAccount>,
  order: LimitOrderSpec,
  dispatchAt: number | undefined,
  closeOrderAt: number,
): Promise<void> {
  await cf.submitExtrinsic({
    extrinsic: (api) =>
      api.tx.liquidityPools.setLimitOrder(
        order.baseAsset,
        'Usdc',
        order.side,
        order.id,
        order.tick,
        order.sellAmount,
        dispatchAt,
        closeOrderAt,
      ),
  });
  cf.info(
    `JIT order ${order.id} (${order.side} ${order.baseAsset}) at tick ${order.tick}: ` +
      `${dispatchAt !== undefined ? `dispatches at block ${dispatchAt}` : 'dispatched now'}, closes at ${closeOrderAt}`,
  );
}

/**
 * Withdraws exactly `amount` (fine units, capped at the free balance) of the LP's balance to
 * the given external address.
 */
export async function withdrawLpFunds(
  cf: ChainflipIO<WithLpAccount>,
  asset: Asset,
  destAddress: string,
  amount: bigint,
): Promise<{ egressId: string; amount: string } | undefined> {
  const address = cf.requirements.account.keypair.address;
  const freeBalance = await getFreeBalance(address, asset);
  const toWithdraw = amount < freeBalance ? amount : freeBalance;
  if (toWithdraw === 0n) {
    return undefined;
  }
  const egress = await cf.submitExtrinsic({
    extrinsic: (api) =>
      api.tx.liquidityProvider.withdrawAsset(
        toWithdraw,
        asset,
        encodedAddress(chainFromAsset(asset), destAddress),
      ),
    expectedEvent: liquidityProviderWithdrawalEgressScheduledEvent.refine(
      (event) => event.asset === asset,
    ),
  });
  cf.info(
    `Withdrawal of ${fineAmountToAmount(toWithdraw.toString(), assetDecimals(asset))} ${asset} scheduled (egress ${egress.egressId})`,
  );
  return { egressId: egress.egressId.join('-'), amount: toWithdraw.toString() };
}

/** The orders a pair needs; all pools are quoted in USDC. */
export function getSwapLegs(sourceAsset: Asset, destAsset: Asset): swapLeg[] {
  const legs: swapLeg[] = [];
  if (sourceAsset !== 'Usdc') {
    // Absorbs the sold deposit by buying the source asset with USDC.
    legs.push({ baseAsset: sourceAsset, side: 'Buy' });
  }
  if (destAsset !== 'Usdc') {
    // Provides the output by selling the destination asset for the USDC intermediate.
    legs.push({ baseAsset: destAsset, side: 'Sell' });
  }
  if (legs.length === 0) {
    throw new Error('Nothing for the LP to fill: both assets are Usdc');
  }
  return legs;
}

/**
 * Compares accounts by underlying bytes: ss58 strings of the same key differ by network
 * prefix, and dedot event fields are AccountId32 objects (with a `raw` hex field).
 */
type AccountLike = string | { raw: string };
function sameAccount(a: AccountLike, b: AccountLike): boolean {
  const hex = (value: AccountLike) =>
    typeof value === 'string' ? bytesToHex(ss58.decode(value).data) : value.raw.toLowerCase();
  return hex(a) === hex(b);
}

// Extra liquidity provided over the swap value, to absorb price movement and fees.
const LP_FUNDING_HEADROOM = 1.25;

/**
 * The oracle base/Usdc price (X128) for `asset`, or undefined when the asset has no feed, its price
 * is stale, or the oracle runtime API is unavailable — callers then fall back to the pool price.
 * Pricing orders and funding at the oracle keeps swaps inside the oracle price protection (LPP).
 */
async function getOraclePrice<A>(cf: ChainflipIO<A>, asset: Asset): Promise<bigint | undefined> {
  const priceAsset = ORACLE_PRICE_ASSET[asset];
  if (priceAsset === undefined) {
    return undefined;
  }
  await using client = await getChainflipApi();
  let prices;
  try {
    prices = await client.call.customRuntimeApi.cfOraclePrices(undefined);
  } catch (error) {
    cf.warn(`Oracle prices unavailable for ${asset}, ${error}`);
    return undefined;
  }
  const usdOf = (priceName: string) =>
    prices.find((p) => String(p.baseAsset) === priceName && p.priceStatus === 'UpToDate')?.price;
  const baseUsd = usdOf(priceAsset);
  const usdcUsd = usdOf('Usdc');
  if (baseUsd === undefined || usdcUsd === undefined) {
    return undefined;
  }
  // asset/Usdc (X128) = (asset/Usd) / (Usdc/Usd).
  return (BigInt(baseUsd) * 2n ** 128n) / BigInt(usdcUsd);
}

/**
 * The price and tick an order in `asset`'s pool should sit at to win the fill on `side`: whichever
 * of the oracle and the pool is more attractive to the swapper — bidding higher when we buy the
 * base, asking lower when we sell it. The oracle alone keeps a swap inside the oracle price
 * protection (LPP) when the pool is worse, but loses outright when the pool is already better (in
 * which case the swap doesn't need that protection anyway). Beating the pool costs one tick, which
 * is immaterial to sizing.
 */
export async function competitivePricing<A>(
  cf: ChainflipIO<A>,
  asset: Asset,
  side: OrderSide,
): Promise<{ priceX128: bigint; tick: number }> {
  const sqrtPrice = await getPoolSqrtPrice(asset);
  const poolPriceX128 = sqrtPriceX96ToPriceX128(sqrtPrice);
  const oraclePriceX128 = await getOraclePrice(cf, asset);
  const oracleWins =
    oraclePriceX128 !== undefined &&
    (side === 'Buy' ? oraclePriceX128 > poolPriceX128 : oraclePriceX128 < poolPriceX128);

  return oracleWins
    ? { priceX128: oraclePriceX128, tick: priceX128ToTick(oraclePriceX128) }
    : { priceX128: poolPriceX128, tick: winningTick(sqrtPrice, side) };
}

/**
 * Ensures the LP has enough free balance to fill a swap of `amount` (estimated via the quoted
 * rate, with headroom), automatically depositing any shortfall from our external wallet.
 * Returns the amounts deposited (fine units per asset), which the run withdraws back
 * afterwards (net of what its orders sold).
 */
export async function ensureLpFunding(
  cf: ChainflipIO<WithLpAccount>,
  sourceAsset: Asset,
  destAsset: Asset,
  amount: string,
): Promise<Map<Asset, bigint>> {
  const lpAddress = cf.requirements.account.keypair.address;
  const deposited = new Map<Asset, bigint>();

  // The LP must hold the sell asset of every leg of both our swap and the gas swap
  const gasAsset = chainGasAsset(chainFromAsset(destAsset));
  const swaps: [Asset, Asset][] = [[sourceAsset, destAsset]];
  if (sourceAsset !== gasAsset) {
    swaps.push([sourceAsset, gasAsset]);
  }
  const sellAssets = new Set<Asset>();
  for (const [from, to] of swaps) {
    for (const leg of getSwapLegs(from, to)) {
      sellAssets.add(leg.side === 'Buy' ? 'Usdc' : leg.baseAsset);
    }
  }

  const legPriceX128 = async (asset: Asset, side: OrderSide): Promise<bigint> =>
    asset === 'Usdc' ? 2n ** 128n : (await competitivePricing(cf, asset, side)).priceX128;
  const withHeadroom = (fine: bigint) =>
    (fine * BigInt(Math.round(LP_FUNDING_HEADROOM * 100))) / 100n;

  for (const sellAsset of sellAssets) {
    // Fine amount of `sellAsset` our order sells to fill `swapAmountFine` of the source: the swap's
    // Usdc value (at the source's quoted price) divided by the sell asset's quoted price.
    const legSellAmountFine = async (swapAmountFine: bigint): Promise<bigint> => {
      const usdcFine =
        sourceAsset === 'Usdc'
          ? swapAmountFine
          : (swapAmountFine * (await legPriceX128(sourceAsset, 'Buy'))) / 2n ** 128n;
      return sellAsset === 'Usdc'
        ? usdcFine
        : (usdcFine * 2n ** 128n) / (await legPriceX128(sellAsset, 'Sell'));
    };

    const needed = withHeadroom(
      await legSellAmountFine(amountToFineAmountBigInt(amount, sourceAsset)),
    );
    const free = await getFreeBalance(lpAddress, sellAsset);
    if (free < needed) {
      // A deposit below the protocol minimum is ignored on-chain (AccountCredited never fires and
      // the fill hangs), so top up to at least the minimum when the shortfall is smaller.
      const minDeposit = isLiveNetwork() ? await minimumDepositAmount(sellAsset) : 0n;
      const depositFine = needed - free < minDeposit ? minDeposit : needed - free;
      if (isLiveNetwork()) {
        const legCap = withHeadroom(
          await legSellAmountFine(
            amountToFineAmountBigInt(String(maxAllowedSwapAmount(sourceAsset)), sourceAsset),
          ),
        );
        if (depositFine > legCap) {
          throw new Error(
            `LP funding of ${fineAmountToAmount(depositFine.toString(), assetDecimals(sellAsset))} ` +
              `${sellAsset} exceeds the ${fineAmountToAmount(legCap.toString(), assetDecimals(sellAsset))} ` +
              `bound for a max ${maxAllowedSwapAmount(sourceAsset)} ${sourceAsset} swap; refusing on a live network`,
          );
        }
      }
      const shortfall = fineAmountToAmount(depositFine.toString(), assetDecimals(sellAsset));
      cf.info(`Depositing ${shortfall} ${sellAsset} of liquidity for ${lpAddress}`);
      await depositLiquidity(
        cf,
        sellAsset,
        Number(shortfall),
        isLiveNetwork() ? ourExternalWallet(sellAsset) : undefined,
      );
      deposited.set(sellAsset, depositFine);
    }
  }
  return deposited;
}

/**
 * Fills one swap with our own just-in-time liquidity. Arm this BEFORE sending the deposit and
 * await it alongside the swap tracking; it returns once the swap request completes (plus a
 * couple of blocks to observe the auto-close fills), or throws on timeout.
 */
export async function fillSwapJit(
  cf: ChainflipIO<WithLpAccount>,
  args: {
    channelId: number;
    sourceAsset: Asset;
    destAsset: Asset;
    /** Fix the order price (USDC per base) instead of one tick better than the pool. */
    fixedPrice?: number;
    timeoutSeconds: number;
  },
): Promise<JitFillReport> {
  const lpAddress = cf.requirements.account.keypair.address;
  const legs = getSwapLegs(args.sourceAsset, args.destAsset);
  if (args.fixedPrice !== undefined && legs.length > 1) {
    throw new Error('fixedPrice is only supported for pairs that need a single order');
  }
  // A manual price override (--lpPrice) sets both the tick and the size, so the order stays
  // internally consistent instead of being sized off the live price it is deliberately ignoring.
  const fixedPriceX128 =
    args.fixedPrice !== undefined
      ? BigInt(assetPriceToInternalAssetPrice(legs[0].baseAsset, 'Usdc', args.fixedPrice))
      : undefined;
  const fixedTick = fixedPriceX128 !== undefined ? priceX128ToTick(fixedPriceX128) : undefined;

  await using client = await getChainflipApi();

  const report: JitFillReport = { orders: [], fills: [] };
  const placedOrderIds = new Set<string>();

  // Pools we provide liquidity in: our swap's hops, plus the second hop of the ingress/egress-fee
  // ("gas") swap the deposit is split into, which lands in the destination chain's gas asset.
  const gasAsset = chainGasAsset(chainFromAsset(args.destAsset));
  const poolAssets = [...new Set([args.sourceAsset, args.destAsset, gasAsset])].filter(
    (asset) => asset !== 'Usdc',
  );
  const fixedBase = args.fixedPrice !== undefined ? legs[0].baseAsset : undefined;

  /**
   * Places our one-shot orders in `baseAsset`'s pool for the swaps executing at `executeAt`:
   * `sellBase` is how much of the base asset is being sold into the pool (so we buy it, offering
   * USDC) and `buyUsdc` how much USDC is buying it out (so we sell it, offering the base asset).
   */
  const placePoolOrders = async (
    baseAsset: Asset,
    executeAt: number,
    sellBase: bigint,
    buyUsdc: bigint,
  ) => {
    const height = await client.query.system.number();
    if (height > executeAt) {
      cf.warn(`Missed swaps executing at ${executeAt} (now ${height}), relying on FoK retry`);
      return;
    }
    const dispatchAt = executeAt > height ? executeAt : undefined;
    const closeOrderAt = executeAt + 1;

    // Tick and size come from the same price, so an order absorbs what it is meant to at the price
    // it actually sits at. `--lpPrice` overrides both, so the override stays self-consistent
    // instead of being sized off the live price it deliberately ignores.
    const pricingFor = async (side: OrderSide) =>
      fixedPriceX128 !== undefined && baseAsset === fixedBase
        ? { priceX128: fixedPriceX128, tick: fixedTick! }
        : competitivePricing(cf, baseAsset, side);
    // 10% headroom for price movement and fees.
    const withHeadroom = (amount: bigint) => (amount * 110n) / 100n;

    const specs: LimitOrderSpec[] = [];
    if (sellBase > 0n) {
      const { priceX128, tick } = await pricingFor('Buy');
      specs.push({
        baseAsset,
        side: 'Buy',
        id: BigInt(executeAt),
        tick,
        sellAmount: withHeadroom((sellBase * priceX128) / 2n ** 128n),
      });
    }
    if (buyUsdc > 0n) {
      const { priceX128, tick } = await pricingFor('Sell');
      specs.push({
        baseAsset,
        side: 'Sell',
        id: BigInt(executeAt),
        tick,
        sellAmount: withHeadroom((buyUsdc * 2n ** 128n) / priceX128),
      });
    }

    await cf.all(
      specs
        .filter((order) => order.sellAmount > 0n)
        .map((order) => async (cfTask: ChainflipIO<WithLpAccount>) => {
          await scheduleJitOrder(cfTask, order, dispatchAt, closeOrderAt);
          placedOrderIds.add(order.id.toString());
          report.orders.push({
            id: order.id.toString(),
            baseAsset: order.baseAsset,
            side: order.side,
            tick: order.tick,
            sellAmount: order.sellAmount.toString(),
            dispatchAt,
            closeOrderAt,
          });
        }),
    );
  };

  let swapRequestId: bigint | undefined;
  // The swap requests our deposit produced: our own swap, plus the ingress/egress fee ("gas") swap
  // the deposit is split into. Only these are ours to fill.
  const ourRequestIds = new Set<bigint>();
  // Chunks we've already filled, keyed `swapId@executeAt`: the queue re-offers a pending chunk
  // every block, and a FoK retry re-offers it under a later executeAt (which needs its own order).
  const filledChunks = new Set<string>();
  let completedAtBlock: number | undefined;

  /** One pass over a block's events, for the things the swap queue doesn't carry. */
  const scanEvents = (height: number, records: FrameSystemEventRecord[]): void => {
    // The fee swap carries no reference back to the deposit that produced it, so it is recognised
    // by being the source→gasAsset fee swap requested in the very block our own swap was.
    const feeSwapsThisBlock: bigint[] = [];
    let foundOurRequestHere = false;

    for (const record of records) {
      const { event } = record;
      if (client.events.swapping.SwapRequested.is(event)) {
        const data = event.palletEvent.data;
        if (
          data.origin.type === 'DepositChannel' &&
          Number(data.origin.value.channelId) === args.channelId &&
          data.inputAsset === args.sourceAsset &&
          data.outputAsset === args.destAsset
        ) {
          swapRequestId = BigInt(data.swapRequestId);
          ourRequestIds.add(swapRequestId);
          foundOurRequestHere = true;
          cf.info(`Filling swap request ${swapRequestId} from our channel ${args.channelId}`);
        } else if (
          data.requestType.type === 'IngressEgressFee' &&
          data.inputAsset === args.sourceAsset &&
          data.outputAsset === gasAsset
        ) {
          feeSwapsThisBlock.push(BigInt(data.swapRequestId));
        }
      } else if (client.events.liquidityPools.LimitOrderUpdated.is(event)) {
        const data = event.palletEvent.data;
        // Record the auto-close of our orders (skipping the creation event, which is an
        // Increase): what they bought, and how much of the committed amount came back unsold.
        if (
          sameAccount(data.lp, lpAddress) &&
          placedOrderIds.has(data.id.toString()) &&
          data.sellAmountChange?.type !== 'Increase'
        ) {
          const unsoldReturned =
            data.sellAmountChange?.type === 'Decrease' ? data.sellAmountChange.value : 0n;
          report.fills.push({
            orderId: data.id.toString(),
            // The runtime asset enum is wider than the bouncer's (e.g. includes Dot), but
            // this is our own order, so it's always one of ours.
            baseAsset: data.baseAsset as Asset,
            side: data.side,
            boughtAmount: data.boughtAmount.toString(),
            collectedFees: data.collectedFees.toString(),
            unsoldReturned: unsoldReturned.toString(),
          });
          cf.info(
            `Order ${data.id} (${data.side} ${data.baseAsset}) closed: bought ${data.boughtAmount}, fees ${data.collectedFees}, unsold ${unsoldReturned}`,
          );
        }
      } else if (client.events.swapping.SwapRequestCompleted.is(event)) {
        const data = event.palletEvent.data;
        if (swapRequestId !== undefined && BigInt(data.swapRequestId) === swapRequestId) {
          completedAtBlock = height;
        }
      }
    }

    if (foundOurRequestHere) {
      for (const feeSwapId of feeSwapsThisBlock) {
        ourRequestIds.add(feeSwapId);
        cf.info(`Also filling fee swap request ${feeSwapId} our deposit was split into`);
      }
    }
  };

  const deadline = Date.now() + args.timeoutSeconds * 1000;

  // Placement is serialized onto this chain: every pool pushes an update each block, placing an
  // order spans blocks, and ChainflipIO rejects concurrent use of a single instance.
  let placing: Promise<void> = Promise.resolve();
  const latestByPool = new Map<Asset, ScheduledSwapsUpdate>();

  const fillDuePools = async () => {
    for (const [baseAsset, update] of latestByPool) {
      // Only what our own deposit was split into: our swap and its ingress/egress fee swap. Anything
      // else scheduled at the same block belongs to somebody else, and execute_group_of_swaps would
      // fill the group pro-rata — but covering that is subsidising their swap, so we leave it. If it
      // dilutes our price past the fill-or-kill limit our swap refunds, which the run reports.
      const ours = update.swaps.filter((swap) => ourRequestIds.has(BigInt(swap.swap_request_id)));
      const due = [...new Set(ours.map((swap) => swap.execute_at))].filter(
        (executeAt) =>
          !filledChunks.has(`${baseAsset}@${executeAt}`) &&
          executeAt - update.block_number <= SCHEDULE_OPEN_LIMIT_BLOCKS,
      );
      for (const executeAt of due) {
        const bundle = ours.filter((swap) => swap.execute_at === executeAt);
        const total = (side: 'buy' | 'sell') =>
          bundle
            .filter((swap) => swap.side === side)
            .reduce((sum, swap) => sum + BigInt(swap.amount), 0n);
        filledChunks.add(`${baseAsset}@${executeAt}`);
        cf.info(
          `Filling ${bundle.length} of our swap(s) in the ${baseAsset}/Usdc pool executing at block ${executeAt}`,
        );
        try {
          await placePoolOrders(baseAsset, executeAt, total('sell'), total('buy'));
        } catch (error) {
          // Best effort: a rejected order shouldn't abort the whole fill; the swap then
          // FoK-retries or refunds, which the tracking side reports as the outcome.
          cf.warn(`Could not place JIT orders in the ${baseAsset}/Usdc pool: ${error}`);
        }
      }
    }
  };

  const enqueueFill = (): void => {
    placing = placing
      .then(() => fillDuePools())
      .catch((error) => cf.warn(`JIT fill pass failed: ${error}`));
  };

  // One stream per pool. The pallet re-offers a pending swap on every block (a FoK retry under a
  // later executeAt), so acting on each update as it arrives fills a swap the moment it enters the
  // scheduling window.
  const unsubscribes = await Promise.all(
    poolAssets.map(
      (baseAsset) =>
        client.rpc.cf_subscribe_scheduled_swaps(
          stateChainAssetFromAsset(baseAsset),
          stateChainAssetFromAsset('Usdc'),
          (update: ScheduledSwapsUpdate) => {
            latestByPool.set(baseAsset, update);
            enqueueFill();
          },
        ) as Promise<() => Promise<void>>,
    ),
  );

  // Best-block subscription
  let blockArrived = false;
  let wake: (() => void) | undefined;
  const unsubscribe = await client.query.system.number(() => {
    if (wake) {
      const resume = wake;
      wake = undefined;
      resume();
    } else {
      blockArrived = true;
    }
  });
  // Resolves on the next imported block, or after `fallbackMs` so the deadline is still enforced if
  // the subscription ever stalls.
  const waitForNextBlock = (fallbackMs: number) =>
    new Promise<void>((resolve) => {
      if (blockArrived) {
        blockArrived = false;
        resolve();
        return;
      }
      const timer = setTimeout(() => {
        wake = undefined;
        resolve();
      }, fallbackMs);
      wake = () => {
        clearTimeout(timer);
        resolve();
      };
    });

  try {
    // Awaiting an order's inclusion can span blocks, and skipping one could miss our SwapRequested
    // or an order's fill, so scan every block up to the head each time we wake.
    let nextBlock = await client.query.system.number();
    for (;;) {
      if (Date.now() > deadline) {
        throw new Error(`fillSwapJit timed out after ${args.timeoutSeconds}s`);
      }
      const head = await client.query.system.number();
      while (nextBlock <= head) {
        const hash = await client.rpc.chain_getBlockHash(nextBlock);
        const at = await client.at(hash!);
        scanEvents(nextBlock, await at.query.system.events());
        nextBlock += 1;
        // The orders auto-close (and report their fills) one block after execution.
        if (completedAtBlock !== undefined && nextBlock > completedAtBlock + 2) {
          // A leg our own orders didn't buy into was filled by somebody else's liquidity, so the
          // JIT fill lost the race even if the swap itself went through.
          report.unfilledLegs = legs.filter(
            (leg) =>
              !report.fills.some(
                (fill) =>
                  fill.baseAsset === leg.baseAsset &&
                  fill.side === leg.side &&
                  BigInt(fill.boughtAmount) > 0n,
              ),
          );
          for (const leg of report.unfilledLegs) {
            cf.warn(
              `Our ${leg.side} ${leg.baseAsset} order did not fill this swap; it went to other liquidity`,
            );
          }
          return report;
        }
      }

      // Placement is driven by the subscription above; this only covers the startup race where an
      // update arrives before the SwapRequested identifying our request has been scanned.
      enqueueFill();
      await placing;
      await waitForNextBlock(6000);
    }
  } finally {
    await Promise.all(unsubscribes.map((stop) => stop().catch(() => undefined)));
    await unsubscribe().catch(() => undefined);
  }
}
