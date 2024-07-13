use std::sync::Arc;
use std::thread;
use pneumatic_core::conns::{ConnFactory, TcpConnFactory};
use pneumatic_core::data::{DataProvider, DefaultDataProvider};
use pneumatic_core::node::{NodeRegistry, NodeRegistryType};
use pneumatic_committer::actions::ActionRouter;
use pneumatic_committer::Committer;

fn main() {
    let registry_conn_factory: Box<dyn ConnFactory> = Box::new(TcpConnFactory::new());
    let registry = Arc::new(NodeRegistry::init(registry_conn_factory));
    let updates_registry = registry.clone();
    let updates_thread = thread::spawn(move || {
        NodeRegistry::listen_for_updates(updates_registry, &NodeRegistryType::Committer);
    });

    let router = Arc::new(ActionRouter::init());
    let registering_router = router.clone();
    let router_thread = thread::spawn(move || {
        ActionRouter::listen_for_registrations(registering_router);
    });

    let data_provider: Arc<Box<dyn DataProvider>> = Arc::new(Box::new(DefaultDataProvider {}));
    let conn_factory: Arc<Box<dyn ConnFactory>> = Arc::new(Box::new(TcpConnFactory::new()));
    let blocks_thread = thread::spawn(move || {
        Committer::listen_for_new_blocks(router, registry, data_provider, conn_factory);
    });

    let _ = updates_thread.join();
    let _ = router_thread.join();
    let _ = blocks_thread.join();
}