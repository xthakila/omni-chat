use cef::wrapper::message_router::*;
use cef::*;
use std::sync::Arc;

use crate::app::SharedState;
use crate::handlers;

pub struct OmniChatClient;

impl OmniChatClient {
    pub fn new_client(state: SharedState, router: Arc<BrowserSideRouter>) -> Client {
        OmniChatServiceClient::new(
            state.clone(),
            state.clone(),
            state.clone(),
            state,
            router,
        )
    }

    pub fn new_sidebar_client(state: SharedState, router: Arc<BrowserSideRouter>) -> Client {
        OmniChatSidebarClient::new(state.clone(), state.clone(), state, router)
    }
}

// --- Service client ---

wrap_client! {
    pub struct OmniChatServiceClient {
        life_span_state: SharedState,
        load_state: SharedState,
        display_state: SharedState,
        request_state: SharedState,
        router: Arc<BrowserSideRouter>,
    }

    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(handlers::life_span::ServiceLifeSpanHandler::new(
                self.life_span_state.clone(),
            ))
        }

        fn load_handler(&self) -> Option<LoadHandler> {
            Some(handlers::load::ServiceLoadHandler::new(
                self.load_state.clone(),
            ))
        }

        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(handlers::display::ServiceDisplayHandler::new(
                self.display_state.clone(),
            ))
        }

        fn request_handler(&self) -> Option<RequestHandler> {
            Some(handlers::request::ServiceRequestHandler::new(
                self.request_state.clone(),
            ))
        }

        fn on_process_message_received(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            source_process: ProcessId,
            message: Option<&mut ProcessMessage>,
        ) -> i32 {
            let handled = self.router.on_process_message_received(
                browser.cloned(),
                frame.cloned(),
                source_process,
                message.cloned(),
            );
            if handled { 1 } else { 0 }
        }
    }
}

// --- Sidebar client ---

wrap_client! {
    pub struct OmniChatSidebarClient {
        life_span_state: SharedState,
        load_state: SharedState,
        request_state: SharedState,
        router: Arc<BrowserSideRouter>,
    }

    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(handlers::life_span::SidebarLifeSpanHandler::new(
                self.life_span_state.clone(),
            ))
        }

        fn load_handler(&self) -> Option<LoadHandler> {
            Some(handlers::load::SidebarLoadHandler::new(
                self.load_state.clone(),
            ))
        }

        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(handlers::display::ServiceDisplayHandler::new(
                self.life_span_state.clone(),
            ))
        }

        fn request_handler(&self) -> Option<RequestHandler> {
            Some(handlers::request::ServiceRequestHandler::new(
                self.request_state.clone(),
            ))
        }

        fn on_process_message_received(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            source_process: ProcessId,
            message: Option<&mut ProcessMessage>,
        ) -> i32 {
            let handled = self.router.on_process_message_received(
                browser.cloned(),
                frame.cloned(),
                source_process,
                message.cloned(),
            );
            if handled { 1 } else { 0 }
        }
    }
}
