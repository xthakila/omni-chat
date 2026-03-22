//! OmniChat CEF helper process.
//!
//! This binary is launched by CEF for renderer, GPU, and utility processes.
//! For renderer processes, it sets up the RendererSideRouter so that
//! `window.cefQuery()` is available in web pages.

use cef::wrapper::message_router::*;
use cef::{args::Args, *};
use std::sync::Arc;

fn main() {
    let args = Args::new();

    #[cfg(target_os = "macos")]
    let _loader = {
        let loader = library_loader::LibraryLoader::new(&std::env::current_exe().unwrap(), true);
        assert!(loader.load());
        loader
    };

    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);

    let mut app = OmniChatHelperApp::new();

    execute_process(
        Some(args.as_main_args()),
        Some(&mut app),
        std::ptr::null_mut(),
    );
}

wrap_app! {
    struct OmniChatHelperApp;

    impl App {
        fn render_process_handler(&self) -> Option<RenderProcessHandler> {
            // Create the renderer-side message router.
            let config = MessageRouterConfig::default();
            let router = RendererSideRouter::new(config);
            Some(OmniChatRenderProcessHandler::new(router))
        }
    }
}

wrap_render_process_handler! {
    struct OmniChatRenderProcessHandler {
        router: Arc<RendererSideRouter>,
    }

    impl RenderProcessHandler {
        fn on_context_created(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            context: Option<&mut V8Context>,
        ) {
            self.router.on_context_created(
                browser.cloned(),
                frame.cloned(),
                context.cloned(),
            );
        }

        fn on_context_released(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            context: Option<&mut V8Context>,
        ) {
            self.router.on_context_released(
                browser.cloned(),
                frame.cloned(),
                context.cloned(),
            );
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
                Some(source_process),
                message.cloned(),
            );
            if handled { 1 } else { 0 }
        }
    }
}
