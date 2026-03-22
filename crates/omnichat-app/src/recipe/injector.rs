use crate::recipe::model::Recipe;
use crate::service::config::ServiceConfig;
use std::path::Path;

const FERDIUM_API_SHIM: &str = include_str!("shim.js");

/// Build composite JS to inject into a service browser on page load.
pub fn build_injection_js(
    service_id: &str,
    service: &ServiceConfig,
    recipe: &Recipe,
) -> String {
    let mut js = String::with_capacity(32768);

    js.push_str("if(!window.__omnichat_injected){window.__omnichat_injected=true;\n");

    let shim = FERDIUM_API_SHIM
        .replace("__SERVICE_ID__", &js_escape(service_id))
        .replace("__SERVICE_NAME__", &js_escape(&service.name))
        .replace("__RECIPE_ID__", &js_escape(&service.recipe_id));
    js.push_str(&shim);
    js.push('\n');

    js.push_str(&notification_patch());
    js.push('\n');

    let service_css = read_file(&recipe.path, "service.css");
    if let Some(ref css) = service_css {
        js.push_str(&css_injection(css));
        js.push('\n');
    }

    if let Some(ref webview_js) = recipe.webview_js {
        js.push_str(&recipe_module(
            webview_js,
            service_id,
            service,
            recipe,
            service_css.is_some(),
        ));
        js.push('\n');
    }

    if service.is_dark_mode_enabled {
        if let Some(ref css) = recipe.darkmode_css {
            js.push_str(&css_injection(css));
            js.push('\n');
        }
    }

    js.push_str("}\n");
    js
}

fn recipe_module(
    webview_js: &str,
    service_id: &str,
    service: &ServiceConfig,
    recipe: &Recipe,
    css_pre_injected: bool,
) -> String {
    let config = serde_json::json!({
        "id": service_id,
        "name": service.name,
        "team": service.team,
        "customUrl": service.custom_url,
        "recipe": {
            "id": recipe.id,
            "name": recipe.name,
            "version": recipe.version,
            "path": recipe.path,
        },
        "isDarkModeEnabled": service.is_dark_mode_enabled,
        "isNotificationEnabled": service.is_notification_enabled,
        "isMuted": service.is_muted,
    });

    let dirname = js_escape(&recipe.path);
    let file_cache = file_cache_json(&recipe.path);

    // The css_fallback controls what happens when injectCSS gets a path not in cache.
    let css_fb = if css_pre_injected {
        "/* already injected */"
    } else {
        "console.warn('[OmniChat] CSS not cached:', fn_)"
    };

    // Build the module wrapper using string concatenation.
    // Cannot use format!() because webview_js contains { } braces (it's JavaScript).
    let mut js = String::with_capacity(webview_js.len() + 4096);
    js.push_str("(function() {\n");
    js.push_str("var module = { exports: {} };\n");
    js.push_str("var exports = module.exports;\n");
    js.push_str(&format!("var __dirname = '{}';\n", dirname));
    js.push_str(&format!("var __filename = '{}/webview.js';\n", dirname));
    js.push_str("function _interopRequireDefault(o) { return o && o.__esModule ? o : { default: o }; }\n");
    js.push_str("var _fc = ");
    js.push_str(&file_cache);
    js.push_str(";\n");
    js.push_str(r#"var _pm = {
    join: function() { return [].slice.call(arguments).join('/'); },
    basename: function(p) { return p.split('/').pop(); },
    dirname: function(p) { var x = p.split('/'); x.pop(); return x.join('/'); },
    extname: function(p) { var m = p.match(/\.[^.]+$/); return m ? m[0] : ''; },
};
_pm.default = _pm;
var require = function(n) {
    if (n === 'path') return _pm;
    if (n === 'fs' || n === 'fs-extra') return {
        existsSync: function(p) { return !!_fc[p.split('/').pop()]; },
        readFileSync: function(p) { return _fc[p.split('/').pop()] || ''; },
        pathExistsSync: function(p) { return !!_fc[p.split('/').pop()]; },
    };
    console.warn('[OmniChat] require("' + n + '") unavailable');
    return {};
};
var _oCSS = window.__omnichat_ferdium.injectCSS;
window.__omnichat_ferdium.injectCSS = function() {
    [].slice.call(arguments).forEach(function(a) {
        if (typeof a !== 'string' || !a) return;
        if (a.indexOf('/') > -1 || a.indexOf('\\') > -1) {
            var fn_ = a.split('/').pop().split('\\').pop();
            if (_fc[fn_]) {
                var s = document.createElement('style');
                s.textContent = _fc[fn_];
                (document.head || document.documentElement).appendChild(s);
"#);
    js.push_str("            } else { ");
    js.push_str(css_fb);
    js.push_str(" }\n");
    js.push_str(r#"        } else { _oCSS.call(window.__omnichat_ferdium, a); }
    });
};
window.Ferdium.injectCSS = window.__omnichat_ferdium.injectCSS;
window.__omnichat_ferdium.injectJSUnsafe = function() {
    [].slice.call(arguments).forEach(function(a) {
        if (typeof a !== 'string') return;
        var fn_ = a.split('/').pop().split('\\').pop();
        var code = _fc[fn_];
        if (code) {
            var script = document.createElement('script');
            script.textContent = code;
            document.documentElement.appendChild(script);
            script.remove();
        }
    });
};
window.Ferdium.injectJSUnsafe = window.__omnichat_ferdium.injectJSUnsafe;
try {
"#);
    js.push_str(webview_js);
    js.push_str("\nvar rf = module.exports;\n");
    let config_str = config.to_string();
    js.push_str("if (typeof rf === 'function') rf(window.__omnichat_ferdium, ");
    js.push_str(&config_str);
    js.push_str(");\n");
    js.push_str("else if (rf && typeof rf.default === 'function') rf.default(window.__omnichat_ferdium, ");
    js.push_str(&config_str);
    js.push_str(");\n");
    js.push_str("} catch(e) { console.error('[OmniChat] Recipe error:', e); }\n");
    js.push_str("})();\n");
    js
}

fn file_cache_json(recipe_path: &str) -> String {
    let dir = Path::new(recipe_path);
    let mut m = serde_json::Map::new();
    for f in &["service.css", "darkmode.css", "webview-unsafe.js", "user.css", "user.js"] {
        let p = dir.join(f);
        if let Ok(c) = std::fs::read_to_string(&p) {
            m.insert(f.to_string(), serde_json::Value::String(c));
        }
    }
    serde_json::to_string(&m).unwrap_or_else(|_| "{}".into())
}

fn read_file(recipe_path: &str, name: &str) -> Option<String> {
    std::fs::read_to_string(Path::new(recipe_path).join(name)).ok()
}

fn notification_patch() -> String {
    // The notification patch reads window.__omnichat_ferdium._serviceId
    // which was set by the shim. This ensures background service notifications
    // include the correct service identifier.
    r#"(function() {
    var ON = window.Notification;
    class N {
        static permission = 'granted';
        constructor(t, o) {
            t = t || ''; o = o || {};
            this._t = t; this._o = o; this._oc = null;
            var sid = (window.__omnichat_ferdium && window.__omnichat_ferdium._serviceId) || '';
            if (window.cefQuery) {
                window.cefQuery({
                    request: JSON.stringify({type:'notification',serviceId:sid,title:t,body:o.body||'',icon:o.icon||'',tag:o.tag||'',silent:o.silent||false}),
                    onSuccess: function() { if (this._oc) this._oc(); }.bind(this),
                    onFailure: function() {},
                });
            }
        }
        static requestPermission(cb) { if (typeof cb==='function') cb('granted'); return Promise.resolve('granted'); }
        close() { this._oc = null; }
        set onclick(c) { this._oc = c; }
        get onclick() { return this._oc; }
    }
    if (ON) Object.setPrototypeOf(N.prototype, ON.prototype);
    window.Notification = N;
    if (window.ServiceWorkerRegistration) {
        window.ServiceWorkerRegistration.prototype.showNotification = function(t, o) {
            new N(t, {body:(o&&o.body)||''});
        };
    }
})();
"#.to_string()
}

fn css_injection(css: &str) -> String {
    let e = css.replace('\\', "\\\\").replace('\'', "\\'").replace('\n', "\\n").replace('\r', "");
    let mut r = String::from("(function(){var s=document.createElement('style');s.textContent='");
    r.push_str(&e);
    r.push_str("';(document.head||document.documentElement).appendChild(s);})();\n");
    r
}

fn js_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'").replace('"', "\\\"").replace('\n', "\\n").replace('\r', "")
}
