import { Signer, ContractReceipt, Event, EventFilter, BaseContract, CallOverrides, BigNumber, BigNumberish, Overrides, ContractTransaction, PopulatedTransaction, utils, BytesLike } from 'ethers';
import { z } from 'zod';
import { FunctionFragment, Result } from '@ethersproject/abi';
import { Listener, Provider } from '@ethersproject/providers';
import { Logger } from 'winston';
import EventEmitter from 'events';

declare const tokenSwapParamsSchema: z.ZodUnion<[z.ZodObject<{
    amount: z.ZodUnion<[z.ZodString, z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>, z.ZodBigInt]>;
    srcChain: z.ZodLiteral<"Ethereum">;
    destChain: z.ZodLiteral<"Ethereum">;
    destAddress: z.ZodEffects<z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>, `0x${string}`, `0x${string}`>;
    srcAsset: z.ZodLiteral<"FLIP">;
    destAsset: z.ZodUnion<[z.ZodLiteral<"USDC">, z.ZodLiteral<"ETH">]>;
}, "strip", z.ZodTypeAny, {
    amount: string | bigint;
    srcChain: "Ethereum";
    srcAsset: "FLIP";
    destChain: "Ethereum";
    destAddress: `0x${string}`;
    destAsset: "USDC" | "ETH";
}, {
    amount: string | bigint;
    srcChain: "Ethereum";
    srcAsset: "FLIP";
    destChain: "Ethereum";
    destAddress: `0x${string}`;
    destAsset: "USDC" | "ETH";
}>, z.ZodObject<{
    amount: z.ZodUnion<[z.ZodString, z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>, z.ZodBigInt]>;
    srcChain: z.ZodLiteral<"Ethereum">;
    destChain: z.ZodLiteral<"Ethereum">;
    destAddress: z.ZodEffects<z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>, `0x${string}`, `0x${string}`>;
    srcAsset: z.ZodLiteral<"USDC">;
    destAsset: z.ZodUnion<[z.ZodLiteral<"FLIP">, z.ZodLiteral<"ETH">]>;
}, "strip", z.ZodTypeAny, {
    amount: string | bigint;
    srcChain: "Ethereum";
    srcAsset: "USDC";
    destChain: "Ethereum";
    destAddress: `0x${string}`;
    destAsset: "FLIP" | "ETH";
}, {
    amount: string | bigint;
    srcChain: "Ethereum";
    srcAsset: "USDC";
    destChain: "Ethereum";
    destAddress: `0x${string}`;
    destAsset: "FLIP" | "ETH";
}>, z.ZodObject<{
    amount: z.ZodUnion<[z.ZodString, z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>, z.ZodBigInt]>;
    srcChain: z.ZodLiteral<"Ethereum">;
    destChain: z.ZodLiteral<"Polkadot">;
    destAddress: z.ZodEffects<z.ZodEffects<z.ZodEffects<z.ZodUnion<[z.ZodString, z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>]>, string | null, string>, string, string>, string, string>;
    destAsset: z.ZodLiteral<"DOT">;
    srcAsset: z.ZodUnion<[z.ZodLiteral<"FLIP">, z.ZodLiteral<"USDC">]>;
}, "strip", z.ZodTypeAny, {
    amount: string | bigint;
    srcChain: "Ethereum";
    srcAsset: "FLIP" | "USDC";
    destChain: "Polkadot";
    destAddress: string;
    destAsset: "DOT";
}, {
    amount: string | bigint;
    srcChain: "Ethereum";
    srcAsset: "FLIP" | "USDC";
    destChain: "Polkadot";
    destAddress: string;
    destAsset: "DOT";
}>, z.ZodObject<{
    amount: z.ZodUnion<[z.ZodString, z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>, z.ZodBigInt]>;
    srcChain: z.ZodLiteral<"Ethereum">;
    destChain: z.ZodLiteral<"Bitcoin">;
    destAddress: z.ZodEffects<z.ZodUnion<[z.ZodEffects<z.ZodString, string, string>, z.ZodEffects<z.ZodString, string, string>, z.ZodEffects<z.ZodString, string, string>]>, string, string>;
    destAsset: z.ZodLiteral<"BTC">;
    srcAsset: z.ZodUnion<[z.ZodLiteral<"FLIP">, z.ZodLiteral<"USDC">]>;
}, "strip", z.ZodTypeAny, {
    amount: string | bigint;
    srcChain: "Ethereum";
    srcAsset: "FLIP" | "USDC";
    destChain: "Bitcoin";
    destAddress: string;
    destAsset: "BTC";
}, {
    amount: string | bigint;
    srcChain: "Ethereum";
    srcAsset: "FLIP" | "USDC";
    destChain: "Bitcoin";
    destAddress: string;
    destAsset: "BTC";
}>]>;
type TokenSwapParams = z.infer<typeof tokenSwapParamsSchema>;
declare const executeSwapParamsSchema: z.ZodUnion<[z.ZodUnion<[z.ZodObject<{
    amount: z.ZodUnion<[z.ZodString, z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>, z.ZodBigInt]>;
    srcChain: z.ZodLiteral<"Ethereum">;
    srcAsset: z.ZodLiteral<"ETH">;
    destChain: z.ZodLiteral<"Ethereum">;
    destAddress: z.ZodEffects<z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>, `0x${string}`, `0x${string}`>;
    destAsset: z.ZodUnion<[z.ZodLiteral<"FLIP">, z.ZodLiteral<"USDC">]>;
}, "strip", z.ZodTypeAny, {
    amount: string | bigint;
    srcChain: "Ethereum";
    srcAsset: "ETH";
    destChain: "Ethereum";
    destAddress: `0x${string}`;
    destAsset: "FLIP" | "USDC";
}, {
    amount: string | bigint;
    srcChain: "Ethereum";
    srcAsset: "ETH";
    destChain: "Ethereum";
    destAddress: `0x${string}`;
    destAsset: "FLIP" | "USDC";
}>, z.ZodObject<{
    amount: z.ZodUnion<[z.ZodString, z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>, z.ZodBigInt]>;
    srcChain: z.ZodLiteral<"Ethereum">;
    srcAsset: z.ZodLiteral<"ETH">;
    destChain: z.ZodLiteral<"Polkadot">;
    destAddress: z.ZodEffects<z.ZodEffects<z.ZodEffects<z.ZodUnion<[z.ZodString, z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>]>, string | null, string>, string, string>, string, string>;
    destAsset: z.ZodLiteral<"DOT">;
}, "strip", z.ZodTypeAny, {
    amount: string | bigint;
    srcChain: "Ethereum";
    srcAsset: "ETH";
    destChain: "Polkadot";
    destAddress: string;
    destAsset: "DOT";
}, {
    amount: string | bigint;
    srcChain: "Ethereum";
    srcAsset: "ETH";
    destChain: "Polkadot";
    destAddress: string;
    destAsset: "DOT";
}>, z.ZodObject<{
    amount: z.ZodUnion<[z.ZodString, z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>, z.ZodBigInt]>;
    srcChain: z.ZodLiteral<"Ethereum">;
    srcAsset: z.ZodLiteral<"ETH">;
    destChain: z.ZodLiteral<"Bitcoin">;
    destAddress: z.ZodEffects<z.ZodUnion<[z.ZodEffects<z.ZodString, string, string>, z.ZodEffects<z.ZodString, string, string>, z.ZodEffects<z.ZodString, string, string>]>, string, string>;
    destAsset: z.ZodLiteral<"BTC">;
}, "strip", z.ZodTypeAny, {
    amount: string | bigint;
    srcChain: "Ethereum";
    srcAsset: "ETH";
    destChain: "Bitcoin";
    destAddress: string;
    destAsset: "BTC";
}, {
    amount: string | bigint;
    srcChain: "Ethereum";
    srcAsset: "ETH";
    destChain: "Bitcoin";
    destAddress: string;
    destAsset: "BTC";
}>]>, z.ZodUnion<[z.ZodObject<{
    amount: z.ZodUnion<[z.ZodString, z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>, z.ZodBigInt]>;
    srcChain: z.ZodLiteral<"Ethereum">;
    destChain: z.ZodLiteral<"Ethereum">;
    destAddress: z.ZodEffects<z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>, `0x${string}`, `0x${string}`>;
    srcAsset: z.ZodLiteral<"FLIP">;
    destAsset: z.ZodUnion<[z.ZodLiteral<"USDC">, z.ZodLiteral<"ETH">]>;
}, "strip", z.ZodTypeAny, {
    amount: string | bigint;
    srcChain: "Ethereum";
    srcAsset: "FLIP";
    destChain: "Ethereum";
    destAddress: `0x${string}`;
    destAsset: "USDC" | "ETH";
}, {
    amount: string | bigint;
    srcChain: "Ethereum";
    srcAsset: "FLIP";
    destChain: "Ethereum";
    destAddress: `0x${string}`;
    destAsset: "USDC" | "ETH";
}>, z.ZodObject<{
    amount: z.ZodUnion<[z.ZodString, z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>, z.ZodBigInt]>;
    srcChain: z.ZodLiteral<"Ethereum">;
    destChain: z.ZodLiteral<"Ethereum">;
    destAddress: z.ZodEffects<z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>, `0x${string}`, `0x${string}`>;
    srcAsset: z.ZodLiteral<"USDC">;
    destAsset: z.ZodUnion<[z.ZodLiteral<"FLIP">, z.ZodLiteral<"ETH">]>;
}, "strip", z.ZodTypeAny, {
    amount: string | bigint;
    srcChain: "Ethereum";
    srcAsset: "USDC";
    destChain: "Ethereum";
    destAddress: `0x${string}`;
    destAsset: "FLIP" | "ETH";
}, {
    amount: string | bigint;
    srcChain: "Ethereum";
    srcAsset: "USDC";
    destChain: "Ethereum";
    destAddress: `0x${string}`;
    destAsset: "FLIP" | "ETH";
}>, z.ZodObject<{
    amount: z.ZodUnion<[z.ZodString, z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>, z.ZodBigInt]>;
    srcChain: z.ZodLiteral<"Ethereum">;
    destChain: z.ZodLiteral<"Polkadot">;
    destAddress: z.ZodEffects<z.ZodEffects<z.ZodEffects<z.ZodUnion<[z.ZodString, z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>]>, string | null, string>, string, string>, string, string>;
    destAsset: z.ZodLiteral<"DOT">;
    srcAsset: z.ZodUnion<[z.ZodLiteral<"FLIP">, z.ZodLiteral<"USDC">]>;
}, "strip", z.ZodTypeAny, {
    amount: string | bigint;
    srcChain: "Ethereum";
    srcAsset: "FLIP" | "USDC";
    destChain: "Polkadot";
    destAddress: string;
    destAsset: "DOT";
}, {
    amount: string | bigint;
    srcChain: "Ethereum";
    srcAsset: "FLIP" | "USDC";
    destChain: "Polkadot";
    destAddress: string;
    destAsset: "DOT";
}>, z.ZodObject<{
    amount: z.ZodUnion<[z.ZodString, z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>, z.ZodBigInt]>;
    srcChain: z.ZodLiteral<"Ethereum">;
    destChain: z.ZodLiteral<"Bitcoin">;
    destAddress: z.ZodEffects<z.ZodUnion<[z.ZodEffects<z.ZodString, string, string>, z.ZodEffects<z.ZodString, string, string>, z.ZodEffects<z.ZodString, string, string>]>, string, string>;
    destAsset: z.ZodLiteral<"BTC">;
    srcAsset: z.ZodUnion<[z.ZodLiteral<"FLIP">, z.ZodLiteral<"USDC">]>;
}, "strip", z.ZodTypeAny, {
    amount: string | bigint;
    srcChain: "Ethereum";
    srcAsset: "FLIP" | "USDC";
    destChain: "Bitcoin";
    destAddress: string;
    destAsset: "BTC";
}, {
    amount: string | bigint;
    srcChain: "Ethereum";
    srcAsset: "FLIP" | "USDC";
    destChain: "Bitcoin";
    destAddress: string;
    destAsset: "BTC";
}>]>]>;
type ExecuteSwapParams = z.infer<typeof executeSwapParamsSchema>;
declare const executeCallParamsSchema: z.ZodUnion<[z.ZodObject<{
    amount: z.ZodUnion<[z.ZodString, z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>, z.ZodBigInt]>;
    srcChain: z.ZodLiteral<"Ethereum">;
    srcAsset: z.ZodLiteral<"ETH">;
    destChain: z.ZodLiteral<"Ethereum">;
    destAddress: z.ZodEffects<z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>, `0x${string}`, `0x${string}`>;
    destAsset: z.ZodUnion<[z.ZodLiteral<"FLIP">, z.ZodLiteral<"USDC">]>;
    message: z.ZodString;
    gasAmount: z.ZodString;
}, "strip", z.ZodTypeAny, {
    amount: string | bigint;
    message: string;
    gasAmount: string;
    srcChain: "Ethereum";
    srcAsset: "ETH";
    destChain: "Ethereum";
    destAddress: `0x${string}`;
    destAsset: "FLIP" | "USDC";
}, {
    amount: string | bigint;
    message: string;
    gasAmount: string;
    srcChain: "Ethereum";
    srcAsset: "ETH";
    destChain: "Ethereum";
    destAddress: `0x${string}`;
    destAsset: "FLIP" | "USDC";
}>, z.ZodUnion<[z.ZodObject<{
    amount: z.ZodUnion<[z.ZodString, z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>, z.ZodBigInt]>;
    srcChain: z.ZodLiteral<"Ethereum">;
    destChain: z.ZodLiteral<"Ethereum">;
    destAddress: z.ZodEffects<z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>, `0x${string}`, `0x${string}`>;
    srcAsset: z.ZodLiteral<"FLIP">;
    destAsset: z.ZodUnion<[z.ZodLiteral<"USDC">, z.ZodLiteral<"ETH">]>;
    message: z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>;
    gasAmount: z.ZodString;
}, "strip", z.ZodTypeAny, {
    amount: string | bigint;
    message: `0x${string}`;
    gasAmount: string;
    srcChain: "Ethereum";
    srcAsset: "FLIP";
    destChain: "Ethereum";
    destAddress: `0x${string}`;
    destAsset: "USDC" | "ETH";
}, {
    amount: string | bigint;
    message: `0x${string}`;
    gasAmount: string;
    srcChain: "Ethereum";
    srcAsset: "FLIP";
    destChain: "Ethereum";
    destAddress: `0x${string}`;
    destAsset: "USDC" | "ETH";
}>, z.ZodObject<{
    amount: z.ZodUnion<[z.ZodString, z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>, z.ZodBigInt]>;
    srcChain: z.ZodLiteral<"Ethereum">;
    destChain: z.ZodLiteral<"Ethereum">;
    destAddress: z.ZodEffects<z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>, `0x${string}`, `0x${string}`>;
    srcAsset: z.ZodLiteral<"USDC">;
    destAsset: z.ZodUnion<[z.ZodLiteral<"FLIP">, z.ZodLiteral<"ETH">]>;
    message: z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>;
    gasAmount: z.ZodString;
}, "strip", z.ZodTypeAny, {
    amount: string | bigint;
    message: `0x${string}`;
    gasAmount: string;
    srcChain: "Ethereum";
    srcAsset: "USDC";
    destChain: "Ethereum";
    destAddress: `0x${string}`;
    destAsset: "FLIP" | "ETH";
}, {
    amount: string | bigint;
    message: `0x${string}`;
    gasAmount: string;
    srcChain: "Ethereum";
    srcAsset: "USDC";
    destChain: "Ethereum";
    destAddress: `0x${string}`;
    destAsset: "FLIP" | "ETH";
}>]>]>;
type ExecuteCallParams = z.infer<typeof executeCallParamsSchema>;
declare const executeOptionsSchema: z.ZodIntersection<z.ZodObject<{
    signer: z.ZodType<Signer, z.ZodTypeDef, Signer>;
    nonce: z.ZodOptional<z.ZodUnion<[z.ZodNumber, z.ZodBigInt, z.ZodString]>>;
}, "strip", z.ZodTypeAny, {
    signer: Signer;
    nonce?: string | number | bigint | undefined;
}, {
    signer: Signer;
    nonce?: string | number | bigint | undefined;
}>, z.ZodUnion<[z.ZodObject<{
    network: z.ZodNativeEnum<{
        sisyphos: "sisyphos";
        perseverance: "perseverance";
        mainnet: "mainnet";
        partnernet: "partnernet";
    }>;
}, "strip", z.ZodTypeAny, {
    network: "sisyphos" | "perseverance" | "mainnet" | "partnernet";
}, {
    network: "sisyphos" | "perseverance" | "mainnet" | "partnernet";
}>, z.ZodObject<{
    network: z.ZodLiteral<"localnet">;
    vaultContractAddress: z.ZodString;
    srcTokenContractAddress: z.ZodOptional<z.ZodString>;
}, "strip", z.ZodTypeAny, {
    network: "localnet";
    vaultContractAddress: string;
    srcTokenContractAddress?: string | undefined;
}, {
    network: "localnet";
    vaultContractAddress: string;
    srcTokenContractAddress?: string | undefined;
}>]>>;
type ExecuteOptions = z.infer<typeof executeOptionsSchema>;

declare const executeSwap: (params: ExecuteSwapParams, options: ExecuteOptions) => Promise<ContractReceipt>;

declare const executeCall: (params: ExecuteCallParams, options: ExecuteOptions) => Promise<ContractReceipt>;

interface TypedEvent<TArgsArray extends Array<any> = any, TArgsObject = any> extends Event {
    args: TArgsArray & TArgsObject;
}
interface TypedEventFilter<_TEvent extends TypedEvent> extends EventFilter {
}
interface TypedListener<TEvent extends TypedEvent> {
    (...listenerArg: [...__TypechainArgsArray<TEvent>, TEvent]): void;
}
type __TypechainArgsArray<T> = T extends TypedEvent<infer U> ? U : never;
interface OnEvent<TRes> {
    <TEvent extends TypedEvent>(eventFilter: TypedEventFilter<TEvent>, listener: TypedListener<TEvent>): TRes;
    (eventName: string, listener: Listener): TRes;
}

interface ERC20Interface extends utils.Interface {
    functions: {
        "allowance(address,address)": FunctionFragment;
        "approve(address,uint256)": FunctionFragment;
        "balanceOf(address)": FunctionFragment;
    };
    getFunction(nameOrSignatureOrTopic: "allowance" | "approve" | "balanceOf"): FunctionFragment;
    encodeFunctionData(functionFragment: "allowance", values: [string, string]): string;
    encodeFunctionData(functionFragment: "approve", values: [string, BigNumberish]): string;
    encodeFunctionData(functionFragment: "balanceOf", values: [string]): string;
    decodeFunctionResult(functionFragment: "allowance", data: BytesLike): Result;
    decodeFunctionResult(functionFragment: "approve", data: BytesLike): Result;
    decodeFunctionResult(functionFragment: "balanceOf", data: BytesLike): Result;
    events: {};
}
interface ERC20 extends BaseContract {
    connect(signerOrProvider: Signer | Provider | string): this;
    attach(addressOrName: string): this;
    deployed(): Promise<this>;
    interface: ERC20Interface;
    queryFilter<TEvent extends TypedEvent>(event: TypedEventFilter<TEvent>, fromBlockOrBlockhash?: string | number | undefined, toBlock?: string | number | undefined): Promise<Array<TEvent>>;
    listeners<TEvent extends TypedEvent>(eventFilter?: TypedEventFilter<TEvent>): Array<TypedListener<TEvent>>;
    listeners(eventName?: string): Array<Listener>;
    removeAllListeners<TEvent extends TypedEvent>(eventFilter: TypedEventFilter<TEvent>): this;
    removeAllListeners(eventName?: string): this;
    off: OnEvent<this>;
    on: OnEvent<this>;
    once: OnEvent<this>;
    removeListener: OnEvent<this>;
    functions: {
        allowance(owner: string, spender: string, overrides?: CallOverrides): Promise<[BigNumber]>;
        approve(spender: string, amount: BigNumberish, overrides?: Overrides & {
            from?: string;
        }): Promise<ContractTransaction>;
        balanceOf(account: string, overrides?: CallOverrides): Promise<[BigNumber]>;
    };
    allowance(owner: string, spender: string, overrides?: CallOverrides): Promise<BigNumber>;
    approve(spender: string, amount: BigNumberish, overrides?: Overrides & {
        from?: string;
    }): Promise<ContractTransaction>;
    balanceOf(account: string, overrides?: CallOverrides): Promise<BigNumber>;
    callStatic: {
        allowance(owner: string, spender: string, overrides?: CallOverrides): Promise<BigNumber>;
        approve(spender: string, amount: BigNumberish, overrides?: CallOverrides): Promise<boolean>;
        balanceOf(account: string, overrides?: CallOverrides): Promise<BigNumber>;
    };
    filters: {};
    estimateGas: {
        allowance(owner: string, spender: string, overrides?: CallOverrides): Promise<BigNumber>;
        approve(spender: string, amount: BigNumberish, overrides?: Overrides & {
            from?: string;
        }): Promise<BigNumber>;
        balanceOf(account: string, overrides?: CallOverrides): Promise<BigNumber>;
    };
    populateTransaction: {
        allowance(owner: string, spender: string, overrides?: CallOverrides): Promise<PopulatedTransaction>;
        approve(spender: string, amount: BigNumberish, overrides?: Overrides & {
            from?: string;
        }): Promise<PopulatedTransaction>;
        balanceOf(account: string, overrides?: CallOverrides): Promise<PopulatedTransaction>;
    };
}

type ArrayToMap<T extends readonly string[]> = {
    [K in T[number]]: K;
};
declare const Chains: ArrayToMap<readonly ["Bitcoin", "Ethereum", "Polkadot", "Arbitrum"]>;
type Chain = (typeof Chains)[keyof typeof Chains];
declare const Assets: ArrayToMap<readonly ["FLIP", "USDC", "DOT", "ETH", "BTC", "ARBETH", "ARBUSDC"]>;
type Asset = (typeof Assets)[keyof typeof Assets];
declare const ChainflipNetworks: ArrayToMap<readonly ["sisyphos", "perseverance", "mainnet", "partnernet"]>;
type ChainflipNetwork = (typeof ChainflipNetworks)[keyof typeof ChainflipNetworks];
declare const assetChains: {
    ETH: "Ethereum";
    FLIP: "Ethereum";
    USDC: "Ethereum";
    BTC: "Bitcoin";
    DOT: "Polkadot";
    ARBETH: "Arbitrum";
    ARBUSDC: "Arbitrum";
};
declare const assetDecimals: {
    DOT: number;
    ETH: number;
    FLIP: number;
    USDC: number;
    BTC: number;
    ARBETH: number;
    ARBUSDC: number;
};
declare const assetContractIds: Record<Asset, number>;
declare const chainAssets: {
    Ethereum: ("FLIP" | "USDC" | "ETH")[];
    Bitcoin: "BTC"[];
    Polkadot: "DOT"[];
    Arbitrum: ("ARBETH" | "ARBUSDC")[];
};
declare const chainContractIds: Record<Chain, number>;

declare const checkAllowance: (amount: BigNumberish, spenderAddress: string, erc20Address: string, signer: Signer) => Promise<{
    allowance: BigNumber;
    isAllowable: boolean;
    erc20: ERC20;
}>;

declare const checkVaultAllowance: (params: Pick<TokenSwapParams, 'srcAsset' | 'amount'>, opts: ExecuteOptions) => ReturnType<typeof checkAllowance>;
declare const approveVault: (params: Pick<TokenSwapParams, 'srcAsset' | 'amount'>, opts: ExecuteOptions) => Promise<ContractReceipt | null>;

declare const checkStateChainGatewayAllowance: (amount: bigint | string | number, options: FundStateChainAccountOptions) => ReturnType<typeof checkAllowance>;
declare const approveStateChainGateway: (amount: bigint | string | number, options: FundStateChainAccountOptions) => Promise<ContractReceipt | null>;

type WithNonce<T> = T & {
    nonce?: number | bigint | string;
};
type SignerOptions = WithNonce<{
    network: ChainflipNetwork;
    signer: Signer;
} | {
    network: 'localnet';
    signer: Signer;
    stateChainGatewayContractAddress: string;
}>;
type ExtendLocalnetOptions<T, U> = T extends {
    network: 'localnet';
} ? T & U : T;
type FundStateChainAccountOptions = ExtendLocalnetOptions<SignerOptions, {
    flipContractAddress: string;
}>;
declare const fundStateChainAccount: (accountId: `0x${string}`, amount: string, options: FundStateChainAccountOptions) => Promise<ContractReceipt>;
declare const executeRedemption: (accountId: `0x${string}`, { nonce, ...options }: WithNonce<SignerOptions>) => Promise<ContractReceipt>;
declare const getMinimumFunding: (options: SignerOptions) => Promise<BigNumber>;
declare const getRedemptionDelay: (options: SignerOptions) => Promise<number>;

declare const ccmMetadataSchema: z.ZodObject<{
    gasBudget: z.ZodUnion<[z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>, z.ZodNumber]>;
    message: z.ZodUnion<[z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>, z.ZodString]>;
    cfParameters: z.ZodOptional<z.ZodUnion<[z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>, z.ZodString]>>;
}, "strip", z.ZodTypeAny, {
    message: string;
    gasBudget: number | `0x${string}`;
    cfParameters?: string | undefined;
}, {
    message: string;
    gasBudget: number | `0x${string}`;
    cfParameters?: string | undefined;
}>;
type CcmMetadata = z.infer<typeof ccmMetadataSchema>;

declare class RpcClient<Req extends Record<string, z.ZodTypeAny>, Res extends Record<string, z.ZodTypeAny>> extends EventEmitter {
    private readonly url;
    private readonly requestMap;
    private readonly responseMap;
    private readonly namespace;
    private readonly logger?;
    private socket;
    private requestId;
    private messages;
    private reconnectAttempts;
    constructor(url: string, requestMap: Req, responseMap: Res, namespace: string, logger?: Logger | undefined);
    close(): Promise<void>;
    private handleClose;
    private connectionReady;
    private handleDisconnect;
    connect(): Promise<this>;
    sendRequest<R extends keyof Req & keyof Res>(method: R, ...params: z.input<Req[R]>): Promise<z.infer<Res[R]>>;
}

type CamelCaseToSnakeCase<S extends string> = S extends `${infer T}${infer U}` ? `${T extends Capitalize<T> ? '_' : ''}${Lowercase<T>}${CamelCaseToSnakeCase<U>}` : S;

type NewSwapRequest = {
    srcAsset: Asset;
    destAsset: Asset;
    srcChain: Chain;
    destChain: Chain;
    destAddress: string;
    ccmMetadata?: CcmMetadata;
};
type SnakeCaseKeys<T> = {
    [K in keyof T as K extends string ? CamelCaseToSnakeCase<K> : K]: T[K];
};
declare const requestValidators: {
    requestSwapDepositAddress: z.ZodEffects<z.ZodTuple<[z.ZodEffects<z.ZodNativeEnum<{
        FLIP: "FLIP";
        USDC: "USDC";
        DOT: "DOT";
        ETH: "ETH";
        BTC: "BTC";
        ARBETH: "ARBETH";
        ARBUSDC: "ARBUSDC";
    }>, "Usdc" | "Flip" | "Dot" | "Eth" | "Btc" | "Arbeth" | "Arbusdc", "FLIP" | "USDC" | "DOT" | "ETH" | "BTC" | "ARBETH" | "ARBUSDC">, z.ZodEffects<z.ZodNativeEnum<{
        FLIP: "FLIP";
        USDC: "USDC";
        DOT: "DOT";
        ETH: "ETH";
        BTC: "BTC";
        ARBETH: "ARBETH";
        ARBUSDC: "ARBUSDC";
    }>, "Usdc" | "Flip" | "Dot" | "Eth" | "Btc" | "Arbeth" | "Arbusdc", "FLIP" | "USDC" | "DOT" | "ETH" | "BTC" | "ARBETH" | "ARBUSDC">, z.ZodUnion<[z.ZodString, z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>, z.ZodUnion<[z.ZodEffects<z.ZodString, string, string>, z.ZodEffects<z.ZodString, string, string>, z.ZodEffects<z.ZodString, string, string>]>]>, z.ZodNumber, z.ZodOptional<z.ZodObject<{
        cfParameters: z.ZodOptional<z.ZodUnion<[z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>, z.ZodString]>>;
        message: z.ZodUnion<[z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>, z.ZodString]>;
        gasBudget: z.ZodUnion<[z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>, z.ZodNumber]>;
        source_chain: z.ZodNativeEnum<{
            Bitcoin: "Bitcoin";
            Ethereum: "Ethereum";
            Polkadot: "Polkadot";
            Arbitrum: "Arbitrum";
        }>;
        source_address: z.ZodUnion<[z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>, z.ZodUnion<[z.ZodEffects<z.ZodString, string, string>, z.ZodEffects<z.ZodString, string, string>, z.ZodEffects<z.ZodString, string, string>]>, z.ZodEffects<z.ZodEffects<z.ZodUnion<[z.ZodString, z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>]>, string | null, string>, string, string>]>;
    }, "strip", z.ZodTypeAny, {
        message: string;
        gasBudget: number | `0x${string}`;
        source_chain: "Bitcoin" | "Ethereum" | "Polkadot" | "Arbitrum";
        source_address: string;
        cfParameters?: string | undefined;
    }, {
        message: string;
        gasBudget: number | `0x${string}`;
        source_chain: "Bitcoin" | "Ethereum" | "Polkadot" | "Arbitrum";
        source_address: string;
        cfParameters?: string | undefined;
    }>>], null>, (string | number | SnakeCaseKeys<{
        message: string;
        gasBudget: number | `0x${string}`;
        source_chain: "Bitcoin" | "Ethereum" | "Polkadot" | "Arbitrum";
        source_address: string;
        cfParameters?: string | undefined;
    }>)[], ["FLIP" | "USDC" | "DOT" | "ETH" | "BTC" | "ARBETH" | "ARBUSDC", "FLIP" | "USDC" | "DOT" | "ETH" | "BTC" | "ARBETH" | "ARBUSDC", string, number, {
        message: string;
        gasBudget: number | `0x${string}`;
        source_chain: "Bitcoin" | "Ethereum" | "Polkadot" | "Arbitrum";
        source_address: string;
        cfParameters?: string | undefined;
    } | undefined]>;
};
declare const responseValidators: {
    requestSwapDepositAddress: z.ZodEffects<z.ZodObject<{
        address: z.ZodUnion<[z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>, z.ZodUnion<[z.ZodEffects<z.ZodString, string, string>, z.ZodEffects<z.ZodString, string, string>, z.ZodEffects<z.ZodString, string, string>]>, z.ZodEffects<z.ZodEffects<z.ZodUnion<[z.ZodString, z.ZodType<`0x${string}`, z.ZodTypeDef, `0x${string}`>]>, string | null, string>, string, string>]>;
        expiry_block: z.ZodNumber;
        issued_block: z.ZodNumber;
        channel_id: z.ZodNumber;
    }, "strip", z.ZodTypeAny, {
        address: string;
        expiry_block: number;
        issued_block: number;
        channel_id: number;
    }, {
        address: string;
        expiry_block: number;
        issued_block: number;
        channel_id: number;
    }>, {
        address: string;
        expiryBlock: number;
        issuedBlock: number;
        channelId: bigint;
    }, {
        address: string;
        expiry_block: number;
        issued_block: number;
        channel_id: number;
    }>;
};
type DepositChannelResponse = z.infer<(typeof responseValidators)['requestSwapDepositAddress']>;
type BrokerClientOpts = {
    url?: string;
    logger?: Logger;
};
declare class BrokerClient extends RpcClient<typeof requestValidators, typeof responseValidators> {
    static create(opts?: BrokerClientOpts): Promise<BrokerClient>;
    private constructor();
    requestSwapDepositAddress(swapRequest: NewSwapRequest): Promise<DepositChannelResponse>;
}

export { Asset, Assets, BrokerClient, Chain, ChainflipNetwork, ChainflipNetworks, Chains, ExecuteCallParams, ExecuteOptions, ExecuteSwapParams, FundStateChainAccountOptions, approveStateChainGateway, approveVault, assetChains, assetContractIds, assetDecimals, chainAssets, chainContractIds, checkStateChainGatewayAllowance, checkVaultAllowance, executeCall, executeRedemption, executeSwap, fundStateChainAccount, getMinimumFunding, getRedemptionDelay };
