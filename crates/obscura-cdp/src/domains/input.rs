use serde_json::{json, Value};

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

    let mut points = Vec::with_capacity(steps);
    for i in 1..=steps {
        let t = i as f64 / steps as f64;
        let eased = t * t * (3.0 - 2.0 * t);
        let jitter_scale = (1.0 - (2.0 * t - 1.0).abs()) * 0.75;
        let jitter = ((i as f64) * 1.618_033_988_75).sin() * jitter_scale;
        points.push(Point {
            x: start.x + dx * eased + normal_x * jitter,
            y: start.y + dy * eased + normal_y * jitter,
        });
    }

    if let Some(last) = points.last_mut() {
        *last = end;
    }
    points
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
                state.target.dispatchEvent(new MouseEvent('mouseout', {{bubbles:true,cancelable:true,clientX:clientX,clientY:clientY,pageX:pageX,pageY:pageY,movementX:movementX,movementY:movementY,button:{button_code},buttons:{buttons}}}));\
                state.target.dispatchEvent(new MouseEvent('mouseleave', {{bubbles:false,cancelable:false,clientX:clientX,clientY:clientY,pageX:pageX,pageY:pageY,movementX:movementX,movementY:movementY,button:{button_code},buttons:{buttons}}}));\
                target.dispatchEvent(new MouseEvent('mouseover', {{bubbles:true,cancelable:true,clientX:clientX,clientY:clientY,pageX:pageX,pageY:pageY,movementX:movementX,movementY:movementY,button:{button_code},buttons:{buttons}}}));\
                target.dispatchEvent(new MouseEvent('mouseenter', {{bubbles:false,cancelable:false,clientX:clientX,clientY:clientY,pageX:pageX,pageY:pageY,movementX:movementX,movementY:movementY,button:{button_code},buttons:{buttons}}}));\
            }}\
            var evt = new MouseEvent('{dom_event}', {{bubbles:{bubbles},cancelable:{cancelable},clientX:clientX,clientY:clientY,pageX:pageX,pageY:pageY,movementX:movementX,movementY:movementY,button:{button_code},buttons:{buttons},detail:{click_count}}});\
            target.dispatchEvent(evt);\
            if ('{event_type}' === 'mousePressed') {{\
                state.lastDownTarget = target;\
                state.lastButton = {button_code};\
                state.lastClickCount = {click_count};\
            }}\
            if ({emit_click} && state.lastDownTarget === target) {{\
                target.dispatchEvent(new MouseEvent('click', {{bubbles:true,cancelable:true,clientX:clientX,clientY:clientY,pageX:pageX,pageY:pageY,movementX:movementX,movementY:movementY,button:state.lastButton,buttons:0,detail:state.lastClickCount}}));\
            }}\
            if ('{event_type}' === 'mouseWheel') {{\
                target.dispatchEvent(new WheelEvent('wheel', {{bubbles:true,cancelable:true,clientX:clientX,clientY:clientY,pageX:pageX,pageY:pageY,deltaX:{delta_x},deltaY:{delta_y},deltaMode:0}}));\
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
            var touchesInit = [{mapped}];\
            var touches = touchesInit.map(function(init) {{\
                var target = doc.elementFromPoint(init.clientX, init.clientY) || doc.body;\
                return new Touch({{identifier:init.identifier,target:target,clientX:init.clientX,clientY:init.clientY,pageX:init.clientX + window.scrollX,pageY:init.clientY + window.scrollY,screenX:init.clientX,screenY:init.clientY,radiusX:init.radiusX,radiusY:init.radiusY,rotationAngle:init.rotationAngle,force:init.force}});\
            }});\
            var primaryTarget = touches[0] ? touches[0].target : (doc.activeElement || doc.body);\
            if (!primaryTarget) return;\
            var evt = new TouchEvent('{dom_event}', {{bubbles:{bubbles},cancelable:{cancelable},touches:touches,targetTouches:touches.filter(function(t){{return t.target===primaryTarget;}}),changedTouches:touches}});\
            primaryTarget.dispatchEvent(evt);\
            globalThis.__obscura_touch_target = primaryTarget;\
        }})()",
        mapped = mapped,
        dom_event = dom_event,
        bubbles = bubbles,
        cancelable = cancelable,
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

            if let Some(page) = ctx.get_session_page_mut(session_id) {
                match event_type {
                    "keyDown" | "rawKeyDown" => {
                        let js = format!(
                            "(function() {{\
                                var target = document.activeElement || document.body;\
                                var evt = new KeyboardEvent('keydown', {{bubbles:true,cancelable:true,key:'{key}',code:'{code}'}});\
                                target.dispatchEvent(evt);\
                            }})()",
                            key = js_escape(key),
                            code = js_escape(code),
                        );
                        page.evaluate(&js);

                        if !text.is_empty() && text != "\r" && text != "\n" {
                            let js = format!(
                                "(function() {{\
                                    var target = document.activeElement;\
                                    if (target && (target.localName === 'input' || target.localName === 'textarea')) {{\
                                        target.value = (target.value || '') + '{text}';\
                                        target.dispatchEvent(new Event('input', {{bubbles:true}}));\
                                    }}\
                                }})()",
                                text = js_escape(text),
                            );
                            page.evaluate(&js);
                        }

                        if key == "Enter" {
                            let js = "(function() {\
                                var target = document.activeElement;\
                                if (target) {\
                                    target.dispatchEvent(new KeyboardEvent('keypress', {bubbles:true,key:'Enter',code:'Enter'}));\
                                    var form = target.form || target.closest && target.closest('form');\
                                    if (form && typeof form.submit === 'function') form.submit();\
                                }\
                            })()";
                            page.evaluate(js);
                        }

                        if key == "Backspace" {
                            let js = "(function() {\
                                var target = document.activeElement;\
                                if (target && (target.localName === 'input' || target.localName === 'textarea')) {\
                                    target.value = target.value.slice(0, -1);\
                                    target.dispatchEvent(new Event('input', {bubbles:true}));\
                                }\
                            })()";
                            page.evaluate(js);
                        }
                    }
                    "keyUp" => {
                        let js = format!(
                            "(function() {{\
                                var target = document.activeElement || document.body;\
                                var evt = new KeyboardEvent('keyup', {{bubbles:true,key:'{key}',code:'{code}'}});\
                                target.dispatchEvent(evt);\
                            }})()",
                            key = js_escape(key),
                            code = js_escape(code),
                        );
                        page.evaluate(&js);
                    }
                    "char" => {
                        if !text.is_empty() {
                            let js = format!(
                                "(function() {{\
                                    var target = document.activeElement;\
                                    if (target && (target.localName === 'input' || target.localName === 'textarea')) {{\
                                        target.value = (target.value || '') + '{text}';\
                                        target.dispatchEvent(new Event('input', {{bubbles:true}}));\
                                    }}\
                                }})()",
                                text = js_escape(text),
                            );
                            page.evaluate(&js);
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
