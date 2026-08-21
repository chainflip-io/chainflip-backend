import { Asset, observeBalanceIncrease } from 'shared/utils';
import { getBalance } from 'shared/get_balance';
import { send } from 'shared/send';
import { getChainflipApi } from 'shared/utils/substrate';
import { Logger } from 'shared/utils/logger';
import {
  ChainflipIO,
  WithLpAccount,
  fullAccountFromUri,
  newChainflipIO,
} from 'shared/utils/chainflip_io';
import { DcaParams } from 'shared/new_swap';
import {
  assertAboveMinimumDeposit,
  assertAmountAllowed,
  assertExpectedNetwork,
  getBouncerNetwork,
  networkTimeouts,
  requiredEnvForAsset,
  requireLiveEnv,
} from 'shared/live/live_config';
import {
  computeFokMinPrice,
  ensureBrokerRole,
  ourExternalWallet,
  requestLiveDepositChannel,
  trackSwapToCompletion,
} from 'shared/live/live_swap';
import { ensureLpFunding, ensureLpRole, fillSwapJit, withdrawLpFunds } from 'shared/live/live_jit';
import { collectSwapEvents, JitFillReport, LiveSwapReport } from 'shared/live/report';

// The full live-swap flow (PRO-2959), reusable from the submit_live_swap command and from
// live tests: submit a swap from our external wallet, fill it just-in-time with our own LP,
// track everything and return a LiveSwapReport. See commands/live/submit_live_swap.ts for
// the CLI wrapper and required environment.

export type LiveSwapArgs = {
  sourceAsset: Asset;
  destAsset: Asset;
  /** Human units of the source asset. */
  amount: string;
  /** Default: our own external wallet on the destination chain. */
  destAddress?: string;
  /** Fill-or-kill tolerance below the quoted rate, in bps. */
  toleranceBps?: number;
  /** Fill-or-kill retry duration in state-chain blocks before refunding. */
  refundDurationBlocks?: number;
  dcaParams?: DcaParams;
  /** Register the broker account (BROKER_URI) if it has no role yet. */
  registerBroker?: boolean;
  /** Don't fill the swap with our own LP liquidity. */
  skipLpFill?: boolean;
  /** Fixed LP order price (USDC per base asset) instead of one tick better than the pool. */
  lpPrice?: number;
  /** Register the LP account (LP_URI) if it has no role yet. */
  registerLp?: boolean;
};

async function currentBlockHeight(): Promise<number> {
  await using client = await getChainflipApi();
  return client.query.system.number();
}

/**
 * The LP funds this run is entitled to reclaim to our wallet: the proceeds our own deposit paid
 * for, plus whatever of the deposit was never sold. When an order sold more than we funded, the
 * surplus came from the LP's shared pre-existing balance, so we claim only the funded proportion
 * of those proceeds — taking the rest would quietly drain a shared account. With no fills at all
 * this returns the whole deposit. Pure: derived only from the fill report and what we deposited.
 */
export function lpEntitlements(
  lpFill: JitFillReport | undefined,
  lpDeposited: Map<Asset, bigint>,
): Map<Asset, bigint> {
  const entitlements = new Map<Asset, bigint>();
  const add = (asset: Asset, delta: bigint) =>
    entitlements.set(asset, (entitlements.get(asset) ?? 0n) + delta);

  // An order's auto-close can emit several LimitOrderUpdated events (bought funds and unsold
  // amount separately), so aggregate per order, grouped by the asset each order sold.
  const bySoldAsset = new Map<Asset, { sold: bigint; bought: Map<Asset, bigint> }>();
  for (const order of lpFill?.orders ?? []) {
    const closes = (lpFill?.fills ?? []).filter(
      (fill) => fill.orderId === order.id && fill.baseAsset === order.baseAsset,
    );
    if (closes.length > 0) {
      const bought = closes.reduce((sum, fill) => sum + BigInt(fill.boughtAmount), 0n);
      const unsold = closes.reduce((sum, fill) => sum + BigInt(fill.unsoldReturned), 0n);
      // A Buy order buys the base asset by selling USDC, a Sell order the other way around.
      const [boughtAsset, soldAsset]: [Asset, Asset] =
        order.side === 'Buy' ? [order.baseAsset, 'Usdc'] : ['Usdc', order.baseAsset];
      const entry = bySoldAsset.get(soldAsset) ?? { sold: 0n, bought: new Map<Asset, bigint>() };
      entry.sold += BigInt(order.sellAmount) - unsold;
      entry.bought.set(boughtAsset, (entry.bought.get(boughtAsset) ?? 0n) + bought);
      bySoldAsset.set(soldAsset, entry);
    }
  }

  for (const [soldAsset, entry] of bySoldAsset) {
    const funded = lpDeposited.get(soldAsset) ?? 0n;
    const ourSold = funded < entry.sold ? funded : entry.sold;
    if (entry.sold > 0n && ourSold > 0n) {
      for (const [boughtAsset, bought] of entry.bought) {
        add(boughtAsset, (bought * ourSold) / entry.sold);
      }
    }
  }
  for (const [asset, deposited] of lpDeposited) {
    const residual = deposited - (bySoldAsset.get(asset)?.sold ?? 0n);
    if (residual > 0n) {
      add(asset, residual);
    }
  }
  return entitlements;
}

/**
 * Withdraws this run's LP entitlement back to our external wallet, which also exercises a real
 * egress. Best-effort per asset: a below-dust-limit rejection just leaves that residue on the LP
 * account. Returns the successful withdrawals for the report.
 */
async function returnLpFunds(
  cfLp: ChainflipIO<WithLpAccount>,
  lpFill: JitFillReport | undefined,
  lpDeposited: Map<Asset, bigint>,
  logger: Logger,
): Promise<NonNullable<JitFillReport['withdrawals']>> {
  const withdrawals: NonNullable<JitFillReport['withdrawals']> = [];
  for (const [asset, entitlement] of lpEntitlements(lpFill, lpDeposited)) {
    if (entitlement > 0n) {
      try {
        const withdrawal = await withdrawLpFunds(
          cfLp,
          asset,
          ourExternalWallet(asset),
          entitlement,
        );
        if (withdrawal) {
          withdrawals.push({ asset, ...withdrawal });
        }
      } catch (error) {
        logger.warn(`Could not withdraw ${entitlement} ${asset} back to our wallet: ${error}`);
      }
    }
  }
  return withdrawals;
}

/**
 * Runs one live swap end to end and returns the report. Note the report is returned even for
 * the 'refunded' and 'incomplete' outcomes - the caller decides what counts as a failure.
 */
export async function performLiveSwap(logger: Logger, args: LiveSwapArgs): Promise<LiveSwapReport> {
  const {
    sourceAsset,
    destAsset,
    amount,
    toleranceBps = Number(process.env.LIVE_FOK_TOLERANCE_BPS ?? 150),
    refundDurationBlocks = 50,
    dcaParams,
  } = args;
  const network = getBouncerNetwork();
  const timeouts = networkTimeouts();
  const startedAt = new Date();

  // ---- safety rails ----
  requireLiveEnv(logger, [
    'BROKER_URI',
    'ETH_USDC_WHALE',
    ...(args.skipLpFill ? [] : ['LP_URI', ...requiredEnvForAsset('Usdc')]),
    ...requiredEnvForAsset(sourceAsset),
    ...requiredEnvForAsset(destAsset),
  ]);
  const genesisHash = await assertExpectedNetwork(logger);
  assertAmountAllowed(sourceAsset, Number(amount));
  await assertAboveMinimumDeposit(logger, sourceAsset, Number(amount));

  const brokerUri = (process.env.BROKER_URI ?? '//BROKER_1') as `//${string}`;
  const cf = await newChainflipIO(logger, { account: fullAccountFromUri(brokerUri, 'Broker') });
  await ensureBrokerRole(cf, args.registerBroker ?? false);

  // ---- our own LP, which will fill the swap just-in-time ----
  // Explicitly typed for the optional (skipLpFill) case, which `settleLp` narrows with `!cfLp`.
  let cfLp: ChainflipIO<WithLpAccount> | undefined;
  let lpDeposited = new Map<Asset, bigint>();
  if (!args.skipLpFill) {
    const lpUri = (process.env.LP_URI ?? '//LP_1') as `//${string}`;
    cfLp = await newChainflipIO(logger.child({ tag: 'lp' }), {
      account: fullAccountFromUri(lpUri, 'LP'),
    });
    await ensureLpRole(cfLp, args.registerLp ?? false);
    lpDeposited = await ensureLpFunding(cfLp, sourceAsset, destAsset, amount);
  }

  const lpWithdrawals: NonNullable<JitFillReport['withdrawals']> = [];
  let lpFill: JitFillReport | undefined;
  let lpFundsReturned = false;

  // Called on the success path so the report can record the withdrawals, and again from a `finally`
  // so an error between funding and there can't strand the deposit on the LP account. Idempotent so
  // the second call is a no-op; independent of the JIT fill, since even a failed fill deposited from
  // our wallet and that has to come back.
  const settleLp = async () => {
    if (lpFundsReturned || !cfLp) {
      return;
    }
    lpFundsReturned = true;
    lpWithdrawals.push(...(await returnLpFunds(cfLp, lpFill, lpDeposited, logger)));
  };

  try {
    const sourceWallet = ourExternalWallet(sourceAsset);
    const destAddress = args.destAddress ?? ourExternalWallet(destAsset);

    const phaseDurationsMs: Record<string, number> = {};
    const startPhase = () => Date.now();
    const endPhase = (name: string, start: number) => {
      phaseDurationsMs[name] = Date.now() - start;
    };

    // ---- starting balances ----
    const sourceBalanceBefore = await getBalance(sourceAsset, sourceWallet);
    const destBalanceBefore = await getBalance(destAsset, destAddress);
    logger.info(
      `Swapping ${amount} ${sourceAsset} -> ${destAsset} (source wallet ${sourceWallet}: ${sourceBalanceBefore} ${sourceAsset}, dest ${destAddress}: ${destBalanceBefore} ${destAsset})`,
    );

    // ---- deposit channel with fill-or-kill floor ----
    let phase = startPhase();
    const { minPriceX128, quotedOutput } = await computeFokMinPrice(
      sourceAsset,
      destAsset,
      amount,
      toleranceBps,
    );
    logger.info(
      `Quoted output: ${quotedOutput} ${destAsset}, fill-or-kill tolerance ${toleranceBps} bps`,
    );
    const sweepFromBlock = await currentBlockHeight();
    const channel = await requestLiveDepositChannel(cf, {
      sourceAsset,
      destAsset,
      destAddress,
      refundAddress: sourceWallet,
      minPriceX128,
      retryDurationBlocks: refundDurationBlocks,
      dcaParams,
    });
    endPhase('requestDepositChannel', phase);
    logger.info(
      `Deposit channel ${channel.swapParams.channelId} ready: ${channel.swapParams.depositAddress}`,
    );

    // ---- arm our LP bot for this channel (before the deposit, so it can't miss the swap) ----
    const lpFillPromise: Promise<JitFillReport | undefined> = cfLp
      ? fillSwapJit(cfLp, {
          channelId: channel.swapParams.channelId,
          sourceAsset,
          destAsset,
          fixedPrice: args.lpPrice,
          timeoutSeconds: timeouts.depositWitnessSeconds + timeouts.swapCompletionSeconds,
        }).catch((error) => {
          // The swap outcome stays authoritative: a lost JIT race ends in a FoK refund, which
          // the tracking side reports.
          logger.error(`LP fill failed: ${error}`);
          return undefined;
        })
      : Promise.resolve(undefined);

    // ---- deposit ----
    phase = startPhase();
    const txReceipt = await send(logger, sourceAsset, channel.swapParams.depositAddress, amount);
    const depositTxHash: string | undefined = txReceipt?.transactionHash;
    const sourceBalanceAfterDeposit = await getBalance(sourceAsset, sourceWallet);
    endPhase('sendDeposit', phase);
    logger.info(
      `Deposit sent${depositTxHash ? ` (tx ${depositTxHash})` : ''}, waiting for the swap`,
    );

    // ---- track the swap on the state chain, with our LP filling it ----
    phase = startPhase();
    // `lpFill` is assigned rather than declared here: `settleLp` needs it from the outer scope so
    // the `finally` can still credit whatever our orders bought.
    const [tracked, fill] = await Promise.all([
      trackSwapToCompletion(cf, channel.swapParams, dcaParams?.numberOfChunks ?? 1),
      lpFillPromise,
    ]);
    lpFill = fill;
    endPhase('trackSwap', phase);

    // ---- confirm funds arrived on the external chain ----
    let destBalanceAfter = destBalanceBefore;
    if (tracked.outcome === 'success') {
      phase = startPhase();
      destBalanceAfter = String(
        await observeBalanceIncrease(
          logger,
          destAsset,
          destAddress,
          destBalanceBefore,
          timeouts.egressSeconds,
        ),
      );
      endPhase('destBalanceIncrease', phase);
    } else if (tracked.outcome === 'refunded') {
      logger.warn('Swap was refunded (fill-or-kill), waiting for funds back at the source wallet');
      phase = startPhase();
      await observeBalanceIncrease(
        logger,
        sourceAsset,
        sourceWallet,
        sourceBalanceAfterDeposit,
        timeouts.egressSeconds,
      );
      endPhase('refundBalanceIncrease', phase);
    }
    const sourceBalanceAfter = await getBalance(sourceAsset, sourceWallet);

    // ---- bring the run's LP entitlement back to our wallet (exercises real egress) ----
    await settleLp();
    // ---- assemble the report from the indexer ----
    const sweepToBlock = await currentBlockHeight();
    const events = await collectSwapEvents(
      logger,
      sweepFromBlock,
      sweepToBlock,
      {
        channelId: BigInt(channel.swapParams.channelId),
        depositAddress: channel.swapParams.depositAddress,
        swapRequestId: BigInt(tracked.swapRequestId),
      },
      [tracked.egress, tracked.refundEgress].flatMap((egress) =>
        egress?.broadcastId !== undefined
          ? [{ chain: egress.egressId.split('-')[0], broadcastId: egress.broadcastId }]
          : [],
      ),
    );

    const { outcome, ...swap } = tracked;
    return {
      network,
      genesisHash,
      startedAt: startedAt.toISOString(),
      finishedAt: new Date().toISOString(),
      sourceAsset,
      destAsset,
      amount,
      outcome,
      brokerAccount: cf.requirements.account.keypair.address,
      channel: {
        channelId: String(channel.swapParams.channelId),
        depositAddress: channel.swapParams.depositAddress,
        issuedAtBlock: sweepFromBlock,
        sourceChainExpiryBlock: channel.sourceChainExpiryBlock.toString(),
      },
      depositTxHash,
      swap,
      externalBalances: {
        source: {
          asset: sourceAsset,
          address: sourceWallet,
          before: sourceBalanceBefore,
          after: sourceBalanceAfter,
        },
        dest: {
          asset: destAsset,
          address: destAddress,
          before: destBalanceBefore,
          after: destBalanceAfter,
        },
      },
      lpFill:
        cfLp && lpFill
          ? {
              lpAccount: cfLp.requirements.account.keypair.address,
              ...lpFill,
              deposited: Object.fromEntries(
                [...lpDeposited].map(([asset, fineAmount]) => [asset, fineAmount.toString()]),
              ),
              withdrawals: lpWithdrawals,
            }
          : undefined,
      phaseDurationsMs,
      events,
    };
  } finally {
    // Safety net for every path that skips the call above - a timeout while tracking the swap, a
    // failed JIT fill, an egress that never lands. Whatever we deposited came from our own wallet
    // and must not be left stranded on the LP account.
    await settleLp().catch((error) =>
      logger.error(`Could not return the LP funds to our wallet: ${error}`),
    );
  }
}
