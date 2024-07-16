pub mod actions;

use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::net::{SocketAddr};
use std::ops::Deref;
use std::sync::{Arc, RwLock};
use dashmap::{DashSet};
use pneumatic_core::config::Config;
use pneumatic_core::{conns, logging::*, messages::*, server::*, node::*, tokens::*, transactions::*};
use pneumatic_core::blocks::Block;
use pneumatic_core::conns::{ConnFactory, Stream};
use pneumatic_core::data::{DataError, DataProvider};
use pneumatic_core::encoding::{deserialize_rmp_to, serialize_to_bytes_rmp};
use pneumatic_core::environment::EnvironmentMetadata;
use crate::actions::ActionRouter;

const BLOCK_LISTENER_THREAD_COUNT: usize = 10;

pub struct Committer {
    config: Config,
    router: Arc<ActionRouter>,      // TODO: have to make a trait for this to properly mock
    registry: Arc<NodeRegistry>,    // TODO: have to make a trait for this to properly mock
    data_provider: Arc<Box<dyn DataProvider>>,
    conn_factory: Arc<Box<dyn ConnFactory>>,
    logger: Arc<Box<dyn Logger>>,
    pending_transactions: Arc<DashSet<Vec<u8>>>
}

impl Committer {
    pub fn init(config: Config,
                router: Arc<ActionRouter>,
                registry: Arc<NodeRegistry>,
                data_provider: Arc<Box<dyn DataProvider>>,
                conn_factory: Arc<Box<dyn ConnFactory>>,
                logger: Arc<Box<dyn Logger>>) -> Self {
        Committer {
            config,
            router,
            registry,
            data_provider,
            conn_factory,
            logger,
            pending_transactions: Arc::new(DashSet::new())
        }
    }

    pub fn listen_for_new_blocks(router: Arc<ActionRouter>,
                                 registry: Arc<NodeRegistry>,
                                 data_provider: Arc<Box<dyn DataProvider>>,
                                 conn_factory: Arc<Box<dyn ConnFactory>>,
                                 logger: Arc<Box<dyn Logger>>) {
        let config = Config::build()
            .expect("Couldn't build config for committer");

        let ip = config.ip_address;
        let addr = SocketAddr::new(ip, conns::COMMITTER_PORT);
        let listener = &conn_factory.get_listener(addr);
        let thread_pool = ThreadPool::build(BLOCK_LISTENER_THREAD_COUNT)
            .expect("Couldn't establish thread pool for committing new blocks");

        let committer = Arc::new(Committer::init(
            config, router, registry, data_provider, conn_factory, logger));

        loop {
            match listener.accept() {
                Err(_) => continue,
                Ok((stream, _)) => {
                    let clone = committer.clone();
                    Self::process_incoming_block(stream, clone, &thread_pool);
                }
            }
        }
    }

    fn process_incoming_block(mut stream: Box<dyn Stream>, committer: Arc<Committer>, pool: &ThreadPool) {
        let _ = pool.execute(move || {
            let mut data: Vec<u8> = vec![];
            let _ = stream.read_to_end(&mut data);
            stream.write_all(&acknowledge()).ok();
            let message = match deserialize_rmp_to::<Message>(&data) {
                Ok(c) => c,
                Err(_) => { return; }
            };

            let _ = match committer.validate_block_and_commit(message) {
                Ok(()) => committer.registry.send_to_all(data, &NodeRegistryType::Committer),
                Err(err) => committer.logger.log(err.to_string())
            };
        });
    }

    fn validate_block_and_commit(&self, message: Message) -> Result<(), CommitterError> {
        let Some(metadata) = &self.config.environment_metadata.get(&message.env_id)
            else { return Err(CommitterError::MissingEnvironmentMetadata(message.env_id)) };

        let commit = self.validate_transaction_message(&message, &metadata)?;
        let locked_token = self.acquire_token(&commit, &metadata)?;

        let token_clone = locked_token.clone();
        let Ok(token) = token_clone.read()
            else { return Err(CommitterError::TokenPoisoned) };

        match token.validate_block(&commit.proposed_block, &metadata) {
            BlockValidationResult::Err(error) => {
                self.queue_for_reconciliation(commit.proposed_block, locked_token);
                Err(CommitterError::FromValidationError(error))
            },
            BlockValidationResult::Ok => {
                let commit_info = self.get_commit_data(commit, &metadata);
                let _ = self.commit_block(locked_token.clone(), commit_info)?;
                self.router.distribute_token(locked_token);
                Ok(())
            }
        }
    }

    fn validate_transaction_message(&self, message: &Message, metadata: &EnvironmentMetadata)
            -> Result<TransactionCommit, CommitterError> {
        let crypto_provider = match metadata.asym_crypto_provider.lock() {
            Ok(provider) => provider,
            Err(_) => return Err(CommitterError::CryptoProviderPoisoned)
        };

        if !crypto_provider.check_signature(&message.signature, &message.body) {
            return Err(CommitterError::InvalidSignature);
        }

        let Ok(commit) = deserialize_rmp_to::<TransactionCommit>(&message.body) else {
            return Err(CommitterError::MalformedTransactionData)
        };

        let _ = match self.pending_transactions.contains(&commit.trans_id) {
            true => return Err(CommitterError::TransactionInProcess),
            false => self.pending_transactions.insert(commit.trans_id.clone())
        };

        if let Ok(_) = self.data_provider.get_data(&commit.trans_id, &metadata.slush_partition_id) {
            return Err(CommitterError::TransactionAlreadyProcessed);
        }

        Ok(commit)
    }

    fn acquire_token(&self, commit: &TransactionCommit, metadata: &EnvironmentMetadata)
            -> Result<Arc<RwLock<Token>>, CommitterError> {
        match self.data_provider.get_token(&commit.token_id, &metadata.token_partition_id) {
            Ok(token) => Ok(token),
            Err(DataError::DataNotFound) => {
                // TODO: mint new token
                Ok(Arc::new(RwLock::new(Token::new())))
            },
            Err(err) => Err(CommitterError::FromDataError(err))
        }
    }

    fn get_commit_data(&self, commit: TransactionCommit, metadata: &EnvironmentMetadata) -> BlockCommitInfo {
        BlockCommitInfo {
            token_id: commit.token_id.clone(),
            env_id: commit.env_id.clone(),
            is_archiver: self.config.node_registry_types
                .contains(&NodeRegistryType::Archiver),
            env_slush_partition: metadata.slush_partition_id.clone(),
            trans_data: commit
        }
    }

    fn commit_block(&self, token: Arc<RwLock<Token>>, info: BlockCommitInfo)
            -> Result<(), CommitterError> {
        let trans_id = info.trans_data.trans_id.clone();
        match serialize_to_bytes_rmp(&info.trans_data) {
            Err(_) => return Err(CommitterError::MalformedTransactionData),
            Ok(data) => {
                let _ = self.data_provider.save_data(&trans_id, data, &info.env_slush_partition);
            }
        }

        {
            let mut write_token = match token.write() {
                Err(_) => return Err(CommitterError::TokenPoisoned),
                Ok(t) => t
            };

            if write_token.has_reached_max_chain_length() && !info.is_archiver {
                write_token.blockchain.remove_block();
            }

            let block = info.trans_data.proposed_block;
            write_token.blockchain.add_block(block);
        }

        match self.data_provider.save_token(&info.token_id, token, &info.env_id) {
            Err(err) => Err(CommitterError::FromDataError(err)),
            Ok(_) => {
                self.pending_transactions.remove(&trans_id);
                Ok(())
            }
        }
    }

    fn queue_for_reconciliation(&self, block: Block, token: Arc<RwLock<Token>>) {
        todo!()
    }
}

#[derive(Debug)]
pub enum CommitterError {
    MissingEnvironmentMetadata(String),
    InvalidSignature,
    CryptoProviderPoisoned,
    TokenPoisoned,
    FromDataError(DataError),
    MalformedTransactionData,
    TransactionInProcess,
    TransactionAlreadyProcessed,
    FromValidationError(BlockValidationError),
}

impl CommitterError {

}

impl Display for CommitterError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl Error for CommitterError {}