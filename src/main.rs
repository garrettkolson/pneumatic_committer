use std::sync::Arc;
use std::thread;
use pneumatic_core::config::Config;
use pneumatic_core::node::NodeRegistry;
use pneumatic_committer::actions::ActionRouter;
use pneumatic_committer::Committer;

fn main() {
    let config = Config::build()
        .expect("Couldn't build config for committer");

    let registry = Arc::new(NodeRegistry::init());
    let router = ActionRouter::init();
    let committer = Arc::new(Committer::init(config, router, registry.clone()));

    let updates_registry = registry.clone();
    let updates_thread = thread::spawn(|| {
        listen_for_internal_updates(updates_registry);
    });

    let blocks_thread = thread::spawn(|| {
        listen_for_new_blocks();
    });

    let _ = updates_thread.join();
    let _ = blocks_thread.join();
}

fn listen_for_internal_updates(registry: Arc<NodeRegistry>) {
    // TODO: this will listen for NodeRegistry updates
    // TODO: should this also listen for ActionRouter registration too, or have that on a separate server?
}

fn listen_for_new_blocks() {
    // TODO: should this also listen for ActionRouter registration too, or have that on a separate server?
}
