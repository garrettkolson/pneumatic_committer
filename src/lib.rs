pub mod actions;

use std::io::{BufReader, Write};
use std::net::{SocketAddr, TcpListener};
use std::ops::Deref;
use std::sync::{Arc, RwLock};
use pneumatic_core::config::Config;
use pneumatic_core::{conns, messages, server, node::*, transactions::*};
use pneumatic_core::data::{DataError, DataProvider};
use pneumatic_core::encoding::deserialize_rmp_to;
use pneumatic_core::tokens::{BlockCommitInfo, BlockValidationResult, Token};
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
                        let commit = match deserialize_rmp_to::<TransactionCommit>(&raw_data) {
                            Ok(c) => c,
                            Err(_) => {
                                stream.write_all(&messages::acknowledge()).ok();
                                return;
                            }
                        };

                        stream.write_all(&messages::acknowledge()).ok();
                        clone.handle_commit(commit);
                    });
                }
            };
        }
    }

    pub fn handle_commit(&self, commit: TransactionCommit) {
        let Some(metadata) = &self.config.environment_metadata.get(&commit.env_id)
            else { return };

        // TODO: have to pass correct args here
        if !metadata.asym_crypto_provider.check_signature(vec![], vec![], vec![]) {
            return;
        }

        let locked_token = match DataProvider::get_token(&commit.token_id, &metadata.token_partition_id) {
            Ok(token) => token,
            Err(DataError::DataNotFound) => {
                // TODO: mint new token
                Arc::new(RwLock::new(Token::new()))
            },
            _ => return
        };

        let Ok(token) = locked_token.read() else {
            // TODO: log this error
            return
        };

        match token.validate_block(&commit.proposed_block, &metadata) {
            BlockValidationResult::Err(error) => {
                // TODO: log this error
                return
            }
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

                Token::commit_block(locked_token, commit_info);
            }
        }
    }
}
//
// impl Committer {
//             var validationResult = await token.ValidateBlock(proposedBlock, metadata);
//
//             if (validationResult.BlockIsValid)
//             {
//                 var isArchiver = _nodeConfig.NodeFunctionTypes.Any(type => type == NodeRegistryType.Archiver);
//                 BlockCommitInfo commitInfo = new(metadata, proposedBlock, result, isArchiver);
//                 await token.CommitBlock(commitInfo);
//                 await _tokenDistributor.Distribute(token);
//             }
//             else
//                 // save validation result for reconciliation at epoch end
//                 await _blockServices.QueueBlockForReconciliation(validationResult, metadata);
//         }
//     }
// }
// // TODO: could have self-signed non-Pneuma tokens if they pay fuel for sentinel/committal fees?
//     public async Task<BlockValidationResult> ValidateBlock(Block block, EnvironmentMetadata metadata)
//     {
//         if (!metadata.BlockValidatorSpecs.TryGetValue(BlockValidationSpecName, out var spec))
//             spec = new ExecutedBlockValidatorSpec();
//
//         spec.LoadSpec(metadata, this);
//
//         return await spec.Validate(block);
//     }