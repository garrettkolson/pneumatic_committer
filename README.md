# Pneumatic Committer

The Pneumatic Committer is a core process that runs as part of a node in the Pneumatic blockchain protocol. This process is responsible for receiving finalized transactions, validating the resulting blocks, and committing finalized blocks to permanent storage. It ensures the integrity and consistency of the blockchain by enforcing validation and proper handling of transaction data.

## Overview
- **Role in Pneumatic Protocol:**
  - Acts as a specialized node process dedicated to block validation and commitment.
  - Works in tandem with other node types (e.g., registries, routers) within the Pneumatic ecosystem.
- **Responsibilities:**
  - Receives finalized transactions from the network.
  - Validates that the block formed from a transaction is correct and meets protocol requirements.
  - Commits the validated block to permanent storage.
  - Handles error cases, such as malformed transactions or invalid blocks, by queueing them for reconciliation.

## Main Components

### 1. Committer
- Central struct orchestrating the commit process.
- Maintains references to:
  - `ActionRouter` (for routing actions and distributing tokens)
  - `NodeRegistry` (for network node management)
  - `DataProvider` (for persistent storage)
  - `ConnFactory` (for network connections)
  - `Logger` (for logging events)
- Tracks pending transactions to avoid double-processing.

### 2. ActionRouter
- Handles routing of actions and distribution of tokens within the node.
- Listens for registration events and coordinates with other components.

### 3. NodeRegistry
- Manages node registration and communication within the network.
- Handles updates and maintains the list of active nodes.

### 4. DataProvider
- Abstracts the persistent storage layer.
- Responsible for saving transaction data, blocks, and tokens.

## Process Flow

1. **Listening for New Blocks:**
   - The committer listens for incoming finalized transactions using a network listener and thread pool.
   - Each incoming transaction is processed asynchronously.

2. **Processing Incoming Transactions:**
   - The transaction is deserialized and validated for integrity (e.g., signature checks, structure, and replay protection).
   - Checks for duplicate or already-processed transactions.

3. **Block Validation:**
   - The block proposed by the transaction is validated according to protocol rules.
   - If validation fails, the block is queued for reconciliation.

4. **Committing the Block:**
   - If validation succeeds, the block is added to the chain and committed to permanent storage.
   - Token and block data are saved via the `DataProvider`.
   - The token is distributed to other nodes as needed.

5. **Error Handling:**
   - Handles malformed data, invalid signatures, and other errors gracefully.
   - Uses a robust error enum (`CommitterError`) for clear error reporting and handling.

## Key Functions
- `listen_for_new_blocks`: Main entry point for block listening and processing.
- `process_incoming_block`: Handles the receipt and initial processing of incoming transactions.
- `validate_transaction_message`: Checks the integrity and validity of a transaction message.
- `validate_block_and_commit`: Validates the block and commits or queues it as appropriate.
- `commit_block`: Handles the actual block commitment to storage.
- `queue_for_reconciliation`: Queues invalid blocks for later review and reconciliation.

## Running the Committer

The committer process is typically launched as part of a Pneumatic node. Ensure all dependencies (network, storage, configuration) are available. The main entry point is in `main.rs`, which sets up the node registry, action router, and committer threads. The process itself will be started from the main Pneumatic node process.

## Extensibility & Testing
- The committer is designed for extensibility and testing, with traits and interfaces for mocking core dependencies.
- Comprehensive error handling and logging are built in.

## License
This project is part of the Pneumatic blockchain protocol and is released under the terms specified in the repository.

---

For more information, see the source code and documentation in the `src/` directory.
