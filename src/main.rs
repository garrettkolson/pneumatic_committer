use std::sync::Arc;
use std::thread;
use std::thread::JoinHandle;
use pneumatic_core::conns::{ConnFactory, TcpConnFactory};
use pneumatic_core::data::{DataProvider, DefaultDataProvider};
use pneumatic_core::logging::{FileLogger, Logger};
use pneumatic_core::node::{NodeRegistry, NodeRegistryType};
use pneumatic_committer::actions::ActionRouter;
use pneumatic_committer::Committer;

fn main() {
    let (registry, updates_thread) = init_node_registry();
    let (router, router_thread) = init_action_router();
    let blocks_thread = init_committer(router, registry);

    let _ = updates_thread.join();
    let _ = router_thread.join();
    let _ = blocks_thread.join();
}

fn init_node_registry() -> (Arc<NodeRegistry>, JoinHandle<()>) {
    let registry_conn_factory: Box<dyn ConnFactory> = Box::new(TcpConnFactory::new());
    let registry = Arc::new(NodeRegistry::init(registry_conn_factory));
    let updates_registry = registry.clone();
    let updates_thread = thread::spawn(move || {
        NodeRegistry::listen_for_updates(updates_registry, &NodeRegistryType::Committer);
    });

    (registry, updates_thread)
}

fn init_action_router() -> (Arc<ActionRouter>, JoinHandle<()>) {
    let router = Arc::new(ActionRouter::init());
    let registering_router = router.clone();
    let router_thread = thread::spawn(move || {
        ActionRouter::listen_for_registrations(registering_router);
    });

    (router, router_thread)
}

fn init_committer(router: Arc<ActionRouter>, registry: Arc<NodeRegistry>) -> JoinHandle<()> {
    let data_provider: Arc<Box<dyn DataProvider>> = Arc::new(Box::new(DefaultDataProvider {}));
    let conn_factory: Arc<Box<dyn ConnFactory>> = Arc::new(Box::new(TcpConnFactory::new()));
    let logger: Arc<Box<dyn Logger>> = Arc::new(Box::new(FileLogger::new("committer_log.txt".to_string())));
    thread::spawn(move || {
        Committer::listen_for_new_blocks(router, registry, data_provider, conn_factory, logger);
    })
}