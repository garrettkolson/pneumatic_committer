use std::sync::{Arc, RwLock};
use pneumatic_core::tokens::Token;

pub struct ActionRouter {

}

impl ActionRouter {
    pub fn init() -> Self {
        ActionRouter {}
    }

    pub fn listen_for_registrations(router: Arc<ActionRouter>) {

    }

    pub fn distribute_token(&self, token: Arc<RwLock<Token>>) {

    }
}