import { gql } from './generated';

export const GET_BATCH = gql(/* GraphQL */ `
  query GetBatch($height: Int!, $limit: Int!, $swapEvents: [String!]!) {
    blocks: allBlocks(
      filter: { height: { greaterThanOrEqualTo: $height } }
      first: $limit
      orderBy: HEIGHT_ASC
    ) {
      nodes {
        height
        hash
        timestamp
        specId
        events: eventsByBlockId(filter: { name: { in: $swapEvents } }) {
          nodes {
            args
            name
            indexInBlock
          }
        }
      }
    }
  }
`);
