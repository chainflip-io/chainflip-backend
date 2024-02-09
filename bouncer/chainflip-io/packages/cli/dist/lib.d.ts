import { DeferredTopicFilter, EventFragment, EventLog, ContractTransactionResponse, FunctionFragment, ContractTransaction, Typed, TransactionRequest, BaseContract, ContractRunner, Listener, AddressLike, BigNumberish, ContractMethod, Interface, BytesLike, Result, Signer, ContractTransactionReceipt } from 'ethers';
import { z } from 'zod';

type ArrayToMap<T extends readonly string[]> = {
    [K in T[number]]: K;
};
declare const Chains: ArrayToMap<readonly ["Bitcoin", "Ethereum", "Polkadot", "Arbitrum"]>;
type Chain = (typeof Chains)[keyof typeof Chains];
declare const Assets: ArrayToMap<readonly ["FLIP", "USDC", "DOT", "ETH", "BTC", "ARBETH", "ARBUSDC"]>;
type Asset = (typeof Assets)[keyof typeof Assets];
declare const ChainflipNetworks: ArrayToMap<readonly ["backspin", "sisyphos", "perseverance", "mainnet"]>;
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

declare const tokenSwapParamsSchema: (network: ChainflipNetwork) => z.ZodUnion<[z.ZodObject<{
    amount: z.ZodString;
    srcChain: z.ZodLiteral<"Ethereum">;
    destChain: z.ZodLiteral<"Ethereum">;
    destAddress: z.ZodEffects<z.ZodEffects<z.ZodString, `0x${string}`, string>, `0x${string}`, string>;
    srcAsset: z.ZodLiteral<"FLIP">;
    destAsset: z.ZodUnion<[z.ZodLiteral<"USDC">, z.ZodLiteral<"ETH">]>;
}, "strip", z.ZodTypeAny, {
    amount: string;
    srcChain: "Ethereum";
    srcAsset: "FLIP";
    destChain: "Ethereum";
    destAddress: `0x${string}`;
    destAsset: "USDC" | "ETH";
}, {
    amount: string;
    srcChain: "Ethereum";
    srcAsset: "FLIP";
    destChain: "Ethereum";
    destAddress: string;
    destAsset: "USDC" | "ETH";
}>, z.ZodObject<{
    amount: z.ZodString;
    srcChain: z.ZodLiteral<"Ethereum">;
    destChain: z.ZodLiteral<"Ethereum">;
    destAddress: z.ZodEffects<z.ZodEffects<z.ZodString, `0x${string}`, string>, `0x${string}`, string>;
    srcAsset: z.ZodLiteral<"USDC">;
    destAsset: z.ZodUnion<[z.ZodLiteral<"FLIP">, z.ZodLiteral<"ETH">]>;
}, "strip", z.ZodTypeAny, {
    amount: string;
    srcChain: "Ethereum";
    srcAsset: "USDC";
    destChain: "Ethereum";
    destAddress: `0x${string}`;
    destAsset: "FLIP" | "ETH";
}, {
    amount: string;
    srcChain: "Ethereum";
    srcAsset: "USDC";
    destChain: "Ethereum";
    destAddress: string;
    destAsset: "FLIP" | "ETH";
}>, z.ZodObject<{
    amount: z.ZodString;
    srcChain: z.ZodLiteral<"Ethereum">;
    destChain: z.ZodLiteral<"Polkadot">;
    destAddress: z.ZodEffects<z.ZodEffects<z.ZodEffects<z.ZodUnion<[z.ZodString, z.ZodEffects<z.ZodString, `0x${string}`, string>]>, string | null, string>, string, string>, string, string>;
    destAsset: z.ZodLiteral<"DOT">;
    srcAsset: z.ZodUnion<[z.ZodLiteral<"FLIP">, z.ZodLiteral<"USDC">]>;
}, "strip", z.ZodTypeAny, {
    amount: string;
    srcChain: "Ethereum";
    srcAsset: "FLIP" | "USDC";
    destChain: "Polkadot";
    destAddress: string;
    destAsset: "DOT";
}, {
    amount: string;
    srcChain: "Ethereum";
    srcAsset: "FLIP" | "USDC";
    destChain: "Polkadot";
    destAddress: string;
    destAsset: "DOT";
}>, z.ZodObject<{
    amount: z.ZodString;
    srcChain: z.ZodLiteral<"Ethereum">;
    destChain: z.ZodLiteral<"Bitcoin">;
    destAddress: z.ZodEffects<z.ZodEffects<z.ZodString, string, string>, string, string> | z.ZodEffects<z.ZodUnion<[z.ZodEffects<z.ZodString, string, string>, z.ZodEffects<z.ZodString, string, string>]>, string, string>;
    destAsset: z.ZodLiteral<"BTC">;
    srcAsset: z.ZodUnion<[z.ZodLiteral<"FLIP">, z.ZodLiteral<"USDC">]>;
}, "strip", z.ZodTypeAny, {
    amount: string;
    srcChain: "Ethereum";
    srcAsset: "FLIP" | "USDC";
    destChain: "Bitcoin";
    destAddress: string;
    destAsset: "BTC";
}, {
    amount: string;
    srcChain: "Ethereum";
    srcAsset: "FLIP" | "USDC";
    destChain: "Bitcoin";
    destAddress: string;
    destAsset: "BTC";
}>]>;
declare const executeSwapParamsSchema: (network: ChainflipNetwork) => z.ZodUnion<[z.ZodObject<{
    amount: z.ZodString;
    srcChain: z.ZodLiteral<"Ethereum">;
    srcAsset: z.ZodLiteral<"ETH">;
    destChain: z.ZodLiteral<"Ethereum">;
    destAddress: z.ZodEffects<z.ZodEffects<z.ZodString, `0x${string}`, string>, `0x${string}`, string>;
    destAsset: z.ZodUnion<[z.ZodLiteral<"FLIP">, z.ZodLiteral<"USDC">]>;
    ccmMetadata: z.ZodObject<{
        gasBudget: z.ZodString;
        message: z.ZodEffects<z.ZodEffects<z.ZodString, `0x${string}`, string>, `0x${string}`, string>;
    }, "strip", z.ZodTypeAny, {
        message: `0x${string}`;
        gasBudget: string;
    }, {
        message: string;
        gasBudget: string;
    }>;
}, "strip", z.ZodTypeAny, {
    amount: string;
    srcChain: "Ethereum";
    srcAsset: "ETH";
    destChain: "Ethereum";
    destAddress: `0x${string}`;
    destAsset: "FLIP" | "USDC";
    ccmMetadata: {
        message: `0x${string}`;
        gasBudget: string;
    };
}, {
    amount: string;
    srcChain: "Ethereum";
    srcAsset: "ETH";
    destChain: "Ethereum";
    destAddress: string;
    destAsset: "FLIP" | "USDC";
    ccmMetadata: {
        message: string;
        gasBudget: string;
    };
}>, z.ZodUnion<[z.ZodObject<{
    amount: z.ZodString;
    srcChain: z.ZodLiteral<"Ethereum">;
    srcAsset: z.ZodLiteral<"FLIP">;
    destChain: z.ZodLiteral<"Ethereum">;
    destAddress: z.ZodEffects<z.ZodEffects<z.ZodString, `0x${string}`, string>, `0x${string}`, string>;
    destAsset: z.ZodUnion<[z.ZodLiteral<"USDC">, z.ZodLiteral<"ETH">]>;
    ccmMetadata: z.ZodObject<{
        gasBudget: z.ZodString;
        message: z.ZodEffects<z.ZodEffects<z.ZodString, `0x${string}`, string>, `0x${string}`, string>;
    }, "strip", z.ZodTypeAny, {
        message: `0x${string}`;
        gasBudget: string;
    }, {
        message: string;
        gasBudget: string;
    }>;
}, "strip", z.ZodTypeAny, {
    amount: string;
    srcChain: "Ethereum";
    srcAsset: "FLIP";
    destChain: "Ethereum";
    destAddress: `0x${string}`;
    destAsset: "USDC" | "ETH";
    ccmMetadata: {
        message: `0x${string}`;
        gasBudget: string;
    };
}, {
    amount: string;
    srcChain: "Ethereum";
    srcAsset: "FLIP";
    destChain: "Ethereum";
    destAddress: string;
    destAsset: "USDC" | "ETH";
    ccmMetadata: {
        message: string;
        gasBudget: string;
    };
}>, z.ZodObject<{
    amount: z.ZodString;
    srcChain: z.ZodLiteral<"Ethereum">;
    srcAsset: z.ZodLiteral<"USDC">;
    destChain: z.ZodLiteral<"Ethereum">;
    destAddress: z.ZodEffects<z.ZodEffects<z.ZodString, `0x${string}`, string>, `0x${string}`, string>;
    destAsset: z.ZodUnion<[z.ZodLiteral<"FLIP">, z.ZodLiteral<"ETH">]>;
    ccmMetadata: z.ZodObject<{
        gasBudget: z.ZodString;
        message: z.ZodEffects<z.ZodEffects<z.ZodString, `0x${string}`, string>, `0x${string}`, string>;
    }, "strip", z.ZodTypeAny, {
        message: `0x${string}`;
        gasBudget: string;
    }, {
        message: string;
        gasBudget: string;
    }>;
}, "strip", z.ZodTypeAny, {
    amount: string;
    srcChain: "Ethereum";
    srcAsset: "USDC";
    destChain: "Ethereum";
    destAddress: `0x${string}`;
    destAsset: "FLIP" | "ETH";
    ccmMetadata: {
        message: `0x${string}`;
        gasBudget: string;
    };
}, {
    amount: string;
    srcChain: "Ethereum";
    srcAsset: "USDC";
    destChain: "Ethereum";
    destAddress: string;
    destAsset: "FLIP" | "ETH";
    ccmMetadata: {
        message: string;
        gasBudget: string;
    };
}>]>, z.ZodUnion<[z.ZodObject<{
    amount: z.ZodString;
    srcChain: z.ZodLiteral<"Ethereum">;
    srcAsset: z.ZodLiteral<"ETH">;
    destChain: z.ZodLiteral<"Ethereum">;
    destAddress: z.ZodEffects<z.ZodEffects<z.ZodString, `0x${string}`, string>, `0x${string}`, string>;
    destAsset: z.ZodUnion<[z.ZodLiteral<"FLIP">, z.ZodLiteral<"USDC">]>;
}, "strip", z.ZodTypeAny, {
    amount: string;
    srcChain: "Ethereum";
    srcAsset: "ETH";
    destChain: "Ethereum";
    destAddress: `0x${string}`;
    destAsset: "FLIP" | "USDC";
}, {
    amount: string;
    srcChain: "Ethereum";
    srcAsset: "ETH";
    destChain: "Ethereum";
    destAddress: string;
    destAsset: "FLIP" | "USDC";
}>, z.ZodObject<{
    amount: z.ZodString;
    srcChain: z.ZodLiteral<"Ethereum">;
    srcAsset: z.ZodLiteral<"ETH">;
    destChain: z.ZodLiteral<"Polkadot">;
    destAddress: z.ZodEffects<z.ZodEffects<z.ZodEffects<z.ZodUnion<[z.ZodString, z.ZodEffects<z.ZodString, `0x${string}`, string>]>, string | null, string>, string, string>, string, string>;
    destAsset: z.ZodLiteral<"DOT">;
}, "strip", z.ZodTypeAny, {
    amount: string;
    srcChain: "Ethereum";
    srcAsset: "ETH";
    destChain: "Polkadot";
    destAddress: string;
    destAsset: "DOT";
}, {
    amount: string;
    srcChain: "Ethereum";
    srcAsset: "ETH";
    destChain: "Polkadot";
    destAddress: string;
    destAsset: "DOT";
}>, z.ZodObject<{
    amount: z.ZodString;
    srcChain: z.ZodLiteral<"Ethereum">;
    srcAsset: z.ZodLiteral<"ETH">;
    destChain: z.ZodLiteral<"Bitcoin">;
    destAddress: z.ZodEffects<z.ZodEffects<z.ZodString, string, string>, string, string> | z.ZodEffects<z.ZodUnion<[z.ZodEffects<z.ZodString, string, string>, z.ZodEffects<z.ZodString, string, string>]>, string, string>;
    destAsset: z.ZodLiteral<"BTC">;
}, "strip", z.ZodTypeAny, {
    amount: string;
    srcChain: "Ethereum";
    srcAsset: "ETH";
    destChain: "Bitcoin";
    destAddress: string;
    destAsset: "BTC";
}, {
    amount: string;
    srcChain: "Ethereum";
    srcAsset: "ETH";
    destChain: "Bitcoin";
    destAddress: string;
    destAsset: "BTC";
}>]>, z.ZodUnion<[z.ZodObject<{
    amount: z.ZodString;
    srcChain: z.ZodLiteral<"Ethereum">;
    destChain: z.ZodLiteral<"Ethereum">;
    destAddress: z.ZodEffects<z.ZodEffects<z.ZodString, `0x${string}`, string>, `0x${string}`, string>;
    srcAsset: z.ZodLiteral<"FLIP">;
    destAsset: z.ZodUnion<[z.ZodLiteral<"USDC">, z.ZodLiteral<"ETH">]>;
}, "strip", z.ZodTypeAny, {
    amount: string;
    srcChain: "Ethereum";
    srcAsset: "FLIP";
    destChain: "Ethereum";
    destAddress: `0x${string}`;
    destAsset: "USDC" | "ETH";
}, {
    amount: string;
    srcChain: "Ethereum";
    srcAsset: "FLIP";
    destChain: "Ethereum";
    destAddress: string;
    destAsset: "USDC" | "ETH";
}>, z.ZodObject<{
    amount: z.ZodString;
    srcChain: z.ZodLiteral<"Ethereum">;
    destChain: z.ZodLiteral<"Ethereum">;
    destAddress: z.ZodEffects<z.ZodEffects<z.ZodString, `0x${string}`, string>, `0x${string}`, string>;
    srcAsset: z.ZodLiteral<"USDC">;
    destAsset: z.ZodUnion<[z.ZodLiteral<"FLIP">, z.ZodLiteral<"ETH">]>;
}, "strip", z.ZodTypeAny, {
    amount: string;
    srcChain: "Ethereum";
    srcAsset: "USDC";
    destChain: "Ethereum";
    destAddress: `0x${string}`;
    destAsset: "FLIP" | "ETH";
}, {
    amount: string;
    srcChain: "Ethereum";
    srcAsset: "USDC";
    destChain: "Ethereum";
    destAddress: string;
    destAsset: "FLIP" | "ETH";
}>, z.ZodObject<{
    amount: z.ZodString;
    srcChain: z.ZodLiteral<"Ethereum">;
    destChain: z.ZodLiteral<"Polkadot">;
    destAddress: z.ZodEffects<z.ZodEffects<z.ZodEffects<z.ZodUnion<[z.ZodString, z.ZodEffects<z.ZodString, `0x${string}`, string>]>, string | null, string>, string, string>, string, string>;
    destAsset: z.ZodLiteral<"DOT">;
    srcAsset: z.ZodUnion<[z.ZodLiteral<"FLIP">, z.ZodLiteral<"USDC">]>;
}, "strip", z.ZodTypeAny, {
    amount: string;
    srcChain: "Ethereum";
    srcAsset: "FLIP" | "USDC";
    destChain: "Polkadot";
    destAddress: string;
    destAsset: "DOT";
}, {
    amount: string;
    srcChain: "Ethereum";
    srcAsset: "FLIP" | "USDC";
    destChain: "Polkadot";
    destAddress: string;
    destAsset: "DOT";
}>, z.ZodObject<{
    amount: z.ZodString;
    srcChain: z.ZodLiteral<"Ethereum">;
    destChain: z.ZodLiteral<"Bitcoin">;
    destAddress: z.ZodEffects<z.ZodEffects<z.ZodString, string, string>, string, string> | z.ZodEffects<z.ZodUnion<[z.ZodEffects<z.ZodString, string, string>, z.ZodEffects<z.ZodString, string, string>]>, string, string>;
    destAsset: z.ZodLiteral<"BTC">;
    srcAsset: z.ZodUnion<[z.ZodLiteral<"FLIP">, z.ZodLiteral<"USDC">]>;
}, "strip", z.ZodTypeAny, {
    amount: string;
    srcChain: "Ethereum";
    srcAsset: "FLIP" | "USDC";
    destChain: "Bitcoin";
    destAddress: string;
    destAsset: "BTC";
}, {
    amount: string;
    srcChain: "Ethereum";
    srcAsset: "FLIP" | "USDC";
    destChain: "Bitcoin";
    destAddress: string;
    destAsset: "BTC";
}>]>]>;
type ExecuteSwapParams = z.infer<ReturnType<typeof executeSwapParamsSchema>>;
type TokenSwapParams = z.infer<ReturnType<typeof tokenSwapParamsSchema>>;

interface TypedDeferredTopicFilter<_TCEvent extends TypedContractEvent> extends DeferredTopicFilter {
}
interface TypedContractEvent<InputTuple extends Array<any> = any, OutputTuple extends Array<any> = any, OutputObject = any> {
    (...args: Partial<InputTuple>): TypedDeferredTopicFilter<TypedContractEvent<InputTuple, OutputTuple, OutputObject>>;
    name: string;
    fragment: EventFragment;
    getFragment(...args: Partial<InputTuple>): EventFragment;
}
type __TypechainAOutputTuple<T> = T extends TypedContractEvent<infer _U, infer W> ? W : never;
type __TypechainOutputObject<T> = T extends TypedContractEvent<infer _U, infer _W, infer V> ? V : never;
interface TypedEventLog<TCEvent extends TypedContractEvent> extends Omit<EventLog, "args"> {
    args: __TypechainAOutputTuple<TCEvent> & __TypechainOutputObject<TCEvent>;
}
type TypedListener<TCEvent extends TypedContractEvent> = (...listenerArg: [
    ...__TypechainAOutputTuple<TCEvent>,
    TypedEventLog<TCEvent>,
    ...undefined[]
]) => void;
type StateMutability = "nonpayable" | "payable" | "view";
type BaseOverrides = Omit<TransactionRequest, "to" | "data">;
type NonPayableOverrides = Omit<BaseOverrides, "value" | "blockTag" | "enableCcipRead">;
type PayableOverrides = Omit<BaseOverrides, "blockTag" | "enableCcipRead">;
type ViewOverrides = Omit<TransactionRequest, "to" | "data">;
type Overrides<S extends StateMutability> = S extends "nonpayable" ? NonPayableOverrides : S extends "payable" ? PayableOverrides : ViewOverrides;
type PostfixOverrides<A extends Array<any>, S extends StateMutability> = A | [...A, Overrides<S>];
type ContractMethodArgs<A extends Array<any>, S extends StateMutability> = PostfixOverrides<{
    [I in keyof A]-?: A[I] | Typed;
}, S>;
type DefaultReturnType<R> = R extends Array<any> ? R[0] : R;
interface TypedContractMethod<A extends Array<any> = Array<any>, R = any, S extends StateMutability = "payable"> {
    (...args: ContractMethodArgs<A, S>): S extends "view" ? Promise<DefaultReturnType<R>> : Promise<ContractTransactionResponse>;
    name: string;
    fragment: FunctionFragment;
    getFragment(...args: ContractMethodArgs<A, S>): FunctionFragment;
    populateTransaction(...args: ContractMethodArgs<A, S>): Promise<ContractTransaction>;
    staticCall(...args: ContractMethodArgs<A, "view">): Promise<DefaultReturnType<R>>;
    send(...args: ContractMethodArgs<A, S>): Promise<ContractTransactionResponse>;
    estimateGas(...args: ContractMethodArgs<A, S>): Promise<bigint>;
    staticCallResult(...args: ContractMethodArgs<A, "view">): Promise<R>;
}

interface ERC20Interface extends Interface {
    getFunction(nameOrSignature: "allowance" | "approve" | "balanceOf"): FunctionFragment;
    encodeFunctionData(functionFragment: "allowance", values: [AddressLike, AddressLike]): string;
    encodeFunctionData(functionFragment: "approve", values: [AddressLike, BigNumberish]): string;
    encodeFunctionData(functionFragment: "balanceOf", values: [AddressLike]): string;
    decodeFunctionResult(functionFragment: "allowance", data: BytesLike): Result;
    decodeFunctionResult(functionFragment: "approve", data: BytesLike): Result;
    decodeFunctionResult(functionFragment: "balanceOf", data: BytesLike): Result;
}
interface ERC20 extends BaseContract {
    connect(runner?: ContractRunner | null): ERC20;
    waitForDeployment(): Promise<this>;
    interface: ERC20Interface;
    queryFilter<TCEvent extends TypedContractEvent>(event: TCEvent, fromBlockOrBlockhash?: string | number | undefined, toBlock?: string | number | undefined): Promise<Array<TypedEventLog<TCEvent>>>;
    queryFilter<TCEvent extends TypedContractEvent>(filter: TypedDeferredTopicFilter<TCEvent>, fromBlockOrBlockhash?: string | number | undefined, toBlock?: string | number | undefined): Promise<Array<TypedEventLog<TCEvent>>>;
    on<TCEvent extends TypedContractEvent>(event: TCEvent, listener: TypedListener<TCEvent>): Promise<this>;
    on<TCEvent extends TypedContractEvent>(filter: TypedDeferredTopicFilter<TCEvent>, listener: TypedListener<TCEvent>): Promise<this>;
    once<TCEvent extends TypedContractEvent>(event: TCEvent, listener: TypedListener<TCEvent>): Promise<this>;
    once<TCEvent extends TypedContractEvent>(filter: TypedDeferredTopicFilter<TCEvent>, listener: TypedListener<TCEvent>): Promise<this>;
    listeners<TCEvent extends TypedContractEvent>(event: TCEvent): Promise<Array<TypedListener<TCEvent>>>;
    listeners(eventName?: string): Promise<Array<Listener>>;
    removeAllListeners<TCEvent extends TypedContractEvent>(event?: TCEvent): Promise<this>;
    allowance: TypedContractMethod<[
        owner: AddressLike,
        spender: AddressLike
    ], [
        bigint
    ], "view">;
    approve: TypedContractMethod<[
        spender: AddressLike,
        amount: BigNumberish
    ], [
        boolean
    ], "nonpayable">;
    balanceOf: TypedContractMethod<[account: AddressLike], [bigint], "view">;
    getFunction<T extends ContractMethod = ContractMethod>(key: string | FunctionFragment): T;
    getFunction(nameOrSignature: "allowance"): TypedContractMethod<[
        owner: AddressLike,
        spender: AddressLike
    ], [
        bigint
    ], "view">;
    getFunction(nameOrSignature: "approve"): TypedContractMethod<[
        spender: AddressLike,
        amount: BigNumberish
    ], [
        boolean
    ], "nonpayable">;
    getFunction(nameOrSignature: "balanceOf"): TypedContractMethod<[account: AddressLike], [bigint], "view">;
    filters: {};
}

type TransactionOptions = {
    gasLimit?: bigint;
    gasPrice?: bigint;
    maxFeePerGas?: bigint;
    maxPriorityFeePerGas?: bigint;
    nonce?: number;
    wait?: number;
};
declare const checkAllowance: (amount: bigint, spenderAddress: string, erc20Address: string, signer: Signer) => Promise<{
    allowance: bigint;
    isAllowable: boolean;
    erc20: ERC20;
}>;

declare const executeSwap: (params: ExecuteSwapParams, networkOpts: SwapNetworkOptions, txOpts: TransactionOptions) => Promise<ContractTransactionReceipt>;

declare const checkVaultAllowance: (params: Pick<TokenSwapParams, 'srcAsset' | 'amount'>, networkOpts: SwapNetworkOptions) => ReturnType<typeof checkAllowance>;
declare const approveVault: (params: Pick<TokenSwapParams, 'srcAsset' | 'amount'>, networkOpts: SwapNetworkOptions, txOpts: TransactionOptions) => Promise<ContractTransactionReceipt | null>;

type SwapNetworkOptions = {
    network: ChainflipNetwork;
    signer: Signer;
} | {
    network: 'localnet';
    signer: Signer;
    vaultContractAddress: string;
    srcTokenContractAddress: string;
};

declare const checkStateChainGatewayAllowance: (amount: bigint, networkOpts: FundingNetworkOptions) => ReturnType<typeof checkAllowance>;
declare const approveStateChainGateway: (amount: bigint, networkOpts: FundingNetworkOptions, txOpts: TransactionOptions) => Promise<ContractTransactionReceipt | null>;

type FundingNetworkOptions = {
    network: ChainflipNetwork;
    signer: Signer;
} | {
    network: 'localnet';
    signer: Signer;
    stateChainGatewayContractAddress: string;
    flipContractAddress: string;
};
declare const fundStateChainAccount: (accountId: `0x${string}`, amount: bigint, networkOpts: FundingNetworkOptions, txOpts: TransactionOptions) => Promise<ContractTransactionReceipt>;
declare const executeRedemption: (accountId: `0x${string}`, networkOpts: FundingNetworkOptions, txOpts: TransactionOptions) => Promise<ContractTransactionReceipt>;
declare const getMinimumFunding: (networkOpts: FundingNetworkOptions) => Promise<bigint>;
declare const getRedemptionDelay: (networkOpts: FundingNetworkOptions) => Promise<bigint>;

declare const ccmMetadataSchema: z.ZodObject<{
    gasBudget: z.ZodString;
    message: z.ZodEffects<z.ZodEffects<z.ZodString, `0x${string}`, string>, `0x${string}`, string>;
}, "strip", z.ZodTypeAny, {
    message: `0x${string}`;
    gasBudget: string;
}, {
    message: string;
    gasBudget: string;
}>;
type CcmMetadata = z.infer<typeof ccmMetadataSchema>;

type NewSwapRequest = {
    srcAsset: Asset;
    destAsset: Asset;
    srcChain: Chain;
    destChain: Chain;
    destAddress: string;
    ccmMetadata?: CcmMetadata;
};
declare const responseValidators: (network: ChainflipNetwork) => {
    requestSwapDepositAddress: z.ZodEffects<z.ZodObject<{
        address: z.ZodUnion<[z.ZodEffects<z.ZodEffects<z.ZodUnion<[z.ZodString, z.ZodEffects<z.ZodString, `0x${string}`, string>]>, string | null, string>, string, string>, z.ZodEffects<z.ZodString, `0x${string}`, string>, z.ZodEffects<z.ZodString, string, string> | z.ZodUnion<[z.ZodEffects<z.ZodString, string, string>, z.ZodEffects<z.ZodString, string, string>]>]>;
        issued_block: z.ZodNumber;
        channel_id: z.ZodNumber;
        expiry_block: z.ZodOptional<z.ZodNumber>;
        source_chain_expiry_block: z.ZodOptional<z.ZodUnion<[z.ZodEffects<z.ZodUnion<[z.ZodNumber, z.ZodString, z.ZodEffects<z.ZodString, `0x${string}`, string>]>, bigint, string | number>, z.ZodEffects<z.ZodNumber, bigint, number>]>>;
    }, "strip", z.ZodTypeAny, {
        address: string;
        issued_block: number;
        channel_id: number;
        expiry_block?: number | undefined;
        source_chain_expiry_block?: bigint | undefined;
    }, {
        address: string;
        issued_block: number;
        channel_id: number;
        expiry_block?: number | undefined;
        source_chain_expiry_block?: string | number | undefined;
    }>, {
        address: string;
        issuedBlock: number;
        channelId: bigint;
        sourceChainExpiryBlock: bigint | undefined;
    }, {
        address: string;
        issued_block: number;
        channel_id: number;
        expiry_block?: number | undefined;
        source_chain_expiry_block?: string | number | undefined;
    }>;
};
type ResponseValidator = ReturnType<typeof responseValidators>;
type DepositChannelResponse = z.infer<ResponseValidator['requestSwapDepositAddress']>;
declare function requestSwapDepositAddress(swapRequest: NewSwapRequest, opts: {
    url: string;
    commissionBps: number;
}, chainflipNetwork: ChainflipNetwork): Promise<DepositChannelResponse>;

type broker_DepositChannelResponse = DepositChannelResponse;
declare const broker_requestSwapDepositAddress: typeof requestSwapDepositAddress;
declare namespace broker {
  export { type broker_DepositChannelResponse as DepositChannelResponse, broker_requestSwapDepositAddress as requestSwapDepositAddress };
}

declare const broadcastParsers: {
    Ethereum: z.ZodObject<{
        tx_out_id: z.ZodObject<{
            signature: z.ZodObject<{
                k_times_g_address: z.ZodArray<z.ZodNumber, "many">;
                s: z.ZodArray<z.ZodNumber, "many">;
            }, "strip", z.ZodTypeAny, {
                s: number[];
                k_times_g_address: number[];
            }, {
                s: number[];
                k_times_g_address: number[];
            }>;
        }, "strip", z.ZodTypeAny, {
            signature: {
                s: number[];
                k_times_g_address: number[];
            };
        }, {
            signature: {
                s: number[];
                k_times_g_address: number[];
            };
        }>;
    }, "strip", z.ZodTypeAny, {
        tx_out_id: {
            signature: {
                s: number[];
                k_times_g_address: number[];
            };
        };
    }, {
        tx_out_id: {
            signature: {
                s: number[];
                k_times_g_address: number[];
            };
        };
    }>;
    Polkadot: z.ZodObject<{
        tx_out_id: z.ZodObject<{
            signature: z.ZodString;
        }, "strip", z.ZodTypeAny, {
            signature: string;
        }, {
            signature: string;
        }>;
    }, "strip", z.ZodTypeAny, {
        tx_out_id: {
            signature: string;
        };
    }, {
        tx_out_id: {
            signature: string;
        };
    }>;
    Bitcoin: z.ZodObject<{
        tx_out_id: z.ZodObject<{
            hash: z.ZodString;
        }, "strip", z.ZodTypeAny, {
            hash: string;
        }, {
            hash: string;
        }>;
    }, "strip", z.ZodTypeAny, {
        tx_out_id: {
            hash: string;
        };
    }, {
        tx_out_id: {
            hash: string;
        };
    }>;
    Arbitrum: z.ZodObject<{
        tx_out_id: z.ZodObject<{
            signature: z.ZodString;
        }, "strip", z.ZodTypeAny, {
            signature: string;
        }, {
            signature: string;
        }>;
    }, "strip", z.ZodTypeAny, {
        tx_out_id: {
            signature: string;
        };
    }, {
        tx_out_id: {
            signature: string;
        };
    }>;
};
type ChainBroadcast<C extends Chain> = z.infer<(typeof broadcastParsers)[C]>;
type EthereumBroadcast = ChainBroadcast<'Ethereum'>;
type PolkadotBroadcast = ChainBroadcast<'Polkadot'>;
type BitcoinBroadcast = ChainBroadcast<'Bitcoin'>;
type Broadcast = ChainBroadcast<Chain>;
declare class RedisClient {
    private client;
    constructor(url: `redis://${string}` | `rediss://${string}`);
    getBroadcast(chain: 'Ethereum', broadcastId: number | bigint): Promise<EthereumBroadcast | null>;
    getBroadcast(chain: 'Polkadot', broadcastId: number | bigint): Promise<PolkadotBroadcast | null>;
    getBroadcast(chain: 'Bitcoin', broadcastId: number | bigint): Promise<BitcoinBroadcast | null>;
    getBroadcast(chain: Chain, broadcastId: number | bigint): Promise<Broadcast | null>;
    getDeposits(chain: Chain, asset: Asset, address: string): Promise<{
        asset: string;
        amount: bigint;
        deposit_chain_block_height: number;
    }[]>;
    getMempoolTransaction(chain: 'Bitcoin', address: string): Promise<{
        value: bigint;
        confirmations: number;
        tx_hash: `0x${string}`;
    } | null>;
    quit(): Promise<"OK">;
}

export { type Asset, Assets, type Chain, type ChainflipNetwork, ChainflipNetworks, Chains, type ExecuteSwapParams, type FundingNetworkOptions, RedisClient, type SwapNetworkOptions, approveStateChainGateway, approveVault, assetChains, assetContractIds, assetDecimals, broker, chainAssets, chainContractIds, checkStateChainGatewayAllowance, checkVaultAllowance, executeRedemption, executeSwap, fundStateChainAccount, getMinimumFunding, getRedemptionDelay };
