pub mod actions;

use std::io::{BufReader, Write};
use std::net::{SocketAddr, TcpListener};
use std::ops::Deref;
use std::sync::{Arc, RwLock};
use pneumatic_core::config::Config;
use pneumatic_core::{conns, messages, server, node::*, transactions::*};
use pneumatic_core::data::{DataError, DataProvider};
use pneumatic_core::encoding::deserialize_rmp_to;
use pneumatic_core::environment::EnvironmentMetadata;
use pneumatic_core::messages::Message;
use pneumatic_core::tokens::*;
use crate::actions::ActionRouter;

pub struct Committer {
    config: Config,
    router: Arc<ActionRouter>,
    registry: Arc<NodeRegistry>
}

impl Committer {
    const BLOCK_LISTENER_THREAD_COUNT: usize = 10;

    pub fn init(config: Config, router: Arc<ActionRouter>, registry: Arc<NodeRegistry>) -> Self {
        Committer {
            config,
            router,
            registry
        }
    }

    pub fn listen_for_new_blocks(router: Arc<ActionRouter>, registry: Arc<NodeRegistry>) {
        let config = Config::build()
            .expect("Couldn't build config for committer");

        let ip = config.ip_address;
        let addr = SocketAddr::new(ip, conns::COMMITTER_PORT);
        let listener = TcpListener::bind(addr)
            .expect("Couldn't set up external TCP listener for new blocks");
        let thread_pool = server::ThreadPool::build(Self::BLOCK_LISTENER_THREAD_COUNT)
            .expect("Couldn't establish thread pool for committing new blocks");

        let committer = Arc::new(Committer::init(config, router, registry));

        for stream in listener.incoming() {
            let _ = match stream {
                Err(_) => continue,
                Ok(mut stream) => {
                    let clone = committer.clone();
                    let _ = thread_pool.execute(move || {
                        let buf_reader = BufReader::new(&mut stream);
                        let raw_data = buf_reader.buffer().to_vec();
                        let message = match deserialize_rmp_to::<Message>(&raw_data) {
                            Ok(c) => c,
                            Err(_) => {
                                stream.write_all(&messages::acknowledge()).ok();
                                return;
                            }
                        };

                        stream.write_all(&messages::acknowledge()).ok();
                        let _ = match clone.handle_commit(message) {
                            Ok(()) => return,
                            Err(err) => return // TODO: log error
                        };
                    });
                }
            };
        }
    }

    pub fn handle_commit(&self, message: Message) -> Result<(), CommitterError> {
        let Some(metadata) = &self.config.environment_metadata.get(&message.chain_id)
            else { return Err(CommitterError::MissingEnvironmentMetadata(message.chain_id)) };

        let commit = Self::validate_transaction_message(&message, &metadata)?;
        let locked_token = Self::acquire_token(&commit, &metadata)?;

        let token_clone = locked_token.clone();
        let Ok(token) = token_clone.read()
            else { return Err(CommitterError::TokenPoisoned) };

        match token.validate_block(&commit.proposed_block, &metadata) {
            BlockValidationResult::Err(error) => {
                // TODO: queue block for reconciliation at end of epoch
                Err(CommitterError::FromValidationError(error))
            },
            BlockValidationResult::Ok => {
                self.router.distribute_token(locked_token.clone());

                let commit_info = BlockCommitInfo {
                    token_id: commit.token_id.clone(),
                    env_id: commit.env_id.clone(),
                    is_archiver: self.config.node_registry_types
                        .contains(&NodeRegistryType::Archiver),
                    env_slush_partition: metadata.slush_partition_id.clone(),
                    trans_data: commit
                };

                Token::commit_block(locked_token, commit_info)
                    .or_else(|err| Err(CommitterError::FromCommitError(err)))
            }
        }
    }

    fn validate_transaction_message(message: &Message, metadata: &EnvironmentMetadata)
            -> Result<TransactionCommit, CommitterError> {
        let crypto_provider = match metadata.asym_crypto_provider.lock() {
            Ok(provider) => provider,
            Err(_) => return Err(CommitterError::CryptoProviderPoisoned)
        };

        if !crypto_provider.check_signature(&message.signature, &message.body) {
            return Err(CommitterError::InvalidSignature);
        }

        match deserialize_rmp_to::<TransactionCommit>(&message.body) {
            Ok(commit) => Ok(commit),
            Err(_) => Err(CommitterError::MalformedTransactionData)
        }
    }

    fn acquire_token(commit: &TransactionCommit, metadata: &EnvironmentMetadata)
            -> Result<Arc<RwLock<Token>>, CommitterError> {
        match DataProvider::get_token(&commit.token_id, &metadata.token_partition_id) {
            Ok(token) => Ok(token),
            Err(DataError::DataNotFound) => {
                // TODO: mint new token
                Ok(Arc::new(RwLock::new(Token::new())))
            },
            Err(err) => Err(CommitterError::FromDataError(err))
        }
    }
}

pub enum CommitterError {
    MissingEnvironmentMetadata(String),
    InvalidSignature,
    CryptoProviderPoisoned,
    TokenPoisoned,
    FromDataError(DataError),
    MalformedTransactionData,
    FromValidationError(BlockValidationError),
    FromCommitError(BlockCommitError)
}