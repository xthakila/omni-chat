// OmniChat Ferdium API Shim
// Injected into each service webview to provide Ferdium-compatible API.
// Template variables __SERVICE_ID__, __SERVICE_NAME__, __RECIPE_ID__ are
// replaced at injection time by Rust.

(function() {
    'use strict';

    var serviceId = '__SERVICE_ID__';
    var serviceName = '__SERVICE_NAME__';
    var recipeId = '__RECIPE_ID__';

    // Internal state
    var _loopFn = null;
    var _onNotifyFn = null;
    var _darkModeHandler = null;
    var _toggleToTalkFn = null;

    // Send IPC message to Rust. Uses cefQuery if available, falls back to URL scheme.
    function sendIPC(msg) {
        var json = JSON.stringify(msg);
        if (window.cefQuery) {
            window.cefQuery({
                request: json,
                onSuccess: function() {},
                onFailure: function(code, message) {
                    console.warn('[OmniChat IPC] cefQuery failed:', code, message);
                    // Fallback on failure.
                    sendIPCviaURL(json);
                },
            });
        } else {
            sendIPCviaURL(json);
        }
    }

    function sendIPCviaURL(json) {
        var encoded = encodeURIComponent(json);
        var iframe = document.createElement('iframe');
        iframe.style.display = 'none';
        iframe.src = 'omnichat-ipc://' + encoded;
        document.body.appendChild(iframe);
        setTimeout(function() { if (iframe.parentNode) iframe.parentNode.removeChild(iframe); }, 100);
    }

    // Helper: safely parse integer.
    function safeParseInt(val) {
        if (val === null || val === undefined || val === '' || val === 'null' || val === 'undefined') {
            return 0;
        }
        if (typeof val === 'number') return Math.floor(val);
        // Strip non-numeric chars (e.g. "3+" → 3).
        var cleaned = String(val).replace(/[^0-9-]/g, '');
        var parsed = parseInt(cleaned, 10);
        return isNaN(parsed) ? 0 : parsed;
    }

    // The Ferdium-compatible API object.
    var Ferdium = {
        // Set unread message badge counts.
        setBadge: function(direct, indirect) {
            direct = safeParseInt(direct);
            indirect = safeParseInt(indirect);
            sendIPC({
                type: 'badge',
                serviceId: serviceId,
                direct: direct,
                indirect: indirect,
            });
        },

        // Set the active dialog title (e.g. contact name in WhatsApp).
        setDialogTitle: function(title) {
            sendIPC({
                type: 'dialog_title',
                serviceId: serviceId,
                title: title || '',
            });
        },

        // Safely parse an integer value.
        safeParseInt: safeParseInt,

        // Check if a link element points to an image.
        isImage: function(link) {
            if (!link) return false;
            if (link.dataset && link.dataset.role === 'img') return true;
            var url = link.getAttribute && link.getAttribute('href');
            if (!url) return false;
            return /\.(jpg|jpeg|png|webp|avif|gif|svg)($|\?|:)/.test(url.split(/[#?]/)[0]);
        },

        // Inject CSS from a string.
        injectCSS: function() {
            var args = Array.prototype.slice.call(arguments);
            args.forEach(function(css) {
                if (typeof css === 'string' && css.length > 0) {
                    var style = document.createElement('style');
                    style.textContent = css;
                    (document.head || document.documentElement).appendChild(style);
                }
            });
        },

        // Register the polling loop function.
        loop: function(fn) {
            if (typeof fn === 'function') {
                _loopFn = fn;
            }
        },

        // Register notification callback (can modify notification data before display).
        onNotify: function(fn) {
            if (typeof fn === 'function') {
                _onNotifyFn = fn;
            }
        },

        // Run an initialization function.
        initialize: function(fn) {
            if (typeof fn === 'function') {
                try { fn(); } catch(e) { console.error('[OmniChat] initialize() error:', e); }
            }
        },

        // Handle dark mode toggling.
        handleDarkMode: function(handler) {
            if (typeof handler === 'function') {
                _darkModeHandler = handler;
            }
        },

        // Set avatar image.
        setAvatarImage: function(url) {
            sendIPC({
                type: 'avatar',
                serviceId: serviceId,
                url: url || '',
            });
        },

        // Open a URL in the system browser.
        openNewWindow: function(url) {
            sendIPC({
                type: 'open_url',
                url: url || '',
            });
        },

        // Push-to-talk toggle (used by Discord recipe).
        toggleToTalk: function(fn) {
            if (typeof fn === 'function') {
                _toggleToTalkFn = fn;
            }
        },

        // Stub for clearStorageData (not needed without Electron).
        clearStorageData: function() {},

        // Stub for releaseServiceWorkers.
        releaseServiceWorkers: function() {},

        // Stub for injectJSUnsafe.
        injectJSUnsafe: function() {},

        // Internal: called by the Rust poll timer.
        _loopFn: null,
        _onNotifyFn: null,
    };

    // Internal accessors (used by poll timer and notification patch).
    Object.defineProperty(Ferdium, '_loopFn', {
        get: function() { return _loopFn; },
        enumerable: false,
    });
    Object.defineProperty(Ferdium, '_onNotifyFn', {
        get: function() { return _onNotifyFn; },
        enumerable: false,
    });
    Object.defineProperty(Ferdium, '_serviceId', {
        value: serviceId,
        enumerable: false,
    });

    // Expose globally.
    window.__omnichat_ferdium = Ferdium;
    window.Ferdium = Ferdium;
})();
