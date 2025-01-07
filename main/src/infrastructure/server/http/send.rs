//! Contains functions for sending data to WebSocket clients.
use crate::application::{SharedUnitModel, UnitModel};
use crate::base::when;
use crate::domain::ProjectionFeedbackValue;
use crate::infrastructure::plugin::BackboneShell;
use crate::infrastructure::server::data::{
    get_active_controller_updated_event, get_controller_routing_updated_event,
    get_projection_feedback_event, get_session_updated_event, send_initial_feedback,
    SessionResponseData, Topic,
};
use crate::infrastructure::server::http::client::WebSocketClient;
use base::Global;
use rxrust::prelude::*;
use serde::Serialize;
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
    let event = if BackboneShell::get()
        .find_unit_model_by_key(session_id)
        .is_some()
    {
        get_session_updated_event(session_id, Some(SessionResponseData {}))
    } else {
        get_session_updated_event(session_id, None)
    };
    client.send(event)
}

fn send_initial_controller_routing(
    client: &WebSocketClient,
    session_id: &str,
) -> Result<(), &'static str> {
    let event = if let Some(session) = BackboneShell::get().find_unit_model_by_key(session_id) {
        get_controller_routing_updated_event(session_id, Some(&session.borrow()))
    } else {
        get_controller_routing_updated_event(session_id, None)
    };
    client.send(event)
}

fn send_initial_controller(client: &WebSocketClient, session_id: &str) -> Result<(), &'static str> {
    let event = if let Some(session) = BackboneShell::get().find_unit_model_by_key(session_id) {
        get_active_controller_updated_event(session_id, Some(&session.borrow()))
    } else {
        get_active_controller_updated_event(session_id, None)
    };
    client.send(&event)
}

pub fn send_updated_active_controller(session: &UnitModel) -> Result<(), &'static str> {
    send_to_clients_subscribed_to(
        &Topic::ActiveController {
            session_id: session.unit_key().to_string(),
        },
        || {
            Some(get_active_controller_updated_event(
                session.unit_key(),
                Some(session),
            ))
        },
    )
}

pub fn send_updated_controller_routing(session: &UnitModel) -> Result<(), &'static str> {
    BackboneShell::get()
        .proto_hub()
        .notify_controller_routing_changed(session);
    send_to_clients_subscribed_to(
        &Topic::ControllerRouting {
            session_id: session.unit_key().to_string(),
        },
        || {
            Some(get_controller_routing_updated_event(
                session.unit_key(),
                Some(session),
            ))
        },
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
        || Some(get_projection_feedback_event(session_id, value)),
    )
}

fn send_to_clients_subscribed_to<T: Serialize>(
    topic: &Topic,
    create_message: impl FnOnce() -> Option<T>,
) -> Result<(), &'static str> {
    for_each_client(
        |client, cached| {
            if let Some(cached) = cached {
                if client.is_subscribed_to(topic) {
                    let _ = client.send(cached);
                }
            }
        },
        create_message,
    )
}

pub fn for_each_client<T: Serialize>(
    op: impl Fn(&WebSocketClient, &T),
    cache: impl FnOnce() -> T,
) -> Result<(), &'static str> {
    let server = BackboneShell::get().server().borrow();
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

pub fn keep_informing_clients_about_sessions(
    sessions_changed: impl LocalObservable<'static, Item = (), Err = ()> + 'static,
) {
    sessions_changed.subscribe(|_| {
        Global::task_support()
            .do_later_in_main_thread_asap(|| {
                send_sessions_to_subscribed_clients();
            })
            .unwrap();
    });
}

pub fn send_sessions_to_subscribed_clients() {
    for_each_client(
        |client, _| {
            for t in client.topics.iter() {
                if let Topic::Session { session_id } = t {
                    let _ = send_initial_session(client, session_id);
                }
            }
        },
        || (),
    )
    .unwrap();
}

pub fn keep_informing_clients_about_session_events(shared_session: &SharedUnitModel) {
    let session = shared_session.borrow();
    let instance_state = session.unit().borrow();
    when(
        instance_state
            .on_mappings_changed()
            .merge(session.mapping_list_changed().map_to(())),
    )
    .with(Rc::downgrade(shared_session))
    .do_async(|session, _| {
        let _ = send_updated_controller_routing(&session.borrow());
    });
    when(
        BackboneShell::get()
            .controller_preset_manager()
            .borrow()
            .changed(),
    )
    .with(Rc::downgrade(shared_session))
    .do_async(|session, _| {
        let _ = send_updated_active_controller(&session.borrow());
    });
    when(session.everything_changed())
        .with(Rc::downgrade(shared_session))
        .do_async(|session, _| {
            send_sessions_to_subscribed_clients();
            let session = session.borrow();
            let _ = send_updated_active_controller(&session);
            let _ = send_updated_controller_routing(&session);
        });
}
