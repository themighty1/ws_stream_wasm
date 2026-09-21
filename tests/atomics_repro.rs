#![cfg(target_feature = "atomics")]
#![allow(dead_code)]
wasm_bindgen_test_configure!(run_in_browser);

// Reproduces the "message arrives before WsStream is constructed" race under
// the multithreaded wasm-bindgen-futures driver (target_feature = "atomics").
//
// With atomics the driver's wake path can cross a macrotask boundary (the
// helper-worker postMessage fallback), so WsMeta::connect may resolve before
// onmessage is attached to the underlying WebSocket. A server that greets
// immediately (sends data before the client writes anything) then has its
// first frame silently dropped by the browser.
//
// To force that boundary deterministically instead of incidentally, we remove
// Atomics.waitAsync before any task sleeps, which makes the driver use its
// worker postMessage fallback.
use futures::prelude::*;
use log::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use wasm_bindgen_test::*;
use ws_stream_wasm::*;

const URL: &str = "ws://127.0.0.1:3313/";

fn force_worker_wake_path() {
    let atomics = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("Atomics"))
        .expect_throw("Atomics global");
    let atomics = js_sys::Object::from(atomics);
    let _ = js_sys::Reflect::delete_property(&atomics, &JsValue::from_str("waitAsync"));
    info!("deleted Atomics.waitAsync (forcing helper-worker wake path)");
}

// Report the page's shared-memory/cross-origin-isolation state. If
// crossOriginIsolated is false or SharedArrayBuffer is missing, the atomics
// module cannot have shared memory and we get "not a shared typed array".
#[wasm_bindgen_test]
async fn diag_shared_memory() {
    let _ = console_log::init_with_level(Level::Trace);
    let expr = js_sys::eval(
        r#"( function () {
            return {
                coi: (typeof crossOriginIsolated !== 'undefined') ? crossOriginIsolated : 'undefined',
                sab: (typeof SharedArrayBuffer !== 'undefined') ? 'yes' : 'no',
                wai: (typeof Atomics !== 'undefined' && typeof Atomics.waitAsync !== 'undefined') ? 'yes' : 'no',
                coop: performance.getEntriesByType ? 'n/a' : 'n/a',
            };
        } )()"#,
    )
    .expect_throw("eval");
    let s = js_sys::JSON::stringify(&expr).expect_throw("stringify").as_string().unwrap();
    web_sys::console::log_1(&JsValue::from_str(&format!("DIAG_SHARED {s}")));
    info!("DIAG_SHARED {s}");
}

// Control: does the multithread (atomics) harness itself complete a trivial
// async test under this headless setup? If this hangs, the hang in the ws test
// below is a harness/infra problem, not the websocket race.
#[wasm_bindgen_test]
async fn sanity_microtask() {
    let _ = console_log::init_with_level(Level::Trace);
    info!("starting sanity_microtask");
    futures::future::ready(()).await;
    info!("sanity_microtask completed");
}

// Stronger control: sleep across a real macrotask (setTimeout) and require a
// genuine wake. If the atomics wake path (Atomics.notify -> waitAsync) is
// broken under this headless setup, this hangs too, meaning a hang in the ws
// test is a harness limitation, not the websocket race.
#[wasm_bindgen_test]
async fn sanity_timer_wake() {
    let _ = console_log::init_with_level(Level::Trace);
    info!("starting sanity_timer_wake");
    let (tx, rx) = futures::channel::oneshot::channel();
    let mut tx = Some(tx);
    let f = js_sys::Function::new_with_args("cb", "setTimeout(()=>cb(), 50);");
    let _ = f.call1(&JsValue::UNDEFINED, &Closure::once_into_js(move || {
        if let Some(t) = tx.take() {
            let _ = t.send(());
        }
    }));
    rx.await.expect_throw("timer wake");
    info!("sanity_timer_wake completed");
}

#[wasm_bindgen_test]
async fn greeter_sentinel_not_lost() {
    let _ = console_log::init_with_level(Level::Trace);
    info!("starting test: greeter_sentinel_not_lost");

    // Force the multithread driver onto its helper-worker postMessage wake
    // path (a real macrotask boundary) so the connect -> WsStream gap is
    // deterministic rather than incidental.
    force_worker_wake_path();

    let (_ws, mut wsio) = WsMeta::connect(URL, None).await.expect_throw("connect");
    info!("connected, waiting for sentinel");

    let msg = wsio.next().await.expect_throw("stream closed before sentinel");
    info!("received message: {:?}", msg);

    assert_eq!(
        WsMessage::Binary(vec![0x51]),
        msg,
        "expected the greeter's sentinel; if this hangs, the first frame was dropped pre-WsStream"
    );
}