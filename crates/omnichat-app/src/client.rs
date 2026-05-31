use cef::wrapper::message_router::*;
use cef::*;
use std::sync::Arc;

use crate::app::SharedState;
use crate::handlers;

pub struct OmniChatClient;

impl OmniChatClient {
    /// A service/content client with no pre-assigned id (used for the default
    /// client, the welcome page, and popups). on_after_created falls back to the
    /// FIFO/URL matching for these.
    pub fn new_client(state: SharedState, router: Arc<BrowserSideRouter>) -> Client {
        OmniChatServiceClient::new(
            state.clone(),
            state.clone(),
            state.clone(),
            state,
            router,
            None,
        )
    }

    /// A client that knows exactly which id its browser should register under.
    /// This makes browser→id association robust against CEF's async creation
    /// order (the FIFO queue races when, e.g., the overlay's small data: URI
    /// realizes before a queued service browser).
    pub fn new_client_for(
        state: SharedState,
        router: Arc<BrowserSideRouter>,
        intended_id: &str,
    ) -> Client {
        OmniChatServiceClient::new(
            state.clone(),
            state.clone(),
            state.clone(),
            state,
            router,
            Some(intended_id.to_string()),
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
        intended_id: Option<String>,
    }

    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(handlers::life_span::ServiceLifeSpanHandler::new(
                self.life_span_state.clone(),
                self.intended_id.clone(),
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
