/* eslint-disable */
import { TypedDocumentNode as DocumentNode } from '@graphql-typed-document-node/core';
export type Maybe<T> = T | null;
export type InputMaybe<T> = Maybe<T>;
export type Exact<T extends { [key: string]: unknown }> = { [K in keyof T]: T[K] };
export type MakeOptional<T, K extends keyof T> = Omit<T, K> & { [SubKey in K]?: Maybe<T[SubKey]> };
export type MakeMaybe<T, K extends keyof T> = Omit<T, K> & { [SubKey in K]: Maybe<T[SubKey]> };
export type MakeEmpty<T extends { [key: string]: unknown }, K extends keyof T> = { [_ in K]?: never };
export type Incremental<T> = T | { [P in keyof T]?: P extends ' $fragmentName' | '__typename' ? T[P] : never };
/** All built-in and custom scalars, mapped to their actual values */
export type Scalars = {
  ID: { input: string; output: string; }
  String: { input: string; output: string; }
  Boolean: { input: boolean; output: boolean; }
  Int: { input: number; output: number; }
  Float: { input: number; output: number; }
  /** A floating point number that requires more precision than IEEE 754 binary 64 */
  BigFloat: { input: any; output: any; }
  /** A location in a connection that can be used for resuming pagination. */
  Cursor: { input: any; output: any; }
  /**
   * A point in time as described by the [ISO
   * 8601](https://en.wikipedia.org/wiki/ISO_8601) standard. May or may not include a timezone.
   */
  Datetime: { input: any; output: any; }
  /** The `JSON` scalar type represents JSON values as specified by [ECMA-404](http://www.ecma-international.org/publications/files/ECMA-ST/ECMA-404.pdf). */
  JSON: { input: any; output: any; }
};

export type AcalaEvmExecuted = Node & {
  __typename?: 'AcalaEvmExecuted';
  contract: Scalars['String']['output'];
  /** Reads a single `Event` that is related to this `AcalaEvmExecuted`. */
  eventByEventId: Event;
  eventId: Scalars['String']['output'];
  /** A globally unique identifier. Can be used in various places throughout the system to identify this single value. */
  nodeId: Scalars['ID']['output'];
};

/**
 * A condition to be used against `AcalaEvmExecuted` object types. All fields are
 * tested for equality and combined with a logical ‘and.’
 */
export type AcalaEvmExecutedCondition = {
  /** Checks for equality with the object’s `contract` field. */
  contract?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `eventId` field. */
  eventId?: InputMaybe<Scalars['String']['input']>;
};

export type AcalaEvmExecutedFailed = Node & {
  __typename?: 'AcalaEvmExecutedFailed';
  contract: Scalars['String']['output'];
  /** Reads a single `Event` that is related to this `AcalaEvmExecutedFailed`. */
  eventByEventId: Event;
  eventId: Scalars['String']['output'];
  /** A globally unique identifier. Can be used in various places throughout the system to identify this single value. */
  nodeId: Scalars['ID']['output'];
};

/**
 * A condition to be used against `AcalaEvmExecutedFailed` object types. All fields
 * are tested for equality and combined with a logical ‘and.’
 */
export type AcalaEvmExecutedFailedCondition = {
  /** Checks for equality with the object’s `contract` field. */
  contract?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `eventId` field. */
  eventId?: InputMaybe<Scalars['String']['input']>;
};

/** A filter to be used against `AcalaEvmExecutedFailed` object types. All fields are combined with a logical ‘and.’ */
export type AcalaEvmExecutedFailedFilter = {
  /** Checks for all expressions in this list. */
  and?: InputMaybe<Array<AcalaEvmExecutedFailedFilter>>;
  /** Filter by the object’s `contract` field. */
  contract?: InputMaybe<StringFilter>;
  /** Filter by the object’s `eventId` field. */
  eventId?: InputMaybe<StringFilter>;
  /** Negates the expression. */
  not?: InputMaybe<AcalaEvmExecutedFailedFilter>;
  /** Checks for any expressions in this list. */
  or?: InputMaybe<Array<AcalaEvmExecutedFailedFilter>>;
};

export type AcalaEvmExecutedFailedLog = Node & {
  __typename?: 'AcalaEvmExecutedFailedLog';
  contract: Scalars['String']['output'];
  /** Reads a single `Event` that is related to this `AcalaEvmExecutedFailedLog`. */
  eventByEventId: Event;
  eventContract: Scalars['String']['output'];
  eventId: Scalars['String']['output'];
  id: Scalars['String']['output'];
  /** A globally unique identifier. Can be used in various places throughout the system to identify this single value. */
  nodeId: Scalars['ID']['output'];
  topic0?: Maybe<Scalars['String']['output']>;
  topic1?: Maybe<Scalars['String']['output']>;
  topic2?: Maybe<Scalars['String']['output']>;
  topic3?: Maybe<Scalars['String']['output']>;
};

/**
 * A condition to be used against `AcalaEvmExecutedFailedLog` object types. All
 * fields are tested for equality and combined with a logical ‘and.’
 */
export type AcalaEvmExecutedFailedLogCondition = {
  /** Checks for equality with the object’s `contract` field. */
  contract?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `eventContract` field. */
  eventContract?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `eventId` field. */
  eventId?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `id` field. */
  id?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `topic0` field. */
  topic0?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `topic1` field. */
  topic1?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `topic2` field. */
  topic2?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `topic3` field. */
  topic3?: InputMaybe<Scalars['String']['input']>;
};

/** A filter to be used against `AcalaEvmExecutedFailedLog` object types. All fields are combined with a logical ‘and.’ */
export type AcalaEvmExecutedFailedLogFilter = {
  /** Checks for all expressions in this list. */
  and?: InputMaybe<Array<AcalaEvmExecutedFailedLogFilter>>;
  /** Filter by the object’s `contract` field. */
  contract?: InputMaybe<StringFilter>;
  /** Filter by the object’s `eventContract` field. */
  eventContract?: InputMaybe<StringFilter>;
  /** Filter by the object’s `eventId` field. */
  eventId?: InputMaybe<StringFilter>;
  /** Filter by the object’s `id` field. */
  id?: InputMaybe<StringFilter>;
  /** Negates the expression. */
  not?: InputMaybe<AcalaEvmExecutedFailedLogFilter>;
  /** Checks for any expressions in this list. */
  or?: InputMaybe<Array<AcalaEvmExecutedFailedLogFilter>>;
  /** Filter by the object’s `topic0` field. */
  topic0?: InputMaybe<StringFilter>;
  /** Filter by the object’s `topic1` field. */
  topic1?: InputMaybe<StringFilter>;
  /** Filter by the object’s `topic2` field. */
  topic2?: InputMaybe<StringFilter>;
  /** Filter by the object’s `topic3` field. */
  topic3?: InputMaybe<StringFilter>;
};

/** A connection to a list of `AcalaEvmExecutedFailedLog` values. */
export type AcalaEvmExecutedFailedLogsConnection = {
  __typename?: 'AcalaEvmExecutedFailedLogsConnection';
  /** A list of edges which contains the `AcalaEvmExecutedFailedLog` and cursor to aid in pagination. */
  edges: Array<AcalaEvmExecutedFailedLogsEdge>;
  /** A list of `AcalaEvmExecutedFailedLog` objects. */
  nodes: Array<AcalaEvmExecutedFailedLog>;
  /** Information to aid in pagination. */
  pageInfo: PageInfo;
  /** The count of *all* `AcalaEvmExecutedFailedLog` you could get from the connection. */
  totalCount: Scalars['Int']['output'];
};

/** A `AcalaEvmExecutedFailedLog` edge in the connection. */
export type AcalaEvmExecutedFailedLogsEdge = {
  __typename?: 'AcalaEvmExecutedFailedLogsEdge';
  /** A cursor for use in pagination. */
  cursor?: Maybe<Scalars['Cursor']['output']>;
  /** The `AcalaEvmExecutedFailedLog` at the end of the edge. */
  node: AcalaEvmExecutedFailedLog;
};

/** Methods to use when ordering `AcalaEvmExecutedFailedLog`. */
export type AcalaEvmExecutedFailedLogsOrderBy =
  | 'CONTRACT_ASC'
  | 'CONTRACT_DESC'
  | 'EVENT_CONTRACT_ASC'
  | 'EVENT_CONTRACT_DESC'
  | 'EVENT_ID_ASC'
  | 'EVENT_ID_DESC'
  | 'ID_ASC'
  | 'ID_DESC'
  | 'NATURAL'
  | 'PRIMARY_KEY_ASC'
  | 'PRIMARY_KEY_DESC'
  | 'TOPIC0_ASC'
  | 'TOPIC0_DESC'
  | 'TOPIC1_ASC'
  | 'TOPIC1_DESC'
  | 'TOPIC2_ASC'
  | 'TOPIC2_DESC'
  | 'TOPIC3_ASC'
  | 'TOPIC3_DESC';

/** A connection to a list of `AcalaEvmExecutedFailed` values. */
export type AcalaEvmExecutedFailedsConnection = {
  __typename?: 'AcalaEvmExecutedFailedsConnection';
  /** A list of edges which contains the `AcalaEvmExecutedFailed` and cursor to aid in pagination. */
  edges: Array<AcalaEvmExecutedFailedsEdge>;
  /** A list of `AcalaEvmExecutedFailed` objects. */
  nodes: Array<AcalaEvmExecutedFailed>;
  /** Information to aid in pagination. */
  pageInfo: PageInfo;
  /** The count of *all* `AcalaEvmExecutedFailed` you could get from the connection. */
  totalCount: Scalars['Int']['output'];
};

/** A `AcalaEvmExecutedFailed` edge in the connection. */
export type AcalaEvmExecutedFailedsEdge = {
  __typename?: 'AcalaEvmExecutedFailedsEdge';
  /** A cursor for use in pagination. */
  cursor?: Maybe<Scalars['Cursor']['output']>;
  /** The `AcalaEvmExecutedFailed` at the end of the edge. */
  node: AcalaEvmExecutedFailed;
};

/** Methods to use when ordering `AcalaEvmExecutedFailed`. */
export type AcalaEvmExecutedFailedsOrderBy =
  | 'CONTRACT_ASC'
  | 'CONTRACT_DESC'
  | 'EVENT_ID_ASC'
  | 'EVENT_ID_DESC'
  | 'NATURAL'
  | 'PRIMARY_KEY_ASC'
  | 'PRIMARY_KEY_DESC';

/** A filter to be used against `AcalaEvmExecuted` object types. All fields are combined with a logical ‘and.’ */
export type AcalaEvmExecutedFilter = {
  /** Checks for all expressions in this list. */
  and?: InputMaybe<Array<AcalaEvmExecutedFilter>>;
  /** Filter by the object’s `contract` field. */
  contract?: InputMaybe<StringFilter>;
  /** Filter by the object’s `eventId` field. */
  eventId?: InputMaybe<StringFilter>;
  /** Negates the expression. */
  not?: InputMaybe<AcalaEvmExecutedFilter>;
  /** Checks for any expressions in this list. */
  or?: InputMaybe<Array<AcalaEvmExecutedFilter>>;
};

export type AcalaEvmExecutedLog = Node & {
  __typename?: 'AcalaEvmExecutedLog';
  contract: Scalars['String']['output'];
  /** Reads a single `Event` that is related to this `AcalaEvmExecutedLog`. */
  eventByEventId: Event;
  eventContract: Scalars['String']['output'];
  eventId: Scalars['String']['output'];
  id: Scalars['String']['output'];
  /** A globally unique identifier. Can be used in various places throughout the system to identify this single value. */
  nodeId: Scalars['ID']['output'];
  topic0?: Maybe<Scalars['String']['output']>;
  topic1?: Maybe<Scalars['String']['output']>;
  topic2?: Maybe<Scalars['String']['output']>;
  topic3?: Maybe<Scalars['String']['output']>;
};

/**
 * A condition to be used against `AcalaEvmExecutedLog` object types. All fields
 * are tested for equality and combined with a logical ‘and.’
 */
export type AcalaEvmExecutedLogCondition = {
  /** Checks for equality with the object’s `contract` field. */
  contract?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `eventContract` field. */
  eventContract?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `eventId` field. */
  eventId?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `id` field. */
  id?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `topic0` field. */
  topic0?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `topic1` field. */
  topic1?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `topic2` field. */
  topic2?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `topic3` field. */
  topic3?: InputMaybe<Scalars['String']['input']>;
};

/** A filter to be used against `AcalaEvmExecutedLog` object types. All fields are combined with a logical ‘and.’ */
export type AcalaEvmExecutedLogFilter = {
  /** Checks for all expressions in this list. */
  and?: InputMaybe<Array<AcalaEvmExecutedLogFilter>>;
  /** Filter by the object’s `contract` field. */
  contract?: InputMaybe<StringFilter>;
  /** Filter by the object’s `eventContract` field. */
  eventContract?: InputMaybe<StringFilter>;
  /** Filter by the object’s `eventId` field. */
  eventId?: InputMaybe<StringFilter>;
  /** Filter by the object’s `id` field. */
  id?: InputMaybe<StringFilter>;
  /** Negates the expression. */
  not?: InputMaybe<AcalaEvmExecutedLogFilter>;
  /** Checks for any expressions in this list. */
  or?: InputMaybe<Array<AcalaEvmExecutedLogFilter>>;
  /** Filter by the object’s `topic0` field. */
  topic0?: InputMaybe<StringFilter>;
  /** Filter by the object’s `topic1` field. */
  topic1?: InputMaybe<StringFilter>;
  /** Filter by the object’s `topic2` field. */
  topic2?: InputMaybe<StringFilter>;
  /** Filter by the object’s `topic3` field. */
  topic3?: InputMaybe<StringFilter>;
};

/** A connection to a list of `AcalaEvmExecutedLog` values. */
export type AcalaEvmExecutedLogsConnection = {
  __typename?: 'AcalaEvmExecutedLogsConnection';
  /** A list of edges which contains the `AcalaEvmExecutedLog` and cursor to aid in pagination. */
  edges: Array<AcalaEvmExecutedLogsEdge>;
  /** A list of `AcalaEvmExecutedLog` objects. */
  nodes: Array<AcalaEvmExecutedLog>;
  /** Information to aid in pagination. */
  pageInfo: PageInfo;
  /** The count of *all* `AcalaEvmExecutedLog` you could get from the connection. */
  totalCount: Scalars['Int']['output'];
};

/** A `AcalaEvmExecutedLog` edge in the connection. */
export type AcalaEvmExecutedLogsEdge = {
  __typename?: 'AcalaEvmExecutedLogsEdge';
  /** A cursor for use in pagination. */
  cursor?: Maybe<Scalars['Cursor']['output']>;
  /** The `AcalaEvmExecutedLog` at the end of the edge. */
  node: AcalaEvmExecutedLog;
};

/** Methods to use when ordering `AcalaEvmExecutedLog`. */
export type AcalaEvmExecutedLogsOrderBy =
  | 'CONTRACT_ASC'
  | 'CONTRACT_DESC'
  | 'EVENT_CONTRACT_ASC'
  | 'EVENT_CONTRACT_DESC'
  | 'EVENT_ID_ASC'
  | 'EVENT_ID_DESC'
  | 'ID_ASC'
  | 'ID_DESC'
  | 'NATURAL'
  | 'PRIMARY_KEY_ASC'
  | 'PRIMARY_KEY_DESC'
  | 'TOPIC0_ASC'
  | 'TOPIC0_DESC'
  | 'TOPIC1_ASC'
  | 'TOPIC1_DESC'
  | 'TOPIC2_ASC'
  | 'TOPIC2_DESC'
  | 'TOPIC3_ASC'
  | 'TOPIC3_DESC';

/** A connection to a list of `AcalaEvmExecuted` values. */
export type AcalaEvmExecutedsConnection = {
  __typename?: 'AcalaEvmExecutedsConnection';
  /** A list of edges which contains the `AcalaEvmExecuted` and cursor to aid in pagination. */
  edges: Array<AcalaEvmExecutedsEdge>;
  /** A list of `AcalaEvmExecuted` objects. */
  nodes: Array<AcalaEvmExecuted>;
  /** Information to aid in pagination. */
  pageInfo: PageInfo;
  /** The count of *all* `AcalaEvmExecuted` you could get from the connection. */
  totalCount: Scalars['Int']['output'];
};

/** A `AcalaEvmExecuted` edge in the connection. */
export type AcalaEvmExecutedsEdge = {
  __typename?: 'AcalaEvmExecutedsEdge';
  /** A cursor for use in pagination. */
  cursor?: Maybe<Scalars['Cursor']['output']>;
  /** The `AcalaEvmExecuted` at the end of the edge. */
  node: AcalaEvmExecuted;
};

/** Methods to use when ordering `AcalaEvmExecuted`. */
export type AcalaEvmExecutedsOrderBy =
  | 'CONTRACT_ASC'
  | 'CONTRACT_DESC'
  | 'EVENT_ID_ASC'
  | 'EVENT_ID_DESC'
  | 'NATURAL'
  | 'PRIMARY_KEY_ASC'
  | 'PRIMARY_KEY_DESC';

/** A filter to be used against BigFloat fields. All fields are combined with a logical ‘and.’ */
export type BigFloatFilter = {
  /** Not equal to the specified value, treating null like an ordinary value. */
  distinctFrom?: InputMaybe<Scalars['BigFloat']['input']>;
  /** Equal to the specified value. */
  equalTo?: InputMaybe<Scalars['BigFloat']['input']>;
  /** Greater than the specified value. */
  greaterThan?: InputMaybe<Scalars['BigFloat']['input']>;
  /** Greater than or equal to the specified value. */
  greaterThanOrEqualTo?: InputMaybe<Scalars['BigFloat']['input']>;
  /** Included in the specified list. */
  in?: InputMaybe<Array<Scalars['BigFloat']['input']>>;
  /** Is null (if `true` is specified) or is not null (if `false` is specified). */
  isNull?: InputMaybe<Scalars['Boolean']['input']>;
  /** Less than the specified value. */
  lessThan?: InputMaybe<Scalars['BigFloat']['input']>;
  /** Less than or equal to the specified value. */
  lessThanOrEqualTo?: InputMaybe<Scalars['BigFloat']['input']>;
  /** Equal to the specified value, treating null like an ordinary value. */
  notDistinctFrom?: InputMaybe<Scalars['BigFloat']['input']>;
  /** Not equal to the specified value. */
  notEqualTo?: InputMaybe<Scalars['BigFloat']['input']>;
  /** Not included in the specified list. */
  notIn?: InputMaybe<Array<Scalars['BigFloat']['input']>>;
};

export type Block = Node & {
  __typename?: 'Block';
  /** Reads and enables pagination through a set of `Call`. */
  callsByBlockId: CallsConnection;
  /** Reads and enables pagination through a set of `Event`. */
  eventsByBlockId: EventsConnection;
  /** Reads and enables pagination through a set of `Extrinsic`. */
  extrinsicsByBlockId: ExtrinsicsConnection;
  extrinsicsRoot: Scalars['String']['output'];
  hash: Scalars['String']['output'];
  height: Scalars['Int']['output'];
  id: Scalars['String']['output'];
  /** A globally unique identifier. Can be used in various places throughout the system to identify this single value. */
  nodeId: Scalars['ID']['output'];
  parentHash: Scalars['String']['output'];
  specId: Scalars['String']['output'];
  stateRoot: Scalars['String']['output'];
  timestamp: Scalars['Datetime']['output'];
  validator?: Maybe<Scalars['String']['output']>;
};


export type BlockCallsByBlockIdArgs = {
  after?: InputMaybe<Scalars['Cursor']['input']>;
  before?: InputMaybe<Scalars['Cursor']['input']>;
  condition?: InputMaybe<CallCondition>;
  filter?: InputMaybe<CallFilter>;
  first?: InputMaybe<Scalars['Int']['input']>;
  last?: InputMaybe<Scalars['Int']['input']>;
  offset?: InputMaybe<Scalars['Int']['input']>;
  orderBy?: InputMaybe<Array<CallsOrderBy>>;
};


export type BlockEventsByBlockIdArgs = {
  after?: InputMaybe<Scalars['Cursor']['input']>;
  before?: InputMaybe<Scalars['Cursor']['input']>;
  condition?: InputMaybe<EventCondition>;
  filter?: InputMaybe<EventFilter>;
  first?: InputMaybe<Scalars['Int']['input']>;
  last?: InputMaybe<Scalars['Int']['input']>;
  offset?: InputMaybe<Scalars['Int']['input']>;
  orderBy?: InputMaybe<Array<EventsOrderBy>>;
};


export type BlockExtrinsicsByBlockIdArgs = {
  after?: InputMaybe<Scalars['Cursor']['input']>;
  before?: InputMaybe<Scalars['Cursor']['input']>;
  condition?: InputMaybe<ExtrinsicCondition>;
  filter?: InputMaybe<ExtrinsicFilter>;
  first?: InputMaybe<Scalars['Int']['input']>;
  last?: InputMaybe<Scalars['Int']['input']>;
  offset?: InputMaybe<Scalars['Int']['input']>;
  orderBy?: InputMaybe<Array<ExtrinsicsOrderBy>>;
};

/** A condition to be used against `Block` object types. All fields are tested for equality and combined with a logical ‘and.’ */
export type BlockCondition = {
  /** Checks for equality with the object’s `extrinsicsRoot` field. */
  extrinsicsRoot?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `hash` field. */
  hash?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `height` field. */
  height?: InputMaybe<Scalars['Int']['input']>;
  /** Checks for equality with the object’s `id` field. */
  id?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `parentHash` field. */
  parentHash?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `specId` field. */
  specId?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `stateRoot` field. */
  stateRoot?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `timestamp` field. */
  timestamp?: InputMaybe<Scalars['Datetime']['input']>;
  /** Checks for equality with the object’s `validator` field. */
  validator?: InputMaybe<Scalars['String']['input']>;
};

/** A filter to be used against `Block` object types. All fields are combined with a logical ‘and.’ */
export type BlockFilter = {
  /** Checks for all expressions in this list. */
  and?: InputMaybe<Array<BlockFilter>>;
  /** Filter by the object’s `extrinsicsRoot` field. */
  extrinsicsRoot?: InputMaybe<StringFilter>;
  /** Filter by the object’s `hash` field. */
  hash?: InputMaybe<StringFilter>;
  /** Filter by the object’s `height` field. */
  height?: InputMaybe<IntFilter>;
  /** Filter by the object’s `id` field. */
  id?: InputMaybe<StringFilter>;
  /** Negates the expression. */
  not?: InputMaybe<BlockFilter>;
  /** Checks for any expressions in this list. */
  or?: InputMaybe<Array<BlockFilter>>;
  /** Filter by the object’s `parentHash` field. */
  parentHash?: InputMaybe<StringFilter>;
  /** Filter by the object’s `specId` field. */
  specId?: InputMaybe<StringFilter>;
  /** Filter by the object’s `stateRoot` field. */
  stateRoot?: InputMaybe<StringFilter>;
  /** Filter by the object’s `timestamp` field. */
  timestamp?: InputMaybe<DatetimeFilter>;
  /** Filter by the object’s `validator` field. */
  validator?: InputMaybe<StringFilter>;
};

/** A connection to a list of `Block` values. */
export type BlocksConnection = {
  __typename?: 'BlocksConnection';
  /** A list of edges which contains the `Block` and cursor to aid in pagination. */
  edges: Array<BlocksEdge>;
  /** A list of `Block` objects. */
  nodes: Array<Block>;
  /** Information to aid in pagination. */
  pageInfo: PageInfo;
  /** The count of *all* `Block` you could get from the connection. */
  totalCount: Scalars['Int']['output'];
};

/** A `Block` edge in the connection. */
export type BlocksEdge = {
  __typename?: 'BlocksEdge';
  /** A cursor for use in pagination. */
  cursor?: Maybe<Scalars['Cursor']['output']>;
  /** The `Block` at the end of the edge. */
  node: Block;
};

/** Methods to use when ordering `Block`. */
export type BlocksOrderBy =
  | 'EXTRINSICS_ROOT_ASC'
  | 'EXTRINSICS_ROOT_DESC'
  | 'HASH_ASC'
  | 'HASH_DESC'
  | 'HEIGHT_ASC'
  | 'HEIGHT_DESC'
  | 'ID_ASC'
  | 'ID_DESC'
  | 'NATURAL'
  | 'PARENT_HASH_ASC'
  | 'PARENT_HASH_DESC'
  | 'PRIMARY_KEY_ASC'
  | 'PRIMARY_KEY_DESC'
  | 'SPEC_ID_ASC'
  | 'SPEC_ID_DESC'
  | 'STATE_ROOT_ASC'
  | 'STATE_ROOT_DESC'
  | 'TIMESTAMP_ASC'
  | 'TIMESTAMP_DESC'
  | 'VALIDATOR_ASC'
  | 'VALIDATOR_DESC';

/** A filter to be used against Boolean fields. All fields are combined with a logical ‘and.’ */
export type BooleanFilter = {
  /** Not equal to the specified value, treating null like an ordinary value. */
  distinctFrom?: InputMaybe<Scalars['Boolean']['input']>;
  /** Equal to the specified value. */
  equalTo?: InputMaybe<Scalars['Boolean']['input']>;
  /** Greater than the specified value. */
  greaterThan?: InputMaybe<Scalars['Boolean']['input']>;
  /** Greater than or equal to the specified value. */
  greaterThanOrEqualTo?: InputMaybe<Scalars['Boolean']['input']>;
  /** Included in the specified list. */
  in?: InputMaybe<Array<Scalars['Boolean']['input']>>;
  /** Is null (if `true` is specified) or is not null (if `false` is specified). */
  isNull?: InputMaybe<Scalars['Boolean']['input']>;
  /** Less than the specified value. */
  lessThan?: InputMaybe<Scalars['Boolean']['input']>;
  /** Less than or equal to the specified value. */
  lessThanOrEqualTo?: InputMaybe<Scalars['Boolean']['input']>;
  /** Equal to the specified value, treating null like an ordinary value. */
  notDistinctFrom?: InputMaybe<Scalars['Boolean']['input']>;
  /** Not equal to the specified value. */
  notEqualTo?: InputMaybe<Scalars['Boolean']['input']>;
  /** Not included in the specified list. */
  notIn?: InputMaybe<Array<Scalars['Boolean']['input']>>;
};

export type Call = Node & {
  __typename?: 'Call';
  args?: Maybe<Scalars['JSON']['output']>;
  /** Reads a single `Block` that is related to this `Call`. */
  blockByBlockId: Block;
  blockId: Scalars['String']['output'];
  /** Reads a single `Call` that is related to this `Call`. */
  callByParentId?: Maybe<Call>;
  /** Reads and enables pagination through a set of `Call`. */
  callsByParentId: CallsConnection;
  error?: Maybe<Scalars['JSON']['output']>;
  /** Reads and enables pagination through a set of `Event`. */
  eventsByCallId: EventsConnection;
  /** Reads a single `Extrinsic` that is related to this `Call`. */
  extrinsicByExtrinsicId: Extrinsic;
  extrinsicId: Scalars['String']['output'];
  /** Reads a single `FrontierEthereumTransaction` that is related to this `Call`. */
  frontierEthereumTransactionByCallId?: Maybe<FrontierEthereumTransaction>;
  /**
   * Reads and enables pagination through a set of `FrontierEthereumTransaction`.
   * @deprecated Please use frontierEthereumTransactionByCallId instead
   */
  frontierEthereumTransactionsByCallId: FrontierEthereumTransactionsConnection;
  id: Scalars['String']['output'];
  name: Scalars['String']['output'];
  /** A globally unique identifier. Can be used in various places throughout the system to identify this single value. */
  nodeId: Scalars['ID']['output'];
  origin?: Maybe<Scalars['JSON']['output']>;
  parentId?: Maybe<Scalars['String']['output']>;
  pos: Scalars['Int']['output'];
  success: Scalars['Boolean']['output'];
};


export type CallCallsByParentIdArgs = {
  after?: InputMaybe<Scalars['Cursor']['input']>;
  before?: InputMaybe<Scalars['Cursor']['input']>;
  condition?: InputMaybe<CallCondition>;
  filter?: InputMaybe<CallFilter>;
  first?: InputMaybe<Scalars['Int']['input']>;
  last?: InputMaybe<Scalars['Int']['input']>;
  offset?: InputMaybe<Scalars['Int']['input']>;
  orderBy?: InputMaybe<Array<CallsOrderBy>>;
};


export type CallEventsByCallIdArgs = {
  after?: InputMaybe<Scalars['Cursor']['input']>;
  before?: InputMaybe<Scalars['Cursor']['input']>;
  condition?: InputMaybe<EventCondition>;
  filter?: InputMaybe<EventFilter>;
  first?: InputMaybe<Scalars['Int']['input']>;
  last?: InputMaybe<Scalars['Int']['input']>;
  offset?: InputMaybe<Scalars['Int']['input']>;
  orderBy?: InputMaybe<Array<EventsOrderBy>>;
};


export type CallFrontierEthereumTransactionsByCallIdArgs = {
  after?: InputMaybe<Scalars['Cursor']['input']>;
  before?: InputMaybe<Scalars['Cursor']['input']>;
  condition?: InputMaybe<FrontierEthereumTransactionCondition>;
  filter?: InputMaybe<FrontierEthereumTransactionFilter>;
  first?: InputMaybe<Scalars['Int']['input']>;
  last?: InputMaybe<Scalars['Int']['input']>;
  offset?: InputMaybe<Scalars['Int']['input']>;
  orderBy?: InputMaybe<Array<FrontierEthereumTransactionsOrderBy>>;
};

/** A condition to be used against `Call` object types. All fields are tested for equality and combined with a logical ‘and.’ */
export type CallCondition = {
  /** Checks for equality with the object’s `args` field. */
  args?: InputMaybe<Scalars['JSON']['input']>;
  /** Checks for equality with the object’s `blockId` field. */
  blockId?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `error` field. */
  error?: InputMaybe<Scalars['JSON']['input']>;
  /** Checks for equality with the object’s `extrinsicId` field. */
  extrinsicId?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `id` field. */
  id?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `name` field. */
  name?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `origin` field. */
  origin?: InputMaybe<Scalars['JSON']['input']>;
  /** Checks for equality with the object’s `parentId` field. */
  parentId?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `pos` field. */
  pos?: InputMaybe<Scalars['Int']['input']>;
  /** Checks for equality with the object’s `success` field. */
  success?: InputMaybe<Scalars['Boolean']['input']>;
};

/** A filter to be used against `Call` object types. All fields are combined with a logical ‘and.’ */
export type CallFilter = {
  /** Checks for all expressions in this list. */
  and?: InputMaybe<Array<CallFilter>>;
  /** Filter by the object’s `args` field. */
  args?: InputMaybe<JsonFilter>;
  /** Filter by the object’s `blockId` field. */
  blockId?: InputMaybe<StringFilter>;
  /** Filter by the object’s `error` field. */
  error?: InputMaybe<JsonFilter>;
  /** Filter by the object’s `extrinsicId` field. */
  extrinsicId?: InputMaybe<StringFilter>;
  /** Filter by the object’s `id` field. */
  id?: InputMaybe<StringFilter>;
  /** Filter by the object’s `name` field. */
  name?: InputMaybe<StringFilter>;
  /** Negates the expression. */
  not?: InputMaybe<CallFilter>;
  /** Checks for any expressions in this list. */
  or?: InputMaybe<Array<CallFilter>>;
  /** Filter by the object’s `origin` field. */
  origin?: InputMaybe<JsonFilter>;
  /** Filter by the object’s `parentId` field. */
  parentId?: InputMaybe<StringFilter>;
  /** Filter by the object’s `pos` field. */
  pos?: InputMaybe<IntFilter>;
  /** Filter by the object’s `success` field. */
  success?: InputMaybe<BooleanFilter>;
};

/** A connection to a list of `Call` values. */
export type CallsConnection = {
  __typename?: 'CallsConnection';
  /** A list of edges which contains the `Call` and cursor to aid in pagination. */
  edges: Array<CallsEdge>;
  /** A list of `Call` objects. */
  nodes: Array<Call>;
  /** Information to aid in pagination. */
  pageInfo: PageInfo;
  /** The count of *all* `Call` you could get from the connection. */
  totalCount: Scalars['Int']['output'];
};

/** A `Call` edge in the connection. */
export type CallsEdge = {
  __typename?: 'CallsEdge';
  /** A cursor for use in pagination. */
  cursor?: Maybe<Scalars['Cursor']['output']>;
  /** The `Call` at the end of the edge. */
  node: Call;
};

/** Methods to use when ordering `Call`. */
export type CallsOrderBy =
  | 'ARGS_ASC'
  | 'ARGS_DESC'
  | 'BLOCK_ID_ASC'
  | 'BLOCK_ID_DESC'
  | 'ERROR_ASC'
  | 'ERROR_DESC'
  | 'EXTRINSIC_ID_ASC'
  | 'EXTRINSIC_ID_DESC'
  | 'ID_ASC'
  | 'ID_DESC'
  | 'NAME_ASC'
  | 'NAME_DESC'
  | 'NATURAL'
  | 'ORIGIN_ASC'
  | 'ORIGIN_DESC'
  | 'PARENT_ID_ASC'
  | 'PARENT_ID_DESC'
  | 'POS_ASC'
  | 'POS_DESC'
  | 'PRIMARY_KEY_ASC'
  | 'PRIMARY_KEY_DESC'
  | 'SUCCESS_ASC'
  | 'SUCCESS_DESC';

export type ContractsContractEmitted = Node & {
  __typename?: 'ContractsContractEmitted';
  contract: Scalars['String']['output'];
  /** Reads a single `Event` that is related to this `ContractsContractEmitted`. */
  eventByEventId: Event;
  eventId: Scalars['String']['output'];
  /** A globally unique identifier. Can be used in various places throughout the system to identify this single value. */
  nodeId: Scalars['ID']['output'];
};

/**
 * A condition to be used against `ContractsContractEmitted` object types. All
 * fields are tested for equality and combined with a logical ‘and.’
 */
export type ContractsContractEmittedCondition = {
  /** Checks for equality with the object’s `contract` field. */
  contract?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `eventId` field. */
  eventId?: InputMaybe<Scalars['String']['input']>;
};

/** A filter to be used against `ContractsContractEmitted` object types. All fields are combined with a logical ‘and.’ */
export type ContractsContractEmittedFilter = {
  /** Checks for all expressions in this list. */
  and?: InputMaybe<Array<ContractsContractEmittedFilter>>;
  /** Filter by the object’s `contract` field. */
  contract?: InputMaybe<StringFilter>;
  /** Filter by the object’s `eventId` field. */
  eventId?: InputMaybe<StringFilter>;
  /** Negates the expression. */
  not?: InputMaybe<ContractsContractEmittedFilter>;
  /** Checks for any expressions in this list. */
  or?: InputMaybe<Array<ContractsContractEmittedFilter>>;
};

/** A connection to a list of `ContractsContractEmitted` values. */
export type ContractsContractEmittedsConnection = {
  __typename?: 'ContractsContractEmittedsConnection';
  /** A list of edges which contains the `ContractsContractEmitted` and cursor to aid in pagination. */
  edges: Array<ContractsContractEmittedsEdge>;
  /** A list of `ContractsContractEmitted` objects. */
  nodes: Array<ContractsContractEmitted>;
  /** Information to aid in pagination. */
  pageInfo: PageInfo;
  /** The count of *all* `ContractsContractEmitted` you could get from the connection. */
  totalCount: Scalars['Int']['output'];
};

/** A `ContractsContractEmitted` edge in the connection. */
export type ContractsContractEmittedsEdge = {
  __typename?: 'ContractsContractEmittedsEdge';
  /** A cursor for use in pagination. */
  cursor?: Maybe<Scalars['Cursor']['output']>;
  /** The `ContractsContractEmitted` at the end of the edge. */
  node: ContractsContractEmitted;
};

/** Methods to use when ordering `ContractsContractEmitted`. */
export type ContractsContractEmittedsOrderBy =
  | 'CONTRACT_ASC'
  | 'CONTRACT_DESC'
  | 'EVENT_ID_ASC'
  | 'EVENT_ID_DESC'
  | 'NATURAL'
  | 'PRIMARY_KEY_ASC'
  | 'PRIMARY_KEY_DESC';

/** A filter to be used against Datetime fields. All fields are combined with a logical ‘and.’ */
export type DatetimeFilter = {
  /** Not equal to the specified value, treating null like an ordinary value. */
  distinctFrom?: InputMaybe<Scalars['Datetime']['input']>;
  /** Equal to the specified value. */
  equalTo?: InputMaybe<Scalars['Datetime']['input']>;
  /** Greater than the specified value. */
  greaterThan?: InputMaybe<Scalars['Datetime']['input']>;
  /** Greater than or equal to the specified value. */
  greaterThanOrEqualTo?: InputMaybe<Scalars['Datetime']['input']>;
  /** Included in the specified list. */
  in?: InputMaybe<Array<Scalars['Datetime']['input']>>;
  /** Is null (if `true` is specified) or is not null (if `false` is specified). */
  isNull?: InputMaybe<Scalars['Boolean']['input']>;
  /** Less than the specified value. */
  lessThan?: InputMaybe<Scalars['Datetime']['input']>;
  /** Less than or equal to the specified value. */
  lessThanOrEqualTo?: InputMaybe<Scalars['Datetime']['input']>;
  /** Equal to the specified value, treating null like an ordinary value. */
  notDistinctFrom?: InputMaybe<Scalars['Datetime']['input']>;
  /** Not equal to the specified value. */
  notEqualTo?: InputMaybe<Scalars['Datetime']['input']>;
  /** Not included in the specified list. */
  notIn?: InputMaybe<Array<Scalars['Datetime']['input']>>;
};

export type Event = Node & {
  __typename?: 'Event';
  /** Reads a single `AcalaEvmExecuted` that is related to this `Event`. */
  acalaEvmExecutedByEventId?: Maybe<AcalaEvmExecuted>;
  /** Reads a single `AcalaEvmExecutedFailed` that is related to this `Event`. */
  acalaEvmExecutedFailedByEventId?: Maybe<AcalaEvmExecutedFailed>;
  /** Reads and enables pagination through a set of `AcalaEvmExecutedFailedLog`. */
  acalaEvmExecutedFailedLogsByEventId: AcalaEvmExecutedFailedLogsConnection;
  /**
   * Reads and enables pagination through a set of `AcalaEvmExecutedFailed`.
   * @deprecated Please use acalaEvmExecutedFailedByEventId instead
   */
  acalaEvmExecutedFailedsByEventId: AcalaEvmExecutedFailedsConnection;
  /** Reads and enables pagination through a set of `AcalaEvmExecutedLog`. */
  acalaEvmExecutedLogsByEventId: AcalaEvmExecutedLogsConnection;
  /**
   * Reads and enables pagination through a set of `AcalaEvmExecuted`.
   * @deprecated Please use acalaEvmExecutedByEventId instead
   */
  acalaEvmExecutedsByEventId: AcalaEvmExecutedsConnection;
  args?: Maybe<Scalars['JSON']['output']>;
  /** Reads a single `Block` that is related to this `Event`. */
  blockByBlockId: Block;
  blockId: Scalars['String']['output'];
  /** Reads a single `Call` that is related to this `Event`. */
  callByCallId?: Maybe<Call>;
  callId?: Maybe<Scalars['String']['output']>;
  /** Reads a single `ContractsContractEmitted` that is related to this `Event`. */
  contractsContractEmittedByEventId?: Maybe<ContractsContractEmitted>;
  /**
   * Reads and enables pagination through a set of `ContractsContractEmitted`.
   * @deprecated Please use contractsContractEmittedByEventId instead
   */
  contractsContractEmittedsByEventId: ContractsContractEmittedsConnection;
  /** Reads a single `Extrinsic` that is related to this `Event`. */
  extrinsicByExtrinsicId?: Maybe<Extrinsic>;
  extrinsicId?: Maybe<Scalars['String']['output']>;
  /** Reads a single `FrontierEvmLog` that is related to this `Event`. */
  frontierEvmLogByEventId?: Maybe<FrontierEvmLog>;
  /**
   * Reads and enables pagination through a set of `FrontierEvmLog`.
   * @deprecated Please use frontierEvmLogByEventId instead
   */
  frontierEvmLogsByEventId: FrontierEvmLogsConnection;
  /** Reads a single `GearMessageEnqueued` that is related to this `Event`. */
  gearMessageEnqueuedByEventId?: Maybe<GearMessageEnqueued>;
  /**
   * Reads and enables pagination through a set of `GearMessageEnqueued`.
   * @deprecated Please use gearMessageEnqueuedByEventId instead
   */
  gearMessageEnqueuedsByEventId: GearMessageEnqueuedsConnection;
  /** Reads a single `GearUserMessageSent` that is related to this `Event`. */
  gearUserMessageSentByEventId?: Maybe<GearUserMessageSent>;
  /**
   * Reads and enables pagination through a set of `GearUserMessageSent`.
   * @deprecated Please use gearUserMessageSentByEventId instead
   */
  gearUserMessageSentsByEventId: GearUserMessageSentsConnection;
  id: Scalars['String']['output'];
  indexInBlock: Scalars['Int']['output'];
  name: Scalars['String']['output'];
  /** A globally unique identifier. Can be used in various places throughout the system to identify this single value. */
  nodeId: Scalars['ID']['output'];
  phase: Scalars['String']['output'];
  pos: Scalars['Int']['output'];
};


export type EventAcalaEvmExecutedFailedLogsByEventIdArgs = {
  after?: InputMaybe<Scalars['Cursor']['input']>;
  before?: InputMaybe<Scalars['Cursor']['input']>;
  condition?: InputMaybe<AcalaEvmExecutedFailedLogCondition>;
  filter?: InputMaybe<AcalaEvmExecutedFailedLogFilter>;
  first?: InputMaybe<Scalars['Int']['input']>;
  last?: InputMaybe<Scalars['Int']['input']>;
  offset?: InputMaybe<Scalars['Int']['input']>;
  orderBy?: InputMaybe<Array<AcalaEvmExecutedFailedLogsOrderBy>>;
};


export type EventAcalaEvmExecutedFailedsByEventIdArgs = {
  after?: InputMaybe<Scalars['Cursor']['input']>;
  before?: InputMaybe<Scalars['Cursor']['input']>;
  condition?: InputMaybe<AcalaEvmExecutedFailedCondition>;
  filter?: InputMaybe<AcalaEvmExecutedFailedFilter>;
  first?: InputMaybe<Scalars['Int']['input']>;
  last?: InputMaybe<Scalars['Int']['input']>;
  offset?: InputMaybe<Scalars['Int']['input']>;
  orderBy?: InputMaybe<Array<AcalaEvmExecutedFailedsOrderBy>>;
};


export type EventAcalaEvmExecutedLogsByEventIdArgs = {
  after?: InputMaybe<Scalars['Cursor']['input']>;
  before?: InputMaybe<Scalars['Cursor']['input']>;
  condition?: InputMaybe<AcalaEvmExecutedLogCondition>;
  filter?: InputMaybe<AcalaEvmExecutedLogFilter>;
  first?: InputMaybe<Scalars['Int']['input']>;
  last?: InputMaybe<Scalars['Int']['input']>;
  offset?: InputMaybe<Scalars['Int']['input']>;
  orderBy?: InputMaybe<Array<AcalaEvmExecutedLogsOrderBy>>;
};


export type EventAcalaEvmExecutedsByEventIdArgs = {
  after?: InputMaybe<Scalars['Cursor']['input']>;
  before?: InputMaybe<Scalars['Cursor']['input']>;
  condition?: InputMaybe<AcalaEvmExecutedCondition>;
  filter?: InputMaybe<AcalaEvmExecutedFilter>;
  first?: InputMaybe<Scalars['Int']['input']>;
  last?: InputMaybe<Scalars['Int']['input']>;
  offset?: InputMaybe<Scalars['Int']['input']>;
  orderBy?: InputMaybe<Array<AcalaEvmExecutedsOrderBy>>;
};


export type EventContractsContractEmittedsByEventIdArgs = {
  after?: InputMaybe<Scalars['Cursor']['input']>;
  before?: InputMaybe<Scalars['Cursor']['input']>;
  condition?: InputMaybe<ContractsContractEmittedCondition>;
  filter?: InputMaybe<ContractsContractEmittedFilter>;
  first?: InputMaybe<Scalars['Int']['input']>;
  last?: InputMaybe<Scalars['Int']['input']>;
  offset?: InputMaybe<Scalars['Int']['input']>;
  orderBy?: InputMaybe<Array<ContractsContractEmittedsOrderBy>>;
};


export type EventFrontierEvmLogsByEventIdArgs = {
  after?: InputMaybe<Scalars['Cursor']['input']>;
  before?: InputMaybe<Scalars['Cursor']['input']>;
  condition?: InputMaybe<FrontierEvmLogCondition>;
  filter?: InputMaybe<FrontierEvmLogFilter>;
  first?: InputMaybe<Scalars['Int']['input']>;
  last?: InputMaybe<Scalars['Int']['input']>;
  offset?: InputMaybe<Scalars['Int']['input']>;
  orderBy?: InputMaybe<Array<FrontierEvmLogsOrderBy>>;
};


export type EventGearMessageEnqueuedsByEventIdArgs = {
  after?: InputMaybe<Scalars['Cursor']['input']>;
  before?: InputMaybe<Scalars['Cursor']['input']>;
  condition?: InputMaybe<GearMessageEnqueuedCondition>;
  filter?: InputMaybe<GearMessageEnqueuedFilter>;
  first?: InputMaybe<Scalars['Int']['input']>;
  last?: InputMaybe<Scalars['Int']['input']>;
  offset?: InputMaybe<Scalars['Int']['input']>;
  orderBy?: InputMaybe<Array<GearMessageEnqueuedsOrderBy>>;
};


export type EventGearUserMessageSentsByEventIdArgs = {
  after?: InputMaybe<Scalars['Cursor']['input']>;
  before?: InputMaybe<Scalars['Cursor']['input']>;
  condition?: InputMaybe<GearUserMessageSentCondition>;
  filter?: InputMaybe<GearUserMessageSentFilter>;
  first?: InputMaybe<Scalars['Int']['input']>;
  last?: InputMaybe<Scalars['Int']['input']>;
  offset?: InputMaybe<Scalars['Int']['input']>;
  orderBy?: InputMaybe<Array<GearUserMessageSentsOrderBy>>;
};

/** A condition to be used against `Event` object types. All fields are tested for equality and combined with a logical ‘and.’ */
export type EventCondition = {
  /** Checks for equality with the object’s `args` field. */
  args?: InputMaybe<Scalars['JSON']['input']>;
  /** Checks for equality with the object’s `blockId` field. */
  blockId?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `callId` field. */
  callId?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `extrinsicId` field. */
  extrinsicId?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `id` field. */
  id?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `indexInBlock` field. */
  indexInBlock?: InputMaybe<Scalars['Int']['input']>;
  /** Checks for equality with the object’s `name` field. */
  name?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `phase` field. */
  phase?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `pos` field. */
  pos?: InputMaybe<Scalars['Int']['input']>;
};

/** A filter to be used against `Event` object types. All fields are combined with a logical ‘and.’ */
export type EventFilter = {
  /** Checks for all expressions in this list. */
  and?: InputMaybe<Array<EventFilter>>;
  /** Filter by the object’s `args` field. */
  args?: InputMaybe<JsonFilter>;
  /** Filter by the object’s `blockId` field. */
  blockId?: InputMaybe<StringFilter>;
  /** Filter by the object’s `callId` field. */
  callId?: InputMaybe<StringFilter>;
  /** Filter by the object’s `extrinsicId` field. */
  extrinsicId?: InputMaybe<StringFilter>;
  /** Filter by the object’s `id` field. */
  id?: InputMaybe<StringFilter>;
  /** Filter by the object’s `indexInBlock` field. */
  indexInBlock?: InputMaybe<IntFilter>;
  /** Filter by the object’s `name` field. */
  name?: InputMaybe<StringFilter>;
  /** Negates the expression. */
  not?: InputMaybe<EventFilter>;
  /** Checks for any expressions in this list. */
  or?: InputMaybe<Array<EventFilter>>;
  /** Filter by the object’s `phase` field. */
  phase?: InputMaybe<StringFilter>;
  /** Filter by the object’s `pos` field. */
  pos?: InputMaybe<IntFilter>;
};

/** A connection to a list of `Event` values. */
export type EventsConnection = {
  __typename?: 'EventsConnection';
  /** A list of edges which contains the `Event` and cursor to aid in pagination. */
  edges: Array<EventsEdge>;
  /** A list of `Event` objects. */
  nodes: Array<Event>;
  /** Information to aid in pagination. */
  pageInfo: PageInfo;
  /** The count of *all* `Event` you could get from the connection. */
  totalCount: Scalars['Int']['output'];
};

/** A `Event` edge in the connection. */
export type EventsEdge = {
  __typename?: 'EventsEdge';
  /** A cursor for use in pagination. */
  cursor?: Maybe<Scalars['Cursor']['output']>;
  /** The `Event` at the end of the edge. */
  node: Event;
};

/** Methods to use when ordering `Event`. */
export type EventsOrderBy =
  | 'ARGS_ASC'
  | 'ARGS_DESC'
  | 'BLOCK_ID_ASC'
  | 'BLOCK_ID_DESC'
  | 'CALL_ID_ASC'
  | 'CALL_ID_DESC'
  | 'EXTRINSIC_ID_ASC'
  | 'EXTRINSIC_ID_DESC'
  | 'ID_ASC'
  | 'ID_DESC'
  | 'INDEX_IN_BLOCK_ASC'
  | 'INDEX_IN_BLOCK_DESC'
  | 'NAME_ASC'
  | 'NAME_DESC'
  | 'NATURAL'
  | 'PHASE_ASC'
  | 'PHASE_DESC'
  | 'POS_ASC'
  | 'POS_DESC'
  | 'PRIMARY_KEY_ASC'
  | 'PRIMARY_KEY_DESC';

export type Extrinsic = Node & {
  __typename?: 'Extrinsic';
  /** Reads a single `Block` that is related to this `Extrinsic`. */
  blockByBlockId: Block;
  blockId: Scalars['String']['output'];
  callId: Scalars['String']['output'];
  /** Reads and enables pagination through a set of `Call`. */
  callsByExtrinsicId: CallsConnection;
  error?: Maybe<Scalars['JSON']['output']>;
  /** Reads and enables pagination through a set of `Event`. */
  eventsByExtrinsicId: EventsConnection;
  fee?: Maybe<Scalars['BigFloat']['output']>;
  hash: Scalars['String']['output'];
  id: Scalars['String']['output'];
  indexInBlock: Scalars['Int']['output'];
  /** A globally unique identifier. Can be used in various places throughout the system to identify this single value. */
  nodeId: Scalars['ID']['output'];
  pos: Scalars['Int']['output'];
  signature?: Maybe<Scalars['JSON']['output']>;
  success: Scalars['Boolean']['output'];
  tip?: Maybe<Scalars['BigFloat']['output']>;
  version: Scalars['Int']['output'];
};


export type ExtrinsicCallsByExtrinsicIdArgs = {
  after?: InputMaybe<Scalars['Cursor']['input']>;
  before?: InputMaybe<Scalars['Cursor']['input']>;
  condition?: InputMaybe<CallCondition>;
  filter?: InputMaybe<CallFilter>;
  first?: InputMaybe<Scalars['Int']['input']>;
  last?: InputMaybe<Scalars['Int']['input']>;
  offset?: InputMaybe<Scalars['Int']['input']>;
  orderBy?: InputMaybe<Array<CallsOrderBy>>;
};


export type ExtrinsicEventsByExtrinsicIdArgs = {
  after?: InputMaybe<Scalars['Cursor']['input']>;
  before?: InputMaybe<Scalars['Cursor']['input']>;
  condition?: InputMaybe<EventCondition>;
  filter?: InputMaybe<EventFilter>;
  first?: InputMaybe<Scalars['Int']['input']>;
  last?: InputMaybe<Scalars['Int']['input']>;
  offset?: InputMaybe<Scalars['Int']['input']>;
  orderBy?: InputMaybe<Array<EventsOrderBy>>;
};

/**
 * A condition to be used against `Extrinsic` object types. All fields are tested
 * for equality and combined with a logical ‘and.’
 */
export type ExtrinsicCondition = {
  /** Checks for equality with the object’s `blockId` field. */
  blockId?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `callId` field. */
  callId?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `error` field. */
  error?: InputMaybe<Scalars['JSON']['input']>;
  /** Checks for equality with the object’s `fee` field. */
  fee?: InputMaybe<Scalars['BigFloat']['input']>;
  /** Checks for equality with the object’s `hash` field. */
  hash?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `id` field. */
  id?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `indexInBlock` field. */
  indexInBlock?: InputMaybe<Scalars['Int']['input']>;
  /** Checks for equality with the object’s `pos` field. */
  pos?: InputMaybe<Scalars['Int']['input']>;
  /** Checks for equality with the object’s `signature` field. */
  signature?: InputMaybe<Scalars['JSON']['input']>;
  /** Checks for equality with the object’s `success` field. */
  success?: InputMaybe<Scalars['Boolean']['input']>;
  /** Checks for equality with the object’s `tip` field. */
  tip?: InputMaybe<Scalars['BigFloat']['input']>;
  /** Checks for equality with the object’s `version` field. */
  version?: InputMaybe<Scalars['Int']['input']>;
};

/** A filter to be used against `Extrinsic` object types. All fields are combined with a logical ‘and.’ */
export type ExtrinsicFilter = {
  /** Checks for all expressions in this list. */
  and?: InputMaybe<Array<ExtrinsicFilter>>;
  /** Filter by the object’s `blockId` field. */
  blockId?: InputMaybe<StringFilter>;
  /** Filter by the object’s `callId` field. */
  callId?: InputMaybe<StringFilter>;
  /** Filter by the object’s `error` field. */
  error?: InputMaybe<JsonFilter>;
  /** Filter by the object’s `fee` field. */
  fee?: InputMaybe<BigFloatFilter>;
  /** Filter by the object’s `hash` field. */
  hash?: InputMaybe<StringFilter>;
  /** Filter by the object’s `id` field. */
  id?: InputMaybe<StringFilter>;
  /** Filter by the object’s `indexInBlock` field. */
  indexInBlock?: InputMaybe<IntFilter>;
  /** Negates the expression. */
  not?: InputMaybe<ExtrinsicFilter>;
  /** Checks for any expressions in this list. */
  or?: InputMaybe<Array<ExtrinsicFilter>>;
  /** Filter by the object’s `pos` field. */
  pos?: InputMaybe<IntFilter>;
  /** Filter by the object’s `signature` field. */
  signature?: InputMaybe<JsonFilter>;
  /** Filter by the object’s `success` field. */
  success?: InputMaybe<BooleanFilter>;
  /** Filter by the object’s `tip` field. */
  tip?: InputMaybe<BigFloatFilter>;
  /** Filter by the object’s `version` field. */
  version?: InputMaybe<IntFilter>;
};

/** A connection to a list of `Extrinsic` values. */
export type ExtrinsicsConnection = {
  __typename?: 'ExtrinsicsConnection';
  /** A list of edges which contains the `Extrinsic` and cursor to aid in pagination. */
  edges: Array<ExtrinsicsEdge>;
  /** A list of `Extrinsic` objects. */
  nodes: Array<Extrinsic>;
  /** Information to aid in pagination. */
  pageInfo: PageInfo;
  /** The count of *all* `Extrinsic` you could get from the connection. */
  totalCount: Scalars['Int']['output'];
};

/** A `Extrinsic` edge in the connection. */
export type ExtrinsicsEdge = {
  __typename?: 'ExtrinsicsEdge';
  /** A cursor for use in pagination. */
  cursor?: Maybe<Scalars['Cursor']['output']>;
  /** The `Extrinsic` at the end of the edge. */
  node: Extrinsic;
};

/** Methods to use when ordering `Extrinsic`. */
export type ExtrinsicsOrderBy =
  | 'BLOCK_ID_ASC'
  | 'BLOCK_ID_DESC'
  | 'CALL_ID_ASC'
  | 'CALL_ID_DESC'
  | 'ERROR_ASC'
  | 'ERROR_DESC'
  | 'FEE_ASC'
  | 'FEE_DESC'
  | 'HASH_ASC'
  | 'HASH_DESC'
  | 'ID_ASC'
  | 'ID_DESC'
  | 'INDEX_IN_BLOCK_ASC'
  | 'INDEX_IN_BLOCK_DESC'
  | 'NATURAL'
  | 'POS_ASC'
  | 'POS_DESC'
  | 'PRIMARY_KEY_ASC'
  | 'PRIMARY_KEY_DESC'
  | 'SIGNATURE_ASC'
  | 'SIGNATURE_DESC'
  | 'SUCCESS_ASC'
  | 'SUCCESS_DESC'
  | 'TIP_ASC'
  | 'TIP_DESC'
  | 'VERSION_ASC'
  | 'VERSION_DESC';

export type FrontierEthereumTransaction = Node & {
  __typename?: 'FrontierEthereumTransaction';
  /** Reads a single `Call` that is related to this `FrontierEthereumTransaction`. */
  callByCallId: Call;
  callId: Scalars['String']['output'];
  contract: Scalars['String']['output'];
  /** A globally unique identifier. Can be used in various places throughout the system to identify this single value. */
  nodeId: Scalars['ID']['output'];
  sighash?: Maybe<Scalars['String']['output']>;
};

/**
 * A condition to be used against `FrontierEthereumTransaction` object types. All
 * fields are tested for equality and combined with a logical ‘and.’
 */
export type FrontierEthereumTransactionCondition = {
  /** Checks for equality with the object’s `callId` field. */
  callId?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `contract` field. */
  contract?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `sighash` field. */
  sighash?: InputMaybe<Scalars['String']['input']>;
};

/** A filter to be used against `FrontierEthereumTransaction` object types. All fields are combined with a logical ‘and.’ */
export type FrontierEthereumTransactionFilter = {
  /** Checks for all expressions in this list. */
  and?: InputMaybe<Array<FrontierEthereumTransactionFilter>>;
  /** Filter by the object’s `callId` field. */
  callId?: InputMaybe<StringFilter>;
  /** Filter by the object’s `contract` field. */
  contract?: InputMaybe<StringFilter>;
  /** Negates the expression. */
  not?: InputMaybe<FrontierEthereumTransactionFilter>;
  /** Checks for any expressions in this list. */
  or?: InputMaybe<Array<FrontierEthereumTransactionFilter>>;
  /** Filter by the object’s `sighash` field. */
  sighash?: InputMaybe<StringFilter>;
};

/** A connection to a list of `FrontierEthereumTransaction` values. */
export type FrontierEthereumTransactionsConnection = {
  __typename?: 'FrontierEthereumTransactionsConnection';
  /** A list of edges which contains the `FrontierEthereumTransaction` and cursor to aid in pagination. */
  edges: Array<FrontierEthereumTransactionsEdge>;
  /** A list of `FrontierEthereumTransaction` objects. */
  nodes: Array<FrontierEthereumTransaction>;
  /** Information to aid in pagination. */
  pageInfo: PageInfo;
  /** The count of *all* `FrontierEthereumTransaction` you could get from the connection. */
  totalCount: Scalars['Int']['output'];
};

/** A `FrontierEthereumTransaction` edge in the connection. */
export type FrontierEthereumTransactionsEdge = {
  __typename?: 'FrontierEthereumTransactionsEdge';
  /** A cursor for use in pagination. */
  cursor?: Maybe<Scalars['Cursor']['output']>;
  /** The `FrontierEthereumTransaction` at the end of the edge. */
  node: FrontierEthereumTransaction;
};

/** Methods to use when ordering `FrontierEthereumTransaction`. */
export type FrontierEthereumTransactionsOrderBy =
  | 'CALL_ID_ASC'
  | 'CALL_ID_DESC'
  | 'CONTRACT_ASC'
  | 'CONTRACT_DESC'
  | 'NATURAL'
  | 'PRIMARY_KEY_ASC'
  | 'PRIMARY_KEY_DESC'
  | 'SIGHASH_ASC'
  | 'SIGHASH_DESC';

export type FrontierEvmLog = Node & {
  __typename?: 'FrontierEvmLog';
  contract: Scalars['String']['output'];
  /** Reads a single `Event` that is related to this `FrontierEvmLog`. */
  eventByEventId: Event;
  eventId: Scalars['String']['output'];
  /** A globally unique identifier. Can be used in various places throughout the system to identify this single value. */
  nodeId: Scalars['ID']['output'];
  topic0?: Maybe<Scalars['String']['output']>;
  topic1?: Maybe<Scalars['String']['output']>;
  topic2?: Maybe<Scalars['String']['output']>;
  topic3?: Maybe<Scalars['String']['output']>;
};

/**
 * A condition to be used against `FrontierEvmLog` object types. All fields are
 * tested for equality and combined with a logical ‘and.’
 */
export type FrontierEvmLogCondition = {
  /** Checks for equality with the object’s `contract` field. */
  contract?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `eventId` field. */
  eventId?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `topic0` field. */
  topic0?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `topic1` field. */
  topic1?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `topic2` field. */
  topic2?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `topic3` field. */
  topic3?: InputMaybe<Scalars['String']['input']>;
};

/** A filter to be used against `FrontierEvmLog` object types. All fields are combined with a logical ‘and.’ */
export type FrontierEvmLogFilter = {
  /** Checks for all expressions in this list. */
  and?: InputMaybe<Array<FrontierEvmLogFilter>>;
  /** Filter by the object’s `contract` field. */
  contract?: InputMaybe<StringFilter>;
  /** Filter by the object’s `eventId` field. */
  eventId?: InputMaybe<StringFilter>;
  /** Negates the expression. */
  not?: InputMaybe<FrontierEvmLogFilter>;
  /** Checks for any expressions in this list. */
  or?: InputMaybe<Array<FrontierEvmLogFilter>>;
  /** Filter by the object’s `topic0` field. */
  topic0?: InputMaybe<StringFilter>;
  /** Filter by the object’s `topic1` field. */
  topic1?: InputMaybe<StringFilter>;
  /** Filter by the object’s `topic2` field. */
  topic2?: InputMaybe<StringFilter>;
  /** Filter by the object’s `topic3` field. */
  topic3?: InputMaybe<StringFilter>;
};

/** A connection to a list of `FrontierEvmLog` values. */
export type FrontierEvmLogsConnection = {
  __typename?: 'FrontierEvmLogsConnection';
  /** A list of edges which contains the `FrontierEvmLog` and cursor to aid in pagination. */
  edges: Array<FrontierEvmLogsEdge>;
  /** A list of `FrontierEvmLog` objects. */
  nodes: Array<FrontierEvmLog>;
  /** Information to aid in pagination. */
  pageInfo: PageInfo;
  /** The count of *all* `FrontierEvmLog` you could get from the connection. */
  totalCount: Scalars['Int']['output'];
};

/** A `FrontierEvmLog` edge in the connection. */
export type FrontierEvmLogsEdge = {
  __typename?: 'FrontierEvmLogsEdge';
  /** A cursor for use in pagination. */
  cursor?: Maybe<Scalars['Cursor']['output']>;
  /** The `FrontierEvmLog` at the end of the edge. */
  node: FrontierEvmLog;
};

/** Methods to use when ordering `FrontierEvmLog`. */
export type FrontierEvmLogsOrderBy =
  | 'CONTRACT_ASC'
  | 'CONTRACT_DESC'
  | 'EVENT_ID_ASC'
  | 'EVENT_ID_DESC'
  | 'NATURAL'
  | 'PRIMARY_KEY_ASC'
  | 'PRIMARY_KEY_DESC'
  | 'TOPIC0_ASC'
  | 'TOPIC0_DESC'
  | 'TOPIC1_ASC'
  | 'TOPIC1_DESC'
  | 'TOPIC2_ASC'
  | 'TOPIC2_DESC'
  | 'TOPIC3_ASC'
  | 'TOPIC3_DESC';

export type GearMessageEnqueued = Node & {
  __typename?: 'GearMessageEnqueued';
  /** Reads a single `Event` that is related to this `GearMessageEnqueued`. */
  eventByEventId: Event;
  eventId: Scalars['String']['output'];
  /** A globally unique identifier. Can be used in various places throughout the system to identify this single value. */
  nodeId: Scalars['ID']['output'];
  program: Scalars['String']['output'];
};

/**
 * A condition to be used against `GearMessageEnqueued` object types. All fields
 * are tested for equality and combined with a logical ‘and.’
 */
export type GearMessageEnqueuedCondition = {
  /** Checks for equality with the object’s `eventId` field. */
  eventId?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `program` field. */
  program?: InputMaybe<Scalars['String']['input']>;
};

/** A filter to be used against `GearMessageEnqueued` object types. All fields are combined with a logical ‘and.’ */
export type GearMessageEnqueuedFilter = {
  /** Checks for all expressions in this list. */
  and?: InputMaybe<Array<GearMessageEnqueuedFilter>>;
  /** Filter by the object’s `eventId` field. */
  eventId?: InputMaybe<StringFilter>;
  /** Negates the expression. */
  not?: InputMaybe<GearMessageEnqueuedFilter>;
  /** Checks for any expressions in this list. */
  or?: InputMaybe<Array<GearMessageEnqueuedFilter>>;
  /** Filter by the object’s `program` field. */
  program?: InputMaybe<StringFilter>;
};

/** A connection to a list of `GearMessageEnqueued` values. */
export type GearMessageEnqueuedsConnection = {
  __typename?: 'GearMessageEnqueuedsConnection';
  /** A list of edges which contains the `GearMessageEnqueued` and cursor to aid in pagination. */
  edges: Array<GearMessageEnqueuedsEdge>;
  /** A list of `GearMessageEnqueued` objects. */
  nodes: Array<GearMessageEnqueued>;
  /** Information to aid in pagination. */
  pageInfo: PageInfo;
  /** The count of *all* `GearMessageEnqueued` you could get from the connection. */
  totalCount: Scalars['Int']['output'];
};

/** A `GearMessageEnqueued` edge in the connection. */
export type GearMessageEnqueuedsEdge = {
  __typename?: 'GearMessageEnqueuedsEdge';
  /** A cursor for use in pagination. */
  cursor?: Maybe<Scalars['Cursor']['output']>;
  /** The `GearMessageEnqueued` at the end of the edge. */
  node: GearMessageEnqueued;
};

/** Methods to use when ordering `GearMessageEnqueued`. */
export type GearMessageEnqueuedsOrderBy =
  | 'EVENT_ID_ASC'
  | 'EVENT_ID_DESC'
  | 'NATURAL'
  | 'PRIMARY_KEY_ASC'
  | 'PRIMARY_KEY_DESC'
  | 'PROGRAM_ASC'
  | 'PROGRAM_DESC';

export type GearUserMessageSent = Node & {
  __typename?: 'GearUserMessageSent';
  /** Reads a single `Event` that is related to this `GearUserMessageSent`. */
  eventByEventId: Event;
  eventId: Scalars['String']['output'];
  /** A globally unique identifier. Can be used in various places throughout the system to identify this single value. */
  nodeId: Scalars['ID']['output'];
  program: Scalars['String']['output'];
};

/**
 * A condition to be used against `GearUserMessageSent` object types. All fields
 * are tested for equality and combined with a logical ‘and.’
 */
export type GearUserMessageSentCondition = {
  /** Checks for equality with the object’s `eventId` field. */
  eventId?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `program` field. */
  program?: InputMaybe<Scalars['String']['input']>;
};

/** A filter to be used against `GearUserMessageSent` object types. All fields are combined with a logical ‘and.’ */
export type GearUserMessageSentFilter = {
  /** Checks for all expressions in this list. */
  and?: InputMaybe<Array<GearUserMessageSentFilter>>;
  /** Filter by the object’s `eventId` field. */
  eventId?: InputMaybe<StringFilter>;
  /** Negates the expression. */
  not?: InputMaybe<GearUserMessageSentFilter>;
  /** Checks for any expressions in this list. */
  or?: InputMaybe<Array<GearUserMessageSentFilter>>;
  /** Filter by the object’s `program` field. */
  program?: InputMaybe<StringFilter>;
};

/** A connection to a list of `GearUserMessageSent` values. */
export type GearUserMessageSentsConnection = {
  __typename?: 'GearUserMessageSentsConnection';
  /** A list of edges which contains the `GearUserMessageSent` and cursor to aid in pagination. */
  edges: Array<GearUserMessageSentsEdge>;
  /** A list of `GearUserMessageSent` objects. */
  nodes: Array<GearUserMessageSent>;
  /** Information to aid in pagination. */
  pageInfo: PageInfo;
  /** The count of *all* `GearUserMessageSent` you could get from the connection. */
  totalCount: Scalars['Int']['output'];
};

/** A `GearUserMessageSent` edge in the connection. */
export type GearUserMessageSentsEdge = {
  __typename?: 'GearUserMessageSentsEdge';
  /** A cursor for use in pagination. */
  cursor?: Maybe<Scalars['Cursor']['output']>;
  /** The `GearUserMessageSent` at the end of the edge. */
  node: GearUserMessageSent;
};

/** Methods to use when ordering `GearUserMessageSent`. */
export type GearUserMessageSentsOrderBy =
  | 'EVENT_ID_ASC'
  | 'EVENT_ID_DESC'
  | 'NATURAL'
  | 'PRIMARY_KEY_ASC'
  | 'PRIMARY_KEY_DESC'
  | 'PROGRAM_ASC'
  | 'PROGRAM_DESC';

/** A filter to be used against Int fields. All fields are combined with a logical ‘and.’ */
export type IntFilter = {
  /** Not equal to the specified value, treating null like an ordinary value. */
  distinctFrom?: InputMaybe<Scalars['Int']['input']>;
  /** Equal to the specified value. */
  equalTo?: InputMaybe<Scalars['Int']['input']>;
  /** Greater than the specified value. */
  greaterThan?: InputMaybe<Scalars['Int']['input']>;
  /** Greater than or equal to the specified value. */
  greaterThanOrEqualTo?: InputMaybe<Scalars['Int']['input']>;
  /** Included in the specified list. */
  in?: InputMaybe<Array<Scalars['Int']['input']>>;
  /** Is null (if `true` is specified) or is not null (if `false` is specified). */
  isNull?: InputMaybe<Scalars['Boolean']['input']>;
  /** Less than the specified value. */
  lessThan?: InputMaybe<Scalars['Int']['input']>;
  /** Less than or equal to the specified value. */
  lessThanOrEqualTo?: InputMaybe<Scalars['Int']['input']>;
  /** Equal to the specified value, treating null like an ordinary value. */
  notDistinctFrom?: InputMaybe<Scalars['Int']['input']>;
  /** Not equal to the specified value. */
  notEqualTo?: InputMaybe<Scalars['Int']['input']>;
  /** Not included in the specified list. */
  notIn?: InputMaybe<Array<Scalars['Int']['input']>>;
};

/** A filter to be used against JSON fields. All fields are combined with a logical ‘and.’ */
export type JsonFilter = {
  /** Contained by the specified JSON. */
  containedBy?: InputMaybe<Scalars['JSON']['input']>;
  /** Contains the specified JSON. */
  contains?: InputMaybe<Scalars['JSON']['input']>;
  /** Contains all of the specified keys. */
  containsAllKeys?: InputMaybe<Array<Scalars['String']['input']>>;
  /** Contains any of the specified keys. */
  containsAnyKeys?: InputMaybe<Array<Scalars['String']['input']>>;
  /** Contains the specified key. */
  containsKey?: InputMaybe<Scalars['String']['input']>;
  /** Not equal to the specified value, treating null like an ordinary value. */
  distinctFrom?: InputMaybe<Scalars['JSON']['input']>;
  /** Equal to the specified value. */
  equalTo?: InputMaybe<Scalars['JSON']['input']>;
  /** Greater than the specified value. */
  greaterThan?: InputMaybe<Scalars['JSON']['input']>;
  /** Greater than or equal to the specified value. */
  greaterThanOrEqualTo?: InputMaybe<Scalars['JSON']['input']>;
  /** Included in the specified list. */
  in?: InputMaybe<Array<Scalars['JSON']['input']>>;
  /** Is null (if `true` is specified) or is not null (if `false` is specified). */
  isNull?: InputMaybe<Scalars['Boolean']['input']>;
  /** Less than the specified value. */
  lessThan?: InputMaybe<Scalars['JSON']['input']>;
  /** Less than or equal to the specified value. */
  lessThanOrEqualTo?: InputMaybe<Scalars['JSON']['input']>;
  /** Equal to the specified value, treating null like an ordinary value. */
  notDistinctFrom?: InputMaybe<Scalars['JSON']['input']>;
  /** Not equal to the specified value. */
  notEqualTo?: InputMaybe<Scalars['JSON']['input']>;
  /** Not included in the specified list. */
  notIn?: InputMaybe<Array<Scalars['JSON']['input']>>;
};

/** A connection to a list of `Metadatum` values. */
export type MetadataConnection = {
  __typename?: 'MetadataConnection';
  /** A list of edges which contains the `Metadatum` and cursor to aid in pagination. */
  edges: Array<MetadataEdge>;
  /** A list of `Metadatum` objects. */
  nodes: Array<Metadatum>;
  /** Information to aid in pagination. */
  pageInfo: PageInfo;
  /** The count of *all* `Metadatum` you could get from the connection. */
  totalCount: Scalars['Int']['output'];
};

/** A `Metadatum` edge in the connection. */
export type MetadataEdge = {
  __typename?: 'MetadataEdge';
  /** A cursor for use in pagination. */
  cursor?: Maybe<Scalars['Cursor']['output']>;
  /** The `Metadatum` at the end of the edge. */
  node: Metadatum;
};

/** Methods to use when ordering `Metadatum`. */
export type MetadataOrderBy =
  | 'BLOCK_HASH_ASC'
  | 'BLOCK_HASH_DESC'
  | 'BLOCK_HEIGHT_ASC'
  | 'BLOCK_HEIGHT_DESC'
  | 'HEX_ASC'
  | 'HEX_DESC'
  | 'ID_ASC'
  | 'ID_DESC'
  | 'NATURAL'
  | 'PRIMARY_KEY_ASC'
  | 'PRIMARY_KEY_DESC'
  | 'SPEC_NAME_ASC'
  | 'SPEC_NAME_DESC'
  | 'SPEC_VERSION_ASC'
  | 'SPEC_VERSION_DESC';

export type Metadatum = Node & {
  __typename?: 'Metadatum';
  blockHash: Scalars['String']['output'];
  blockHeight: Scalars['Int']['output'];
  hex: Scalars['String']['output'];
  id: Scalars['String']['output'];
  /** A globally unique identifier. Can be used in various places throughout the system to identify this single value. */
  nodeId: Scalars['ID']['output'];
  specName: Scalars['String']['output'];
  specVersion?: Maybe<Scalars['Int']['output']>;
};

/**
 * A condition to be used against `Metadatum` object types. All fields are tested
 * for equality and combined with a logical ‘and.’
 */
export type MetadatumCondition = {
  /** Checks for equality with the object’s `blockHash` field. */
  blockHash?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `blockHeight` field. */
  blockHeight?: InputMaybe<Scalars['Int']['input']>;
  /** Checks for equality with the object’s `hex` field. */
  hex?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `id` field. */
  id?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `specName` field. */
  specName?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `specVersion` field. */
  specVersion?: InputMaybe<Scalars['Int']['input']>;
};

/** A filter to be used against `Metadatum` object types. All fields are combined with a logical ‘and.’ */
export type MetadatumFilter = {
  /** Checks for all expressions in this list. */
  and?: InputMaybe<Array<MetadatumFilter>>;
  /** Filter by the object’s `blockHash` field. */
  blockHash?: InputMaybe<StringFilter>;
  /** Filter by the object’s `blockHeight` field. */
  blockHeight?: InputMaybe<IntFilter>;
  /** Filter by the object’s `hex` field. */
  hex?: InputMaybe<StringFilter>;
  /** Filter by the object’s `id` field. */
  id?: InputMaybe<StringFilter>;
  /** Negates the expression. */
  not?: InputMaybe<MetadatumFilter>;
  /** Checks for any expressions in this list. */
  or?: InputMaybe<Array<MetadatumFilter>>;
  /** Filter by the object’s `specName` field. */
  specName?: InputMaybe<StringFilter>;
  /** Filter by the object’s `specVersion` field. */
  specVersion?: InputMaybe<IntFilter>;
};

export type Migration = Node & {
  __typename?: 'Migration';
  executedAt?: Maybe<Scalars['Datetime']['output']>;
  hash: Scalars['String']['output'];
  id: Scalars['Int']['output'];
  name: Scalars['String']['output'];
  /** A globally unique identifier. Can be used in various places throughout the system to identify this single value. */
  nodeId: Scalars['ID']['output'];
};

/**
 * A condition to be used against `Migration` object types. All fields are tested
 * for equality and combined with a logical ‘and.’
 */
export type MigrationCondition = {
  /** Checks for equality with the object’s `executedAt` field. */
  executedAt?: InputMaybe<Scalars['Datetime']['input']>;
  /** Checks for equality with the object’s `hash` field. */
  hash?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `id` field. */
  id?: InputMaybe<Scalars['Int']['input']>;
  /** Checks for equality with the object’s `name` field. */
  name?: InputMaybe<Scalars['String']['input']>;
};

/** A filter to be used against `Migration` object types. All fields are combined with a logical ‘and.’ */
export type MigrationFilter = {
  /** Checks for all expressions in this list. */
  and?: InputMaybe<Array<MigrationFilter>>;
  /** Filter by the object’s `executedAt` field. */
  executedAt?: InputMaybe<DatetimeFilter>;
  /** Filter by the object’s `hash` field. */
  hash?: InputMaybe<StringFilter>;
  /** Filter by the object’s `id` field. */
  id?: InputMaybe<IntFilter>;
  /** Filter by the object’s `name` field. */
  name?: InputMaybe<StringFilter>;
  /** Negates the expression. */
  not?: InputMaybe<MigrationFilter>;
  /** Checks for any expressions in this list. */
  or?: InputMaybe<Array<MigrationFilter>>;
};

/** A connection to a list of `Migration` values. */
export type MigrationsConnection = {
  __typename?: 'MigrationsConnection';
  /** A list of edges which contains the `Migration` and cursor to aid in pagination. */
  edges: Array<MigrationsEdge>;
  /** A list of `Migration` objects. */
  nodes: Array<Migration>;
  /** Information to aid in pagination. */
  pageInfo: PageInfo;
  /** The count of *all* `Migration` you could get from the connection. */
  totalCount: Scalars['Int']['output'];
};

/** A `Migration` edge in the connection. */
export type MigrationsEdge = {
  __typename?: 'MigrationsEdge';
  /** A cursor for use in pagination. */
  cursor?: Maybe<Scalars['Cursor']['output']>;
  /** The `Migration` at the end of the edge. */
  node: Migration;
};

/** Methods to use when ordering `Migration`. */
export type MigrationsOrderBy =
  | 'EXECUTED_AT_ASC'
  | 'EXECUTED_AT_DESC'
  | 'HASH_ASC'
  | 'HASH_DESC'
  | 'ID_ASC'
  | 'ID_DESC'
  | 'NAME_ASC'
  | 'NAME_DESC'
  | 'NATURAL'
  | 'PRIMARY_KEY_ASC'
  | 'PRIMARY_KEY_DESC';

/** An object with a globally unique `ID`. */
export type Node = {
  /** A globally unique identifier. Can be used in various places throughout the system to identify this single value. */
  nodeId: Scalars['ID']['output'];
};

/** Information about pagination in a connection. */
export type PageInfo = {
  __typename?: 'PageInfo';
  /** When paginating forwards, the cursor to continue. */
  endCursor?: Maybe<Scalars['Cursor']['output']>;
  /** When paginating forwards, are there more items? */
  hasNextPage: Scalars['Boolean']['output'];
  /** When paginating backwards, are there more items? */
  hasPreviousPage: Scalars['Boolean']['output'];
  /** When paginating backwards, the cursor to continue. */
  startCursor?: Maybe<Scalars['Cursor']['output']>;
};

/** The root query type which gives access points into the data universe. */
export type Query = Node & {
  __typename?: 'Query';
  /** Reads a single `AcalaEvmExecuted` using its globally unique `ID`. */
  acalaEvmExecuted?: Maybe<AcalaEvmExecuted>;
  acalaEvmExecutedByEventId?: Maybe<AcalaEvmExecuted>;
  /** Reads a single `AcalaEvmExecutedFailed` using its globally unique `ID`. */
  acalaEvmExecutedFailed?: Maybe<AcalaEvmExecutedFailed>;
  acalaEvmExecutedFailedByEventId?: Maybe<AcalaEvmExecutedFailed>;
  /** Reads a single `AcalaEvmExecutedFailedLog` using its globally unique `ID`. */
  acalaEvmExecutedFailedLog?: Maybe<AcalaEvmExecutedFailedLog>;
  acalaEvmExecutedFailedLogById?: Maybe<AcalaEvmExecutedFailedLog>;
  /** Reads a single `AcalaEvmExecutedLog` using its globally unique `ID`. */
  acalaEvmExecutedLog?: Maybe<AcalaEvmExecutedLog>;
  acalaEvmExecutedLogById?: Maybe<AcalaEvmExecutedLog>;
  /** Reads and enables pagination through a set of `AcalaEvmExecutedFailedLog`. */
  allAcalaEvmExecutedFailedLogs?: Maybe<AcalaEvmExecutedFailedLogsConnection>;
  /** Reads and enables pagination through a set of `AcalaEvmExecutedFailed`. */
  allAcalaEvmExecutedFaileds?: Maybe<AcalaEvmExecutedFailedsConnection>;
  /** Reads and enables pagination through a set of `AcalaEvmExecutedLog`. */
  allAcalaEvmExecutedLogs?: Maybe<AcalaEvmExecutedLogsConnection>;
  /** Reads and enables pagination through a set of `AcalaEvmExecuted`. */
  allAcalaEvmExecuteds?: Maybe<AcalaEvmExecutedsConnection>;
  /** Reads and enables pagination through a set of `Block`. */
  allBlocks?: Maybe<BlocksConnection>;
  /** Reads and enables pagination through a set of `Call`. */
  allCalls?: Maybe<CallsConnection>;
  /** Reads and enables pagination through a set of `ContractsContractEmitted`. */
  allContractsContractEmitteds?: Maybe<ContractsContractEmittedsConnection>;
  /** Reads and enables pagination through a set of `Event`. */
  allEvents?: Maybe<EventsConnection>;
  /** Reads and enables pagination through a set of `Extrinsic`. */
  allExtrinsics?: Maybe<ExtrinsicsConnection>;
  /** Reads and enables pagination through a set of `FrontierEthereumTransaction`. */
  allFrontierEthereumTransactions?: Maybe<FrontierEthereumTransactionsConnection>;
  /** Reads and enables pagination through a set of `FrontierEvmLog`. */
  allFrontierEvmLogs?: Maybe<FrontierEvmLogsConnection>;
  /** Reads and enables pagination through a set of `GearMessageEnqueued`. */
  allGearMessageEnqueueds?: Maybe<GearMessageEnqueuedsConnection>;
  /** Reads and enables pagination through a set of `GearUserMessageSent`. */
  allGearUserMessageSents?: Maybe<GearUserMessageSentsConnection>;
  /** Reads and enables pagination through a set of `Metadatum`. */
  allMetadata?: Maybe<MetadataConnection>;
  /** Reads and enables pagination through a set of `Migration`. */
  allMigrations?: Maybe<MigrationsConnection>;
  /** Reads and enables pagination through a set of `Warning`. */
  allWarnings?: Maybe<WarningsConnection>;
  /** Reads a single `Block` using its globally unique `ID`. */
  block?: Maybe<Block>;
  blockById?: Maybe<Block>;
  /** Reads a single `Call` using its globally unique `ID`. */
  call?: Maybe<Call>;
  callById?: Maybe<Call>;
  /** Reads a single `ContractsContractEmitted` using its globally unique `ID`. */
  contractsContractEmitted?: Maybe<ContractsContractEmitted>;
  contractsContractEmittedByEventId?: Maybe<ContractsContractEmitted>;
  /** Reads a single `Event` using its globally unique `ID`. */
  event?: Maybe<Event>;
  eventById?: Maybe<Event>;
  /** Reads a single `Extrinsic` using its globally unique `ID`. */
  extrinsic?: Maybe<Extrinsic>;
  extrinsicById?: Maybe<Extrinsic>;
  /** Reads a single `FrontierEthereumTransaction` using its globally unique `ID`. */
  frontierEthereumTransaction?: Maybe<FrontierEthereumTransaction>;
  frontierEthereumTransactionByCallId?: Maybe<FrontierEthereumTransaction>;
  /** Reads a single `FrontierEvmLog` using its globally unique `ID`. */
  frontierEvmLog?: Maybe<FrontierEvmLog>;
  frontierEvmLogByEventId?: Maybe<FrontierEvmLog>;
  /** Reads a single `GearMessageEnqueued` using its globally unique `ID`. */
  gearMessageEnqueued?: Maybe<GearMessageEnqueued>;
  gearMessageEnqueuedByEventId?: Maybe<GearMessageEnqueued>;
  /** Reads a single `GearUserMessageSent` using its globally unique `ID`. */
  gearUserMessageSent?: Maybe<GearUserMessageSent>;
  gearUserMessageSentByEventId?: Maybe<GearUserMessageSent>;
  /** Reads a single `Metadatum` using its globally unique `ID`. */
  metadatum?: Maybe<Metadatum>;
  metadatumById?: Maybe<Metadatum>;
  /** Reads a single `Migration` using its globally unique `ID`. */
  migration?: Maybe<Migration>;
  migrationById?: Maybe<Migration>;
  migrationByName?: Maybe<Migration>;
  /** Fetches an object given its globally unique `ID`. */
  node?: Maybe<Node>;
  /** The root query type must be a `Node` to work well with Relay 1 mutations. This just resolves to `query`. */
  nodeId: Scalars['ID']['output'];
  /**
   * Exposes the root query type nested one level down. This is helpful for Relay 1
   * which can only query top level fields if they are in a particular form.
   */
  query: Query;
};


/** The root query type which gives access points into the data universe. */
export type QueryAcalaEvmExecutedArgs = {
  nodeId: Scalars['ID']['input'];
};


/** The root query type which gives access points into the data universe. */
export type QueryAcalaEvmExecutedByEventIdArgs = {
  eventId: Scalars['String']['input'];
};


/** The root query type which gives access points into the data universe. */
export type QueryAcalaEvmExecutedFailedArgs = {
  nodeId: Scalars['ID']['input'];
};


/** The root query type which gives access points into the data universe. */
export type QueryAcalaEvmExecutedFailedByEventIdArgs = {
  eventId: Scalars['String']['input'];
};


/** The root query type which gives access points into the data universe. */
export type QueryAcalaEvmExecutedFailedLogArgs = {
  nodeId: Scalars['ID']['input'];
};


/** The root query type which gives access points into the data universe. */
export type QueryAcalaEvmExecutedFailedLogByIdArgs = {
  id: Scalars['String']['input'];
};


/** The root query type which gives access points into the data universe. */
export type QueryAcalaEvmExecutedLogArgs = {
  nodeId: Scalars['ID']['input'];
};


/** The root query type which gives access points into the data universe. */
export type QueryAcalaEvmExecutedLogByIdArgs = {
  id: Scalars['String']['input'];
};


/** The root query type which gives access points into the data universe. */
export type QueryAllAcalaEvmExecutedFailedLogsArgs = {
  after?: InputMaybe<Scalars['Cursor']['input']>;
  before?: InputMaybe<Scalars['Cursor']['input']>;
  condition?: InputMaybe<AcalaEvmExecutedFailedLogCondition>;
  filter?: InputMaybe<AcalaEvmExecutedFailedLogFilter>;
  first?: InputMaybe<Scalars['Int']['input']>;
  last?: InputMaybe<Scalars['Int']['input']>;
  offset?: InputMaybe<Scalars['Int']['input']>;
  orderBy?: InputMaybe<Array<AcalaEvmExecutedFailedLogsOrderBy>>;
};


/** The root query type which gives access points into the data universe. */
export type QueryAllAcalaEvmExecutedFailedsArgs = {
  after?: InputMaybe<Scalars['Cursor']['input']>;
  before?: InputMaybe<Scalars['Cursor']['input']>;
  condition?: InputMaybe<AcalaEvmExecutedFailedCondition>;
  filter?: InputMaybe<AcalaEvmExecutedFailedFilter>;
  first?: InputMaybe<Scalars['Int']['input']>;
  last?: InputMaybe<Scalars['Int']['input']>;
  offset?: InputMaybe<Scalars['Int']['input']>;
  orderBy?: InputMaybe<Array<AcalaEvmExecutedFailedsOrderBy>>;
};


/** The root query type which gives access points into the data universe. */
export type QueryAllAcalaEvmExecutedLogsArgs = {
  after?: InputMaybe<Scalars['Cursor']['input']>;
  before?: InputMaybe<Scalars['Cursor']['input']>;
  condition?: InputMaybe<AcalaEvmExecutedLogCondition>;
  filter?: InputMaybe<AcalaEvmExecutedLogFilter>;
  first?: InputMaybe<Scalars['Int']['input']>;
  last?: InputMaybe<Scalars['Int']['input']>;
  offset?: InputMaybe<Scalars['Int']['input']>;
  orderBy?: InputMaybe<Array<AcalaEvmExecutedLogsOrderBy>>;
};


/** The root query type which gives access points into the data universe. */
export type QueryAllAcalaEvmExecutedsArgs = {
  after?: InputMaybe<Scalars['Cursor']['input']>;
  before?: InputMaybe<Scalars['Cursor']['input']>;
  condition?: InputMaybe<AcalaEvmExecutedCondition>;
  filter?: InputMaybe<AcalaEvmExecutedFilter>;
  first?: InputMaybe<Scalars['Int']['input']>;
  last?: InputMaybe<Scalars['Int']['input']>;
  offset?: InputMaybe<Scalars['Int']['input']>;
  orderBy?: InputMaybe<Array<AcalaEvmExecutedsOrderBy>>;
};


/** The root query type which gives access points into the data universe. */
export type QueryAllBlocksArgs = {
  after?: InputMaybe<Scalars['Cursor']['input']>;
  before?: InputMaybe<Scalars['Cursor']['input']>;
  condition?: InputMaybe<BlockCondition>;
  filter?: InputMaybe<BlockFilter>;
  first?: InputMaybe<Scalars['Int']['input']>;
  last?: InputMaybe<Scalars['Int']['input']>;
  offset?: InputMaybe<Scalars['Int']['input']>;
  orderBy?: InputMaybe<Array<BlocksOrderBy>>;
};


/** The root query type which gives access points into the data universe. */
export type QueryAllCallsArgs = {
  after?: InputMaybe<Scalars['Cursor']['input']>;
  before?: InputMaybe<Scalars['Cursor']['input']>;
  condition?: InputMaybe<CallCondition>;
  filter?: InputMaybe<CallFilter>;
  first?: InputMaybe<Scalars['Int']['input']>;
  last?: InputMaybe<Scalars['Int']['input']>;
  offset?: InputMaybe<Scalars['Int']['input']>;
  orderBy?: InputMaybe<Array<CallsOrderBy>>;
};


/** The root query type which gives access points into the data universe. */
export type QueryAllContractsContractEmittedsArgs = {
  after?: InputMaybe<Scalars['Cursor']['input']>;
  before?: InputMaybe<Scalars['Cursor']['input']>;
  condition?: InputMaybe<ContractsContractEmittedCondition>;
  filter?: InputMaybe<ContractsContractEmittedFilter>;
  first?: InputMaybe<Scalars['Int']['input']>;
  last?: InputMaybe<Scalars['Int']['input']>;
  offset?: InputMaybe<Scalars['Int']['input']>;
  orderBy?: InputMaybe<Array<ContractsContractEmittedsOrderBy>>;
};


/** The root query type which gives access points into the data universe. */
export type QueryAllEventsArgs = {
  after?: InputMaybe<Scalars['Cursor']['input']>;
  before?: InputMaybe<Scalars['Cursor']['input']>;
  condition?: InputMaybe<EventCondition>;
  filter?: InputMaybe<EventFilter>;
  first?: InputMaybe<Scalars['Int']['input']>;
  last?: InputMaybe<Scalars['Int']['input']>;
  offset?: InputMaybe<Scalars['Int']['input']>;
  orderBy?: InputMaybe<Array<EventsOrderBy>>;
};


/** The root query type which gives access points into the data universe. */
export type QueryAllExtrinsicsArgs = {
  after?: InputMaybe<Scalars['Cursor']['input']>;
  before?: InputMaybe<Scalars['Cursor']['input']>;
  condition?: InputMaybe<ExtrinsicCondition>;
  filter?: InputMaybe<ExtrinsicFilter>;
  first?: InputMaybe<Scalars['Int']['input']>;
  last?: InputMaybe<Scalars['Int']['input']>;
  offset?: InputMaybe<Scalars['Int']['input']>;
  orderBy?: InputMaybe<Array<ExtrinsicsOrderBy>>;
};


/** The root query type which gives access points into the data universe. */
export type QueryAllFrontierEthereumTransactionsArgs = {
  after?: InputMaybe<Scalars['Cursor']['input']>;
  before?: InputMaybe<Scalars['Cursor']['input']>;
  condition?: InputMaybe<FrontierEthereumTransactionCondition>;
  filter?: InputMaybe<FrontierEthereumTransactionFilter>;
  first?: InputMaybe<Scalars['Int']['input']>;
  last?: InputMaybe<Scalars['Int']['input']>;
  offset?: InputMaybe<Scalars['Int']['input']>;
  orderBy?: InputMaybe<Array<FrontierEthereumTransactionsOrderBy>>;
};


/** The root query type which gives access points into the data universe. */
export type QueryAllFrontierEvmLogsArgs = {
  after?: InputMaybe<Scalars['Cursor']['input']>;
  before?: InputMaybe<Scalars['Cursor']['input']>;
  condition?: InputMaybe<FrontierEvmLogCondition>;
  filter?: InputMaybe<FrontierEvmLogFilter>;
  first?: InputMaybe<Scalars['Int']['input']>;
  last?: InputMaybe<Scalars['Int']['input']>;
  offset?: InputMaybe<Scalars['Int']['input']>;
  orderBy?: InputMaybe<Array<FrontierEvmLogsOrderBy>>;
};


/** The root query type which gives access points into the data universe. */
export type QueryAllGearMessageEnqueuedsArgs = {
  after?: InputMaybe<Scalars['Cursor']['input']>;
  before?: InputMaybe<Scalars['Cursor']['input']>;
  condition?: InputMaybe<GearMessageEnqueuedCondition>;
  filter?: InputMaybe<GearMessageEnqueuedFilter>;
  first?: InputMaybe<Scalars['Int']['input']>;
  last?: InputMaybe<Scalars['Int']['input']>;
  offset?: InputMaybe<Scalars['Int']['input']>;
  orderBy?: InputMaybe<Array<GearMessageEnqueuedsOrderBy>>;
};


/** The root query type which gives access points into the data universe. */
export type QueryAllGearUserMessageSentsArgs = {
  after?: InputMaybe<Scalars['Cursor']['input']>;
  before?: InputMaybe<Scalars['Cursor']['input']>;
  condition?: InputMaybe<GearUserMessageSentCondition>;
  filter?: InputMaybe<GearUserMessageSentFilter>;
  first?: InputMaybe<Scalars['Int']['input']>;
  last?: InputMaybe<Scalars['Int']['input']>;
  offset?: InputMaybe<Scalars['Int']['input']>;
  orderBy?: InputMaybe<Array<GearUserMessageSentsOrderBy>>;
};


/** The root query type which gives access points into the data universe. */
export type QueryAllMetadataArgs = {
  after?: InputMaybe<Scalars['Cursor']['input']>;
  before?: InputMaybe<Scalars['Cursor']['input']>;
  condition?: InputMaybe<MetadatumCondition>;
  filter?: InputMaybe<MetadatumFilter>;
  first?: InputMaybe<Scalars['Int']['input']>;
  last?: InputMaybe<Scalars['Int']['input']>;
  offset?: InputMaybe<Scalars['Int']['input']>;
  orderBy?: InputMaybe<Array<MetadataOrderBy>>;
};


/** The root query type which gives access points into the data universe. */
export type QueryAllMigrationsArgs = {
  after?: InputMaybe<Scalars['Cursor']['input']>;
  before?: InputMaybe<Scalars['Cursor']['input']>;
  condition?: InputMaybe<MigrationCondition>;
  filter?: InputMaybe<MigrationFilter>;
  first?: InputMaybe<Scalars['Int']['input']>;
  last?: InputMaybe<Scalars['Int']['input']>;
  offset?: InputMaybe<Scalars['Int']['input']>;
  orderBy?: InputMaybe<Array<MigrationsOrderBy>>;
};


/** The root query type which gives access points into the data universe. */
export type QueryAllWarningsArgs = {
  after?: InputMaybe<Scalars['Cursor']['input']>;
  before?: InputMaybe<Scalars['Cursor']['input']>;
  condition?: InputMaybe<WarningCondition>;
  filter?: InputMaybe<WarningFilter>;
  first?: InputMaybe<Scalars['Int']['input']>;
  last?: InputMaybe<Scalars['Int']['input']>;
  offset?: InputMaybe<Scalars['Int']['input']>;
  orderBy?: InputMaybe<Array<WarningsOrderBy>>;
};


/** The root query type which gives access points into the data universe. */
export type QueryBlockArgs = {
  nodeId: Scalars['ID']['input'];
};


/** The root query type which gives access points into the data universe. */
export type QueryBlockByIdArgs = {
  id: Scalars['String']['input'];
};


/** The root query type which gives access points into the data universe. */
export type QueryCallArgs = {
  nodeId: Scalars['ID']['input'];
};


/** The root query type which gives access points into the data universe. */
export type QueryCallByIdArgs = {
  id: Scalars['String']['input'];
};


/** The root query type which gives access points into the data universe. */
export type QueryContractsContractEmittedArgs = {
  nodeId: Scalars['ID']['input'];
};


/** The root query type which gives access points into the data universe. */
export type QueryContractsContractEmittedByEventIdArgs = {
  eventId: Scalars['String']['input'];
};


/** The root query type which gives access points into the data universe. */
export type QueryEventArgs = {
  nodeId: Scalars['ID']['input'];
};


/** The root query type which gives access points into the data universe. */
export type QueryEventByIdArgs = {
  id: Scalars['String']['input'];
};


/** The root query type which gives access points into the data universe. */
export type QueryExtrinsicArgs = {
  nodeId: Scalars['ID']['input'];
};


/** The root query type which gives access points into the data universe. */
export type QueryExtrinsicByIdArgs = {
  id: Scalars['String']['input'];
};


/** The root query type which gives access points into the data universe. */
export type QueryFrontierEthereumTransactionArgs = {
  nodeId: Scalars['ID']['input'];
};


/** The root query type which gives access points into the data universe. */
export type QueryFrontierEthereumTransactionByCallIdArgs = {
  callId: Scalars['String']['input'];
};


/** The root query type which gives access points into the data universe. */
export type QueryFrontierEvmLogArgs = {
  nodeId: Scalars['ID']['input'];
};


/** The root query type which gives access points into the data universe. */
export type QueryFrontierEvmLogByEventIdArgs = {
  eventId: Scalars['String']['input'];
};


/** The root query type which gives access points into the data universe. */
export type QueryGearMessageEnqueuedArgs = {
  nodeId: Scalars['ID']['input'];
};


/** The root query type which gives access points into the data universe. */
export type QueryGearMessageEnqueuedByEventIdArgs = {
  eventId: Scalars['String']['input'];
};


/** The root query type which gives access points into the data universe. */
export type QueryGearUserMessageSentArgs = {
  nodeId: Scalars['ID']['input'];
};


/** The root query type which gives access points into the data universe. */
export type QueryGearUserMessageSentByEventIdArgs = {
  eventId: Scalars['String']['input'];
};


/** The root query type which gives access points into the data universe. */
export type QueryMetadatumArgs = {
  nodeId: Scalars['ID']['input'];
};


/** The root query type which gives access points into the data universe. */
export type QueryMetadatumByIdArgs = {
  id: Scalars['String']['input'];
};


/** The root query type which gives access points into the data universe. */
export type QueryMigrationArgs = {
  nodeId: Scalars['ID']['input'];
};


/** The root query type which gives access points into the data universe. */
export type QueryMigrationByIdArgs = {
  id: Scalars['Int']['input'];
};


/** The root query type which gives access points into the data universe. */
export type QueryMigrationByNameArgs = {
  name: Scalars['String']['input'];
};


/** The root query type which gives access points into the data universe. */
export type QueryNodeArgs = {
  nodeId: Scalars['ID']['input'];
};

/** A filter to be used against String fields. All fields are combined with a logical ‘and.’ */
export type StringFilter = {
  /** Not equal to the specified value, treating null like an ordinary value. */
  distinctFrom?: InputMaybe<Scalars['String']['input']>;
  /** Not equal to the specified value, treating null like an ordinary value (case-insensitive). */
  distinctFromInsensitive?: InputMaybe<Scalars['String']['input']>;
  /** Ends with the specified string (case-sensitive). */
  endsWith?: InputMaybe<Scalars['String']['input']>;
  /** Ends with the specified string (case-insensitive). */
  endsWithInsensitive?: InputMaybe<Scalars['String']['input']>;
  /** Equal to the specified value. */
  equalTo?: InputMaybe<Scalars['String']['input']>;
  /** Equal to the specified value (case-insensitive). */
  equalToInsensitive?: InputMaybe<Scalars['String']['input']>;
  /** Greater than the specified value. */
  greaterThan?: InputMaybe<Scalars['String']['input']>;
  /** Greater than the specified value (case-insensitive). */
  greaterThanInsensitive?: InputMaybe<Scalars['String']['input']>;
  /** Greater than or equal to the specified value. */
  greaterThanOrEqualTo?: InputMaybe<Scalars['String']['input']>;
  /** Greater than or equal to the specified value (case-insensitive). */
  greaterThanOrEqualToInsensitive?: InputMaybe<Scalars['String']['input']>;
  /** Included in the specified list. */
  in?: InputMaybe<Array<Scalars['String']['input']>>;
  /** Included in the specified list (case-insensitive). */
  inInsensitive?: InputMaybe<Array<Scalars['String']['input']>>;
  /** Contains the specified string (case-sensitive). */
  includes?: InputMaybe<Scalars['String']['input']>;
  /** Contains the specified string (case-insensitive). */
  includesInsensitive?: InputMaybe<Scalars['String']['input']>;
  /** Is null (if `true` is specified) or is not null (if `false` is specified). */
  isNull?: InputMaybe<Scalars['Boolean']['input']>;
  /** Less than the specified value. */
  lessThan?: InputMaybe<Scalars['String']['input']>;
  /** Less than the specified value (case-insensitive). */
  lessThanInsensitive?: InputMaybe<Scalars['String']['input']>;
  /** Less than or equal to the specified value. */
  lessThanOrEqualTo?: InputMaybe<Scalars['String']['input']>;
  /** Less than or equal to the specified value (case-insensitive). */
  lessThanOrEqualToInsensitive?: InputMaybe<Scalars['String']['input']>;
  /** Matches the specified pattern (case-sensitive). An underscore (_) matches any single character; a percent sign (%) matches any sequence of zero or more characters. */
  like?: InputMaybe<Scalars['String']['input']>;
  /** Matches the specified pattern (case-insensitive). An underscore (_) matches any single character; a percent sign (%) matches any sequence of zero or more characters. */
  likeInsensitive?: InputMaybe<Scalars['String']['input']>;
  /** Equal to the specified value, treating null like an ordinary value. */
  notDistinctFrom?: InputMaybe<Scalars['String']['input']>;
  /** Equal to the specified value, treating null like an ordinary value (case-insensitive). */
  notDistinctFromInsensitive?: InputMaybe<Scalars['String']['input']>;
  /** Does not end with the specified string (case-sensitive). */
  notEndsWith?: InputMaybe<Scalars['String']['input']>;
  /** Does not end with the specified string (case-insensitive). */
  notEndsWithInsensitive?: InputMaybe<Scalars['String']['input']>;
  /** Not equal to the specified value. */
  notEqualTo?: InputMaybe<Scalars['String']['input']>;
  /** Not equal to the specified value (case-insensitive). */
  notEqualToInsensitive?: InputMaybe<Scalars['String']['input']>;
  /** Not included in the specified list. */
  notIn?: InputMaybe<Array<Scalars['String']['input']>>;
  /** Not included in the specified list (case-insensitive). */
  notInInsensitive?: InputMaybe<Array<Scalars['String']['input']>>;
  /** Does not contain the specified string (case-sensitive). */
  notIncludes?: InputMaybe<Scalars['String']['input']>;
  /** Does not contain the specified string (case-insensitive). */
  notIncludesInsensitive?: InputMaybe<Scalars['String']['input']>;
  /** Does not match the specified pattern (case-sensitive). An underscore (_) matches any single character; a percent sign (%) matches any sequence of zero or more characters. */
  notLike?: InputMaybe<Scalars['String']['input']>;
  /** Does not match the specified pattern (case-insensitive). An underscore (_) matches any single character; a percent sign (%) matches any sequence of zero or more characters. */
  notLikeInsensitive?: InputMaybe<Scalars['String']['input']>;
  /** Does not start with the specified string (case-sensitive). */
  notStartsWith?: InputMaybe<Scalars['String']['input']>;
  /** Does not start with the specified string (case-insensitive). */
  notStartsWithInsensitive?: InputMaybe<Scalars['String']['input']>;
  /** Starts with the specified string (case-sensitive). */
  startsWith?: InputMaybe<Scalars['String']['input']>;
  /** Starts with the specified string (case-insensitive). */
  startsWithInsensitive?: InputMaybe<Scalars['String']['input']>;
};

export type Warning = {
  __typename?: 'Warning';
  blockId?: Maybe<Scalars['String']['output']>;
  message?: Maybe<Scalars['String']['output']>;
};

/** A condition to be used against `Warning` object types. All fields are tested for equality and combined with a logical ‘and.’ */
export type WarningCondition = {
  /** Checks for equality with the object’s `blockId` field. */
  blockId?: InputMaybe<Scalars['String']['input']>;
  /** Checks for equality with the object’s `message` field. */
  message?: InputMaybe<Scalars['String']['input']>;
};

/** A filter to be used against `Warning` object types. All fields are combined with a logical ‘and.’ */
export type WarningFilter = {
  /** Checks for all expressions in this list. */
  and?: InputMaybe<Array<WarningFilter>>;
  /** Filter by the object’s `blockId` field. */
  blockId?: InputMaybe<StringFilter>;
  /** Filter by the object’s `message` field. */
  message?: InputMaybe<StringFilter>;
  /** Negates the expression. */
  not?: InputMaybe<WarningFilter>;
  /** Checks for any expressions in this list. */
  or?: InputMaybe<Array<WarningFilter>>;
};

/** A connection to a list of `Warning` values. */
export type WarningsConnection = {
  __typename?: 'WarningsConnection';
  /** A list of edges which contains the `Warning` and cursor to aid in pagination. */
  edges: Array<WarningsEdge>;
  /** A list of `Warning` objects. */
  nodes: Array<Warning>;
  /** Information to aid in pagination. */
  pageInfo: PageInfo;
  /** The count of *all* `Warning` you could get from the connection. */
  totalCount: Scalars['Int']['output'];
};

/** A `Warning` edge in the connection. */
export type WarningsEdge = {
  __typename?: 'WarningsEdge';
  /** A cursor for use in pagination. */
  cursor?: Maybe<Scalars['Cursor']['output']>;
  /** The `Warning` at the end of the edge. */
  node: Warning;
};

/** Methods to use when ordering `Warning`. */
export type WarningsOrderBy =
  | 'BLOCK_ID_ASC'
  | 'BLOCK_ID_DESC'
  | 'MESSAGE_ASC'
  | 'MESSAGE_DESC'
  | 'NATURAL';

export type GetBatchQueryVariables = Exact<{
  height: Scalars['Int']['input'];
  limit: Scalars['Int']['input'];
  swapEvents: Array<Scalars['String']['input']> | Scalars['String']['input'];
}>;


export type GetBatchQuery = { __typename?: 'Query', blocks?: { __typename?: 'BlocksConnection', nodes: Array<{ __typename?: 'Block', height: number, hash: string, timestamp: any, specId: string, events: { __typename?: 'EventsConnection', nodes: Array<{ __typename?: 'Event', args?: any | null, name: string, indexInBlock: number }> } }> } | null };


export const GetBatchDocument = {"kind":"Document","definitions":[{"kind":"OperationDefinition","operation":"query","name":{"kind":"Name","value":"GetBatch"},"variableDefinitions":[{"kind":"VariableDefinition","variable":{"kind":"Variable","name":{"kind":"Name","value":"height"}},"type":{"kind":"NonNullType","type":{"kind":"NamedType","name":{"kind":"Name","value":"Int"}}}},{"kind":"VariableDefinition","variable":{"kind":"Variable","name":{"kind":"Name","value":"limit"}},"type":{"kind":"NonNullType","type":{"kind":"NamedType","name":{"kind":"Name","value":"Int"}}}},{"kind":"VariableDefinition","variable":{"kind":"Variable","name":{"kind":"Name","value":"swapEvents"}},"type":{"kind":"NonNullType","type":{"kind":"ListType","type":{"kind":"NonNullType","type":{"kind":"NamedType","name":{"kind":"Name","value":"String"}}}}}}],"selectionSet":{"kind":"SelectionSet","selections":[{"kind":"Field","alias":{"kind":"Name","value":"blocks"},"name":{"kind":"Name","value":"allBlocks"},"arguments":[{"kind":"Argument","name":{"kind":"Name","value":"filter"},"value":{"kind":"ObjectValue","fields":[{"kind":"ObjectField","name":{"kind":"Name","value":"height"},"value":{"kind":"ObjectValue","fields":[{"kind":"ObjectField","name":{"kind":"Name","value":"greaterThanOrEqualTo"},"value":{"kind":"Variable","name":{"kind":"Name","value":"height"}}}]}}]}},{"kind":"Argument","name":{"kind":"Name","value":"first"},"value":{"kind":"Variable","name":{"kind":"Name","value":"limit"}}},{"kind":"Argument","name":{"kind":"Name","value":"orderBy"},"value":{"kind":"EnumValue","value":"HEIGHT_ASC"}}],"selectionSet":{"kind":"SelectionSet","selections":[{"kind":"Field","name":{"kind":"Name","value":"nodes"},"selectionSet":{"kind":"SelectionSet","selections":[{"kind":"Field","name":{"kind":"Name","value":"height"}},{"kind":"Field","name":{"kind":"Name","value":"hash"}},{"kind":"Field","name":{"kind":"Name","value":"timestamp"}},{"kind":"Field","name":{"kind":"Name","value":"specId"}},{"kind":"Field","alias":{"kind":"Name","value":"events"},"name":{"kind":"Name","value":"eventsByBlockId"},"arguments":[{"kind":"Argument","name":{"kind":"Name","value":"filter"},"value":{"kind":"ObjectValue","fields":[{"kind":"ObjectField","name":{"kind":"Name","value":"name"},"value":{"kind":"ObjectValue","fields":[{"kind":"ObjectField","name":{"kind":"Name","value":"in"},"value":{"kind":"Variable","name":{"kind":"Name","value":"swapEvents"}}}]}}]}}],"selectionSet":{"kind":"SelectionSet","selections":[{"kind":"Field","name":{"kind":"Name","value":"nodes"},"selectionSet":{"kind":"SelectionSet","selections":[{"kind":"Field","name":{"kind":"Name","value":"args"}},{"kind":"Field","name":{"kind":"Name","value":"name"}},{"kind":"Field","name":{"kind":"Name","value":"indexInBlock"}}]}}]}}]}}]}}]}}]} as unknown as DocumentNode<GetBatchQuery, GetBatchQueryVariables>;