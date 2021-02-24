## Testing against a mock Chainflip Engine (CFE)

You can use [json-server](https://github.com/typicode/json-server) to test without having to run the CFE.

From project root you can run:
```
// install json-server
npm install -g json-server

// TODO CHECK THE ENDPOINTS MATCH
// run the server using witness_db
json-server --watch test/witness_db.json
```