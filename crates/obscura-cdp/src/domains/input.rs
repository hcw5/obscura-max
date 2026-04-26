use serde_json::{json, Value};
use tokio::time::{sleep, Duration};

use crate::dispatch::CdpContext;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Point {
    x: f64,
    y: f64,
}

fn js_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn mouse_button_to_code(button: &str) -> i32 {
    match button {
        "left" => 0,
        "middle" => 1,
        "right" => 2,
        "back" => 3,
        "forward" => 4,
        _ => 0,
    }
}

fn mouse_event_defaults(event_type: &str) -> (&'static str, bool, bool) {
    match event_type {
        "mousePressed" => ("mousedown", true, true),
        "mouseReleased" => ("mouseup", true, true),
        "mouseMoved" => ("mousemove", true, true),
        "mouseWheel" => ("wheel", true, true),
        _ => ("mousemove", true, true),
    }
}

fn touch_event_defaults(event_type: &str) -> (&'static str, bool, bool) {
    match event_type {
        "touchStart" => ("touchstart", true, true),
        "touchMove" => ("touchmove", true, true),
        "touchEnd" => ("touchend", true, true),
        "touchCancel" => ("touchcancel", true, false),
        _ => ("touchmove", true, true),
    }
}

pub fn generate_human_like_trajectory(start: Point, end: Point, steps: usize) -> Vec<Point> {
    if steps <= 1 {
        return vec![end];
    }

    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let length = (dx * dx + dy * dy).sqrt();
    let normal_x = if length > 0.0 { -dy / length } else { 0.0 };
    let normal_y = if length > 0.0 { dx / length } else { 0.0 };
    let tangent_x = if length > 0.0 { dx / length } else { 1.0 };
    let tangent_y = if length > 0.0 { dy / length } else { 0.0 };

    let base_arc = (length * 0.16).clamp(3.0, 42.0);
    let c1 = Point {
        x: start.x + dx * 0.30 + normal_x * base_arc,
        y: start.y + dy * 0.30 + normal_y * base_arc,
    };
    let c2 = Point {
        x: start.x + dx * 0.72 - normal_x * (base_arc * 0.62),
        y: start.y + dy * 0.72 - normal_y * (base_arc * 0.62),
    };

    let mut points = Vec::with_capacity(steps);
    for i in 1..=steps {
        let progress = i as f64 / steps as f64;
        let t = progress * progress * (3.0 - 2.0 * progress);
        let omt = 1.0 - t;
        let base_x = omt * omt * omt * start.x
            + 3.0 * omt * omt * t * c1.x
            + 3.0 * omt * t * t * c2.x
            + t * t * t * end.x;
        let base_y = omt * omt * omt * start.y
            + 3.0 * omt * omt * t * c1.y
            + 3.0 * omt * t * t * c2.y
            + t * t * t * end.y;

        let micro_decay = (1.0 - progress).powf(1.3);
        let micro_normal = ((i as f64) * 2.245 + length * 0.07).sin() * 0.65 * micro_decay;
        let micro_tangent = ((i as f64) * 1.137 + length * 0.03).cos() * 0.28 * micro_decay;
        points.push(Point {
            x: base_x + normal_x * micro_normal + tangent_x * micro_tangent,
            y: base_y + normal_y * micro_normal + tangent_y * micro_tangent,
        });
    }

    if let Some(last) = points.last_mut() {
        *last = end;
    }
    points
}

fn pseudo_uniform(seed: f64) -> f64 {
    let x = (seed.sin() * 43_758.545_312_3).fract();
    x.abs().clamp(1e-9, 1.0 - 1e-9)
}

fn sample_lognormal_delay_ms(
    index: usize,
    mean_ms: f64,
    sigma: f64,
    min_ms: u64,
    max_ms: u64,
) -> u64 {
    let mu = mean_ms.max(1.0).ln() - (sigma * sigma) / 2.0;
    let u1 = pseudo_uniform(index as f64 * 12.9898 + 78.233);
    let u2 = pseudo_uniform(index as f64 * 39.3467 + 11.135);
    let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
    let sampled = (mu + sigma * z).exp();
    sampled.round().clamp(min_ms as f64, max_ms as f64) as u64
}

fn build_mouse_script(
    event_type: &str,
    point: Point,
    button_code: i32,
    click_count: u64,
    buttons: i64,
    delta_x: f64,
    delta_y: f64,
) -> String {
    let (dom_event, bubbles, cancelable) = mouse_event_defaults(event_type);
    let emit_click = event_type == "mouseReleased";

    format!(
        "(function() {{\
            var state = globalThis.__obscura_mouse_state || {{x:0,y:0,target:null,lastDownTarget:null,lastButton:0,lastClickCount:1}};\
            var dispatch = globalThis.__obscura_dispatch_event || function(target, evt) {{ return target.dispatchEvent(evt); }};\
            var clientX = {x};\
            var clientY = {y};\
            var prevX = Number.isFinite(state.x) ? state.x : clientX;\
            var prevY = Number.isFinite(state.y) ? state.y : clientY;\
            var movementX = clientX - prevX;\
            var movementY = clientY - prevY;\
            var target = document.elementFromPoint(clientX, clientY) || document.activeElement || document.body;\
            if (!target) return;\
            var view = target.ownerDocument && target.ownerDocument.defaultView ? target.ownerDocument.defaultView : window;\
            var pageX = clientX + view.scrollX;\
            var pageY = clientY + view.scrollY;\
            if (state.target && state.target !== target) {{\
                dispatch(state.target, new MouseEvent('mouseout', {{bubbles:true,cancelable:true,clientX:clientX,clientY:clientY,pageX:pageX,pageY:pageY,movementX:movementX,movementY:movementY,button:{button_code},buttons:{buttons}}}), {{trusted:true}});\
                dispatch(state.target, new MouseEvent('mouseleave', {{bubbles:false,cancelable:false,clientX:clientX,clientY:clientY,pageX:pageX,pageY:pageY,movementX:movementX,movementY:movementY,button:{button_code},buttons:{buttons}}}), {{trusted:true}});\
                dispatch(target, new MouseEvent('mouseover', {{bubbles:true,cancelable:true,clientX:clientX,clientY:clientY,pageX:pageX,pageY:pageY,movementX:movementX,movementY:movementY,button:{button_code},buttons:{buttons}}}), {{trusted:true}});\
                dispatch(target, new MouseEvent('mouseenter', {{bubbles:false,cancelable:false,clientX:clientX,clientY:clientY,pageX:pageX,pageY:pageY,movementX:movementX,movementY:movementY,button:{button_code},buttons:{buttons}}}), {{trusted:true}});\
            }}\
            var evt = new MouseEvent('{dom_event}', {{bubbles:{bubbles},cancelable:{cancelable},clientX:clientX,clientY:clientY,pageX:pageX,pageY:pageY,movementX:movementX,movementY:movementY,button:{button_code},buttons:{buttons},detail:{click_count}}});\
            dispatch(target, evt, {{trusted:true}});\
            if ('{event_type}' === 'mousePressed') {{\
                state.lastDownTarget = target;\
                state.lastButton = {button_code};\
                state.lastClickCount = {click_count};\
            }}\
            if ({emit_click} && state.lastDownTarget === target) {{\
                dispatch(target, new MouseEvent('click', {{bubbles:true,cancelable:true,clientX:clientX,clientY:clientY,pageX:pageX,pageY:pageY,movementX:movementX,movementY:movementY,button:state.lastButton,buttons:0,detail:state.lastClickCount}}), {{trusted:true}});\
            }}\
            if ('{event_type}' === 'mouseWheel') {{\
                dispatch(target, new WheelEvent('wheel', {{bubbles:true,cancelable:true,clientX:clientX,clientY:clientY,pageX:pageX,pageY:pageY,deltaX:{delta_x},deltaY:{delta_y},deltaMode:0}}), {{trusted:true}});\
            }}\
            state.x = clientX;\
            state.y = clientY;\
            state.target = target;\
            globalThis.__obscura_mouse_state = state;\
            globalThis.__obscura_click_target = target;\
        }})()",
        x = point.x,
        y = point.y,
        button_code = button_code,
        buttons = buttons,
        click_count = click_count,
        dom_event = dom_event,
        bubbles = bubbles,
        cancelable = cancelable,
        event_type = event_type,
        emit_click = emit_click,
        delta_x = delta_x,
        delta_y = delta_y,
    )
}

fn build_touch_script(event_type: &str, touch_points: &[&Value]) -> String {
    let (dom_event, bubbles, cancelable) = touch_event_defaults(event_type);
    let mapped = touch_points
        .iter()
        .map(|p| {
            let x = p.get("x").and_then(Value::as_f64).unwrap_or(0.0);
            let y = p.get("y").and_then(Value::as_f64).unwrap_or(0.0);
            let radius_x = p.get("radiusX").and_then(Value::as_f64).unwrap_or(1.0);
            let radius_y = p.get("radiusY").and_then(Value::as_f64).unwrap_or(1.0);
            let rotation = p
                .get("rotationAngle")
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            let force = p.get("force").and_then(Value::as_f64).unwrap_or(0.5);
            let id = p.get("id").and_then(Value::as_i64).unwrap_or(0);

            format!(
                "{{identifier:{id},clientX:{x},clientY:{y},radiusX:{radius_x},radiusY:{radius_y},rotationAngle:{rotation},force:{force}}}",
                id = id,
                x = x,
                y = y,
                radius_x = radius_x,
                radius_y = radius_y,
                rotation = rotation,
                force = force,
            )
        })
        .collect::<Vec<_>>()
        .join(",");

    format!(
        "(function() {{\
            var doc = document;\
            var dispatch = globalThis.__obscura_dispatch_event || function(target, evt) {{ return target.dispatchEvent(evt); }};\
            var touchesInit = [{mapped}];\
            var touches = touchesInit.map(function(init) {{\
                var target = doc.elementFromPoint(init.clientX, init.clientY) || doc.body;\
                return new Touch({{identifier:init.identifier,target:target,clientX:init.clientX,clientY:init.clientY,pageX:init.clientX + window.scrollX,pageY:init.clientY + window.scrollY,screenX:init.clientX,screenY:init.clientY,radiusX:init.radiusX,radiusY:init.radiusY,rotationAngle:init.rotationAngle,force:init.force}});\
            }});\
            var primaryTarget = touches[0] ? touches[0].target : (doc.activeElement || doc.body);\
            if (!primaryTarget) return;\
            var evt = new TouchEvent('{dom_event}', {{bubbles:{bubbles},cancelable:{cancelable},touches:touches,targetTouches:touches.filter(function(t){{return t.target===primaryTarget;}}),changedTouches:touches}});\
            dispatch(primaryTarget, evt, {{trusted:true}});\
            globalThis.__obscura_touch_target = primaryTarget;\
        }})()",
        mapped = mapped,
        dom_event = dom_event,
        bubbles = bubbles,
        cancelable = cancelable,
    )
}

fn build_keyboard_script(dom_event: &str, key: &str, code: &str) -> String {
    format!(
        "(function() {{\
            var target = document.activeElement || document.body;\
            if (!target) return;\
            var dispatch = globalThis.__obscura_dispatch_event || function(t, e) {{ return t.dispatchEvent(e); }};\
            dispatch(target, new KeyboardEvent('{dom_event}', {{bubbles:true,cancelable:true,key:'{key}',code:'{code}'}}), {{trusted:true}});\
        }})()",
        dom_event = dom_event,
        key = js_escape(key),
        code = js_escape(code),
    )
}

fn build_text_input_script(text: &str) -> String {
    format!(
        "(function() {{\
            var target = document.activeElement;\
            if (!target) return;\
            var dispatch = globalThis.__obscura_dispatch_event || function(t, e) {{ return t.dispatchEvent(e); }};\
            if (target.localName === 'input' || target.localName === 'textarea') {{\
                target.value = (target.value || '') + '{text}';\
                dispatch(target, new Event('input', {{bubbles:true}}), {{trusted:true}});\
            }}\
        }})()",
        text = js_escape(text),
    )
}

pub async fn handle(
    method: &str,
    params: &Value,
    ctx: &mut CdpContext,
    session_id: &Option<String>,
) -> Result<Value, String> {
    match method {
        "dispatchMouseEvent" => {
            let event_type = params.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let x = params.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let y = params.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let button = params
                .get("button")
                .and_then(|v| v.as_str())
                .unwrap_or("left");
            let click_count = params
                .get("clickCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(1);
            let buttons = params.get("buttons").and_then(|v| v.as_i64()).unwrap_or(0);
            let delta_x = params.get("deltaX").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let delta_y = params.get("deltaY").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let steps = params.get("steps").and_then(|v| v.as_u64()).unwrap_or(1) as usize;

            if let Some(page) = ctx.get_session_page_mut(session_id) {
                let button_code = mouse_button_to_code(button);
                let path = if event_type == "mouseMoved" && steps > 1 {
                    let start = Point {
                        x: params.get("fromX").and_then(Value::as_f64).unwrap_or(x),
                        y: params.get("fromY").and_then(Value::as_f64).unwrap_or(y),
                    };
                    generate_human_like_trajectory(start, Point { x, y }, steps)
                } else {
                    vec![Point { x, y }]
                };

                for point in path {
                    let code = build_mouse_script(
                        event_type,
                        point,
                        button_code,
                        click_count,
                        buttons,
                        delta_x,
                        delta_y,
                    );
                    page.evaluate(&code);
                }
            }

            Ok(json!({}))
        }
        "dispatchKeyEvent" => {
            let event_type = params.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let key = params.get("key").and_then(|v| v.as_str()).unwrap_or("");
            let code = params.get("code").and_then(|v| v.as_str()).unwrap_or("");
            let text = params.get("text").and_then(|v| v.as_str()).unwrap_or("");
            let modifiers = params.get("modifiers").and_then(Value::as_u64).unwrap_or(0);
            let delay_cfg = params.get("delay").unwrap_or(&Value::Null);
            let mean_delay_ms = delay_cfg
                .get("meanMs")
                .and_then(Value::as_f64)
                .unwrap_or(52.0);
            let sigma = delay_cfg
                .get("sigma")
                .and_then(Value::as_f64)
                .unwrap_or(0.38);
            let min_delay_ms = delay_cfg.get("minMs").and_then(Value::as_u64).unwrap_or(18);
            let max_delay_ms = delay_cfg
                .get("maxMs")
                .and_then(Value::as_u64)
                .unwrap_or(240);

            if let Some(page) = ctx.get_session_page_mut(session_id) {
                match event_type {
                    "keyDown" | "rawKeyDown" => {
                        let mut active_modifiers: Vec<(&str, &str)> = Vec::new();
                        if modifiers & 2 != 0 {
                            active_modifiers.push(("Control", "ControlLeft"));
                        }
                        if modifiers & 1 != 0 {
                            active_modifiers.push(("Alt", "AltLeft"));
                        }
                        if modifiers & 8 != 0 {
                            active_modifiers.push(("Shift", "ShiftLeft"));
                        }
                        if modifiers & 4 != 0 {
                            active_modifiers.push(("Meta", "MetaLeft"));
                        }

                        for (idx, (m_key, m_code)) in active_modifiers.iter().enumerate() {
                            if *m_key != key {
                                page.evaluate(&build_keyboard_script("keydown", m_key, m_code));
                                let ms = sample_lognormal_delay_ms(
                                    idx,
                                    mean_delay_ms * 0.6,
                                    sigma.max(0.1),
                                    min_delay_ms,
                                    max_delay_ms,
                                );
                                sleep(Duration::from_millis(ms)).await;
                            }
                        }

                        page.evaluate(&build_keyboard_script("keydown", key, code));

                        if !text.is_empty() && text != "\r" && text != "\n" {
                            for (idx, ch) in text.chars().enumerate() {
                                page.evaluate(&build_text_input_script(&ch.to_string()));
                                let ms = sample_lognormal_delay_ms(
                                    idx + active_modifiers.len() + 1,
                                    mean_delay_ms,
                                    sigma.max(0.05),
                                    min_delay_ms,
                                    max_delay_ms,
                                );
                                sleep(Duration::from_millis(ms)).await;
                            }
                        }

                        if key == "Enter" {
                            let js = "(function() {\
                                var target = document.activeElement;\
                                var dispatch = globalThis.__obscura_dispatch_event || function(t, e) { return t.dispatchEvent(e); };\
                                if (target) {\
                                    dispatch(target, new KeyboardEvent('keypress', {bubbles:true,key:'Enter',code:'Enter'}), {trusted:true});\
                                    var form = target.form || target.closest && target.closest('form');\
                                    if (form && typeof form.submit === 'function') form.submit();\
                                }\
                            })()";
                            page.evaluate(js);
                        }

                        if key == "Backspace" {
                            let js = "(function() {\
                                var target = document.activeElement;\
                                var dispatch = globalThis.__obscura_dispatch_event || function(t, e) { return t.dispatchEvent(e); };\
                                if (target && (target.localName === 'input' || target.localName === 'textarea')) {\
                                    target.value = target.value.slice(0, -1);\
                                    dispatch(target, new Event('input', {bubbles:true}), {trusted:true});\
                                }\
                            })()";
                            page.evaluate(js);
                        }

                        for (idx, (m_key, m_code)) in active_modifiers.iter().enumerate().rev() {
                            if *m_key != key {
                                let ms = sample_lognormal_delay_ms(
                                    idx + 17,
                                    mean_delay_ms * 0.4,
                                    sigma.max(0.1),
                                    min_delay_ms,
                                    max_delay_ms,
                                );
                                sleep(Duration::from_millis(ms)).await;
                                page.evaluate(&build_keyboard_script("keyup", m_key, m_code));
                            }
                        }
                    }
                    "keyUp" => {
                        page.evaluate(&build_keyboard_script("keyup", key, code));
                    }
                    "char" => {
                        if !text.is_empty() {
                            for (idx, ch) in text.chars().enumerate() {
                                page.evaluate(&build_text_input_script(&ch.to_string()));
                                let ms = sample_lognormal_delay_ms(
                                    idx,
                                    mean_delay_ms,
                                    sigma.max(0.05),
                                    min_delay_ms,
                                    max_delay_ms,
                                );
                                sleep(Duration::from_millis(ms)).await;
                            }
                        }
                    }
                    _ => {}
                }
            }

            Ok(json!({}))
        }
        "dispatchTouchEvent" => {
            let event_type = params.get("type").and_then(Value::as_str).unwrap_or("");
            let touch_points = params
                .get("touchPoints")
                .and_then(Value::as_array)
                .map(|points| points.iter().collect::<Vec<_>>())
                .unwrap_or_default();

            if let Some(page) = ctx.get_session_page_mut(session_id) {
                let code = build_touch_script(event_type, &touch_points);
                page.evaluate(&code);
            }

            Ok(json!({}))
        }
        "setIgnoreInputEvents" => Ok(json!({})),
        _ => Err(format!("Unknown Input method: {}", method)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trajectory_has_requested_steps_and_ends_at_destination() {
        let points =
            generate_human_like_trajectory(Point { x: 0.0, y: 0.0 }, Point { x: 50.0, y: 10.0 }, 6);
        assert_eq!(points.len(), 6);
        assert_eq!(points.last().copied(), Some(Point { x: 50.0, y: 10.0 }));
        assert_ne!(points[0], Point { x: 50.0, y: 10.0 });
    }

    #[test]
    fn mouse_script_contains_movement_page_coordinates_and_transition_order() {
        let script = build_mouse_script(
            "mouseMoved",
            Point { x: 100.0, y: 120.0 },
            0,
            1,
            1,
            0.0,
            0.0,
        );
        assert!(script.contains("movementX"));
        assert!(script.contains("movementY"));
        assert!(script.contains("pageX"));
        assert!(script.contains("pageY"));

        let idx_out = script.find("'mouseout'").unwrap();
        let idx_over = script.find("'mouseover'").unwrap();
        let idx_move = script.find("new MouseEvent('mousemove'").unwrap();
        assert!(idx_out < idx_over && idx_over < idx_move);

        assert!(script.contains("'mouseleave', {bubbles:false,cancelable:false"));
        assert!(script.contains("'mouseenter', {bubbles:false,cancelable:false"));
    }

    #[test]
    fn touch_script_maps_touch_payload_and_dispatches_touch_event() {
        let payload = json!([
            {
                "id": 7,
                "x": 15.0,
                "y": 30.0,
                "radiusX": 6.0,
                "radiusY": 7.0,
                "rotationAngle": 12.0,
                "force": 0.8
            }
        ]);
        let points = payload.as_array().unwrap().iter().collect::<Vec<_>>();
        let script = build_touch_script("touchStart", &points);

        assert!(script.contains("new TouchEvent('touchstart'"));
        assert!(script.contains("identifier:7"));
        assert!(script.contains("clientX:15"));
        assert!(script.contains("clientY:30"));
        assert!(script.contains("radiusX:6"));
        assert!(script.contains("radiusY:7"));
        assert!(script.contains("rotationAngle:12"));
        assert!(script.contains("force:0.8"));
        assert!(script.contains("bubbles:true,cancelable:true"));
    }
}
