pub mod actions;

use std::io::{BufReader, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::Arc;
use pneumatic_core::config::Config;
use pneumatic_core::{conns, messages, server};
use pneumatic_core::encoding::deserialize_rmp_to;
use pneumatic_core::node::*;
use pneumatic_core::transactions::*;
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
                        stream.write_all(&messages::acknowledge()).ok();
                        let buf_reader = BufReader::new(&mut stream);
                        let raw_data = buf_reader.buffer().to_vec();
                        let Ok(commit) = deserialize_rmp_to::<TransactionCommit>(&raw_data)
                            else { return };

                        clone.handle_commit(commit);
                    });
                }
            };
        }
    }

    pub fn handle_commit(&self, commit: TransactionCommit) {

    }
}

//use std::sync::Arc;
// use dashmap::mapref::entry::{OccupiedEntry, VacantEntry};
// use dashmap::mapref::entry::Entry::{Occupied, Vacant};
// use serde_json::error::Category::Data;
// use crate::config::Config;
// use crate::data::crypto::{AsymCryptoProviderType, RsaCryptoProvider};
// use crate::data::encoding::DataSerializer;
// use crate::message::Message;
// use crate::node_functions::IsNodeFunction;
// use crate::node_functions::server::ActionRouter;
// use crate::transactions::TransactionCommit;
//
// // TODO: can we try to make all the node functions stateless?
//
// pub struct Committer {
//
// }
//
// impl Committer {
//     // pub fn new(
//     //     config: &Config,
//     //     router: Arc<ActionRouter>) -> Committer {
//     //     Committer {}
//     // }
//
//     pub async fn handle_data_received(config: &Config, router: &Arc<ActionRouter>, data: Vec<u8>) {
//         let message = match DataSerializer::deserialize_rmp_to::<Message>(data) {
//             Err(_) => return,
//             Ok(message) => message
//         };
//
//         let metadata_arc = Arc::clone(&config.environment_metadata);
//         let entry = match metadata_arc.entry(message.chain_id.to_string()) {
//             Vacant(vacant) => return,
//             Occupied(entry) => entry
//         };
//
//         let env_metadata = entry.get();
//         let sig_is_valid = match env_metadata.asym_crypto_provider_type {
//             AsymCryptoProviderType::RSA => RsaCryptoProvider::message_has_valid_signature(&message)
//         };
//
//         if !sig_is_valid { return; }
//         if let Ok(commit) = DataSerializer::deserialize_rmp_to::<TransactionCommit>(message.body) {
//             // TODO: figure out how to get the underlying asset for a new token (probably have to pass it in with the transaction)
//             // var token = await metadata.DataProvider.GetTokenAsync<IToken>(result.TokenId) ??
//             //      await TokenFactory.MintToken(new object());
//             // var validationResult = await _blockServices.ValidateBlock(token, proposedBlock, metadata);
//             //
//             // if (validationResult.BlockIsValid)
//             // {
//             //     await _blockServices.CommitBlock(token, metadata, proposedBlock, result);
//             //     await _tokenDistributor.Distribute(token);
//             // }
//             // else
//             //      // save validation result for reconciliation at epoch end
//             //      await _blockServices.QueueBlockForReconciliation(validationResult, metadata);
//         }
//     }
// }
//
// // impl IsNodeFunction for Committer {
// //     fn initialize(&mut self) {
// //         todo!()
// //     }
// //
// //     fn handle_data_received(&mut self, data: Vec<u8>) {
// //         todo!()
// //     }
// // }