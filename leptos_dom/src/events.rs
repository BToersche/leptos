use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashSet;
use wasm_bindgen::{JsCast, JsValue, UnwrapThrowExt};
use wasm_bindgen::convert::FromWasmAbi;
use wasm_bindgen::closure::Closure;
use crate::window;

thread_local! {
    pub static GLOBAL_EVENTS: RefCell<HashSet<Cow<'static, str>>> = RefCell::new(HashSet::new());
}

/// Adds an event listener to the `Window`.
pub fn window_event_listener(event_name: &str, cb: impl Fn(web_sys::Event) + 'static) {
	let handler = Box::new(cb) as Box<dyn FnMut(web_sys::Event)>;

	let cb = Closure::wrap(handler).into_js_value();
	_ = window().add_event_listener_with_callback(event_name, cb.unchecked_ref());
}

/// Adds an event listener to the target DOM element using implicit event delegation.
pub fn add_event_listener<E>(
    target: &web_sys::Element,
    event_name: Cow<'static, str>,
    cb: impl FnMut(E) + 'static,
) where
    E: FromWasmAbi + 'static,
{
    let cb = Closure::wrap(Box::new(cb) as Box<dyn FnMut(E)>).into_js_value();
    let key = event_delegation_key(&event_name);
    _ = js_sys::Reflect::set(target, &JsValue::from_str(&key), &cb);
    add_delegated_event_listener(event_name);
}

#[doc(hidden)]
pub fn add_event_listener_undelegated<E>(
    target: &web_sys::Element,
    event_name: &str,
    cb: impl FnMut(E) + 'static,
) where
    E: FromWasmAbi + 'static,
{
    let cb = Closure::wrap(Box::new(cb) as Box<dyn FnMut(E)>).into_js_value();
    _ = target.add_event_listener_with_callback(&event_name, cb.unchecked_ref());
}

// cf eventHandler in ryansolid/dom-expressions
pub(crate) fn add_delegated_event_listener(event_name: Cow<'static, str>) {
    GLOBAL_EVENTS.with(|global_events| {
        let mut events = global_events.borrow_mut();
        if !events.contains(&event_name) {
            // create global handler
            let key = JsValue::from_str(&event_delegation_key(&event_name));
            let handler = move |ev: web_sys::Event| {
                let target = ev.target();
                let node = ev.composed_path().get(0);
                let mut node = if node.is_undefined() || node.is_null() {
                    JsValue::from(target)
                } else {
                    node
                };

                // TODO reverse Shadow DOM retargetting

                // TODO simulate currentTarget

                while !node.is_null() {
                    let node_is_disabled =
                        js_sys::Reflect::get(&node, &JsValue::from_str("disabled"))
                            .unwrap_throw()
                            .is_truthy();
                    if !node_is_disabled {
                        let maybe_handler = js_sys::Reflect::get(&node, &key).unwrap_throw();
                        if !maybe_handler.is_undefined() {
                            let f = maybe_handler.unchecked_ref::<js_sys::Function>();
                            if let Err(e) = f.call1(&node, &ev) {
                                #[cfg(not(debug_assertions))]
                                {
                                    _ = e;
                                }
                            }

                            if ev.cancel_bubble() {
                                return;
                            }
                        }
                    }

                    // navigate up tree
                    let host =
                        js_sys::Reflect::get(&node, &JsValue::from_str("host")).unwrap_throw();
                    if host.is_truthy() && host != node && host.dyn_ref::<web_sys::Node>().is_some()
                    {
                        node = host;
                    } else if let Some(parent) =
                        node.unchecked_into::<web_sys::Node>().parent_node()
                    {
                        node = parent.into()
                    } else {
                        node = JsValue::null()
                    }
                }
            };

            window_event_listener(&event_name, handler);

            // register that we've created handler
            events.insert(event_name.into());
        }
    })
}

pub(crate) fn event_delegation_key(event_name: &str) -> String {
    let mut n = String::from("$$$");
    n.push_str(&event_name);
    n
}