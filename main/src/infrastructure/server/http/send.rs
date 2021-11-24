use crate::application::Session;
use crate::domain::ProjectionFeedbackValue;
use crate::infrastructure::data::{ControllerPresetData, PresetData};
use crate::infrastructure::plugin::App;
use crate::infrastructure::server::http::routes::{get_controller_routing, SessionResponseData};
use crate::infrastructure::server::http::server::{Topic, WebSocketClient};
use crate::infrastructure::server::ControllerRouting;
use helgoboss_learn::UnitValue;
use maplit::hashmap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::rc::Rc;

pub fn send_initial_events(client: &WebSocketClient) {
    for topic in &client.topics {
        let _ = send_initial_events_for_topic(client, topic);
    }
}

fn send_initial_events_for_topic(
    client: &WebSocketClient,
    topic: &Topic,
) -> Result<(), &'static str> {
    use Topic::*;
    match topic {
        Session { session_id } => send_initial_session(client, session_id),
        ControllerRouting { session_id } => send_initial_controller_routing(client, session_id),
        ActiveController { session_id } => send_initial_controller(client, session_id),
        Feedback { session_id } => {
            send_initial_feedback(session_id);
            Ok(())
        }
    }
}
pub fn send_initial_session(
    client: &WebSocketClient,
    session_id: &str,
) -> Result<(), &'static str> {
    let event = if App::get().find_session_by_id(session_id).is_some() {
        get_session_updated_event(session_id, Some(SessionResponseData {}))
    } else {
        get_session_updated_event(session_id, None)
    };
    client.send(&event)
}

fn send_initial_controller_routing(
    client: &WebSocketClient,
    session_id: &str,
) -> Result<(), &'static str> {
    let event = if let Some(session) = App::get().find_session_by_id(session_id) {
        get_controller_routing_updated_event(session_id, Some(&session.borrow()))
    } else {
        get_controller_routing_updated_event(session_id, None)
    };
    client.send(&event)
}

fn send_initial_controller(client: &WebSocketClient, session_id: &str) -> Result<(), &'static str> {
    let event = if let Some(session) = App::get().find_session_by_id(session_id) {
        get_active_controller_updated_event(session_id, Some(&session.borrow()))
    } else {
        get_active_controller_updated_event(session_id, None)
    };
    client.send(&event)
}

fn send_initial_feedback(session_id: &str) {
    if let Some(session) = App::get().find_session_by_id(session_id) {
        session.borrow_mut().send_all_feedback();
    }
}

pub fn send_updated_active_controller(session: &Session) -> Result<(), &'static str> {
    send_to_clients_subscribed_to(
        &Topic::ActiveController {
            session_id: session.id().to_string(),
        },
        || get_active_controller_updated_event(session.id(), Some(session)),
    )
}

pub fn send_updated_controller_routing(session: &Session) -> Result<(), &'static str> {
    send_to_clients_subscribed_to(
        &Topic::ControllerRouting {
            session_id: session.id().to_string(),
        },
        || get_controller_routing_updated_event(session.id(), Some(session)),
    )
}

pub fn send_projection_feedback_to_subscribed_clients(
    session_id: &str,
    value: ProjectionFeedbackValue,
) -> Result<(), &'static str> {
    send_to_clients_subscribed_to(
        &Topic::Feedback {
            session_id: session_id.to_string(),
        },
        || get_projection_feedback_event(session_id, value),
    )
}

fn send_to_clients_subscribed_to<T: Serialize>(
    topic: &Topic,
    create_message: impl FnOnce() -> T,
) -> Result<(), &'static str> {
    for_each_client(
        |client, cached| {
            if client.is_subscribed_to(topic) {
                let _ = client.send(cached);
            }
        },
        create_message,
    )
}

pub fn for_each_client<T: Serialize>(
    op: impl Fn(&WebSocketClient, &T),
    cache: impl FnOnce() -> T,
) -> Result<(), &'static str> {
    let server = App::get().server().borrow();
    if !server.is_running() {
        return Ok(());
    }
    let clients = server.clients()?.clone();
    let clients = clients
        .read()
        .map_err(|_| "couldn't get read lock for client")?;
    if clients.is_empty() {
        return Ok(());
    }
    let cached = cache();
    for client in clients.values() {
        op(client, &cached);
    }
    Ok(())
}

fn get_active_controller_updated_event(
    session_id: &str,
    session: Option<&Session>,
) -> Event<Option<ControllerPresetData>> {
    Event::put(
        format!("/realearn/session/{}/controller", session_id),
        session.and_then(get_controller),
    )
}

fn get_projection_feedback_event(
    session_id: &str,
    feedback_value: ProjectionFeedbackValue,
) -> Event<HashMap<Rc<str>, UnitValue>> {
    Event::patch(
        format!("/realearn/session/{}/feedback", session_id),
        hashmap! {
            feedback_value.mapping_key => feedback_value.value
        },
    )
}

fn get_session_updated_event(
    session_id: &str,
    session_data: Option<SessionResponseData>,
) -> Event<Option<SessionResponseData>> {
    Event::put(format!("/realearn/session/{}", session_id), session_data)
}

pub fn get_controller_routing_updated_event(
    session_id: &str,
    session: Option<&Session>,
) -> Event<Option<ControllerRouting>> {
    Event::put(
        format!("/realearn/session/{}/controller-routing", session_id),
        session.map(get_controller_routing),
    )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Event<T> {
    /// Roughly corresponds to the HTTP method of the resource.
    r#type: EventType,
    /// Corresponds to the HTTP path of the resource.
    path: String,
    /// Corresponds to the HTTP body.
    ///
    /// HTTP 404 corresponds to this value being `null` or undefined in JSON. If this is not enough
    /// in future use cases, we can still add another field that resembles the HTTP status.
    body: T,
}

impl<T> Event<T> {
    pub fn put(path: String, body: T) -> Event<T> {
        Event {
            r#type: EventType::Put,
            path,
            body,
        }
    }

    pub fn patch(path: String, body: T) -> Event<T> {
        Event {
            r#type: EventType::Patch,
            path,
            body,
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum EventType {
    Put,
    Patch,
}

fn get_controller(session: &Session) -> Option<ControllerPresetData> {
    let controller = session.active_controller()?;
    Some(ControllerPresetData::from_model(&controller))
}
