//! Contains functions for sending data to WebSocket clients.
use crate::application::{Session, SharedSession};
use crate::base::{when, Global};
use crate::domain::{BackboneState, ProjectionFeedbackValue};
use crate::infrastructure::plugin::App;
use crate::infrastructure::server::data::{
    create_clip_matrix_event, get_active_controller_updated_event,
    get_controller_routing_updated_event, get_projection_feedback_event, get_session_updated_event,
    send_initial_feedback, SessionResponseData, Topic,
};
use crate::infrastructure::server::http::client::WebSocketClient;
use playtime_api::runtime::{
    ClipPlayStateUpdate, ClipPositionUpdate, FrequentSlotUpdate, OccasionalSlotUpdate,
    QualifiedSlotEvent, SlotCoordinates,
};
use playtime_clip_engine::main::{ClipMatrixEvent, SlotWithColumn};
use playtime_clip_engine::rt::ClipChangeEvent;
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
        ClipMatrixOccasionalSlotUpdates { session_id } => {
            send_initial_clip_matrix_occasional_slot_updates(client, session_id)
        }
        ClipMatrixClipPositionUpdates { session_id } => {
            send_initial_clip_matrix_clip_positions(client, session_id)
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

fn send_initial_clip_matrix_occasional_slot_updates(
    client: &WebSocketClient,
    session_id: &str,
) -> Result<(), &'static str> {
    send_initial_clip_matrix_slot_events(client, session_id, "occasional-slot-updates", |slot| {
        slot.value()
            .clip_play_state()
            .map(|play_state| {
                OccasionalSlotUpdate::PlayState(ClipPlayStateUpdate {
                    play_state: play_state.get(),
                })
            })
            .into_iter()
            .collect()
    })
}

fn send_initial_clip_matrix_clip_positions(
    client: &WebSocketClient,
    session_id: &str,
) -> Result<(), &'static str> {
    send_initial_clip_matrix_slot_events(client, session_id, "clip-position-updates", |slot| {
        slot.value()
            .proportional_position()
            .map(|pos| {
                FrequentSlotUpdate::Position(ClipPositionUpdate {
                    position: pos.get(),
                })
            })
            .into_iter()
            .collect()
    })
}

fn send_initial_clip_matrix_slot_events<T: Serialize>(
    client: &WebSocketClient,
    session_id: &str,
    clip_matrix_topic_key: &str,
    create_payloads: impl Fn(SlotWithColumn) -> Vec<T>,
) -> Result<(), &'static str> {
    let session = App::get()
        .find_session_by_id(session_id)
        .ok_or("session not found")?;
    let session = session.borrow();
    let instance_state = session.instance_state();
    let slot_events: Vec<QualifiedSlotEvent<T>> =
        BackboneState::get().with_clip_matrix(&instance_state, |matrix| {
            matrix
                .all_slots()
                .flat_map(|slot| {
                    let coordinates = SlotCoordinates {
                        column: slot.column_index() as u32,
                        row: slot.value().index() as u32,
                    };
                    let payloads = create_payloads(slot);
                    payloads.into_iter().map(move |payload| QualifiedSlotEvent {
                        coordinates,
                        payload,
                    })
                })
                .collect()
        })?;
    if slot_events.is_empty() {
        return Ok(());
    }
    let aggregated_event = create_clip_matrix_event(session_id, clip_matrix_topic_key, slot_events);
    client.send(&aggregated_event)
}

pub fn send_updated_active_controller(session: &Session) -> Result<(), &'static str> {
    send_to_clients_subscribed_to(
        &Topic::ActiveController {
            session_id: session.id().to_string(),
        },
        || {
            Some(get_active_controller_updated_event(
                session.id(),
                Some(session),
            ))
        },
    )
}

pub fn send_updated_controller_routing(session: &Session) -> Result<(), &'static str> {
    send_to_clients_subscribed_to(
        &Topic::ControllerRouting {
            session_id: session.id().to_string(),
        },
        || {
            Some(get_controller_routing_updated_event(
                session.id(),
                Some(session),
            ))
        },
    )
}

pub fn send_clip_matrix_events_to_subscribed_clients(
    session_id: &str,
    clip_matrix_events: &[ClipMatrixEvent],
) -> Result<(), &'static str> {
    send_to_clients_subscribed_to(
        &Topic::ClipMatrixOccasionalSlotUpdates {
            session_id: session_id.to_string(),
        },
        || {
            let events = create_clip_matrix_occasional_slot_update_events(clip_matrix_events);
            if events.is_empty() {
                return None;
            }
            Some(create_clip_matrix_event(
                session_id,
                "occasional-slot-updates",
                events,
            ))
        },
    )?;
    send_to_clients_subscribed_to(
        &Topic::ClipMatrixClipPositionUpdates {
            session_id: session_id.to_string(),
        },
        || {
            let events = create_clip_matrix_clip_position_update_events(clip_matrix_events);
            if events.is_empty() {
                return None;
            }
            Some(create_clip_matrix_event(
                session_id,
                "clip-position-updates",
                events,
            ))
        },
    )?;
    Ok(())
}

fn create_clip_matrix_occasional_slot_update_events(
    events: &[ClipMatrixEvent],
) -> Vec<QualifiedSlotEvent<OccasionalSlotUpdate>> {
    create_clip_matrix_slot_events(events, |event| {
        if let ClipChangeEvent::PlayState(p) = event {
            Some(OccasionalSlotUpdate::PlayState(ClipPlayStateUpdate {
                play_state: p.get(),
            }))
        } else {
            None
        }
    })
}

fn create_clip_matrix_clip_position_update_events(
    events: &[ClipMatrixEvent],
) -> Vec<QualifiedSlotEvent<FrequentSlotUpdate>> {
    create_clip_matrix_slot_events(events, |event| {
        if let ClipChangeEvent::ClipPosition(p) = event {
            Some(FrequentSlotUpdate::Position(ClipPositionUpdate {
                position: p.get(),
            }))
        } else {
            None
        }
    })
}

fn create_clip_matrix_slot_events<T: Serialize>(
    events: &[ClipMatrixEvent],
    create_payload: impl Fn(&ClipChangeEvent) -> Option<T>,
) -> Vec<QualifiedSlotEvent<T>> {
    events
        .iter()
        .filter_map(|e| {
            let e = match e {
                ClipMatrixEvent::ClipChanged(e) => e,
                _ => return None,
            };
            let payload = create_payload(&e.event)?;
            // TODO-high We probably want to use the API SlotCoordinates everywhere!
            let coordinates = SlotCoordinates {
                column: e.slot_coordinates.column() as u32,
                row: e.slot_coordinates.row() as u32,
            };
            let event = QualifiedSlotEvent {
                coordinates,
                payload,
            };
            Some(event)
        })
        .collect()
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

pub fn keep_informing_clients_about_sessions() {
    App::get().sessions_changed().subscribe(|_| {
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

pub fn keep_informing_clients_about_session_events(shared_session: &SharedSession) {
    let session = shared_session.borrow();
    let instance_state = session.instance_state().borrow();
    when(
        instance_state
            .on_mappings_changed()
            .merge(session.mapping_list_changed().map_to(())),
    )
    .with(Rc::downgrade(shared_session))
    .do_async(|session, _| {
        let _ = send_updated_controller_routing(&session.borrow());
    });
    when(App::get().controller_preset_manager().borrow().changed())
        .with(Rc::downgrade(shared_session))
        .do_async(|session, _| {
            let _ = send_updated_active_controller(&session.borrow());
        });
    when(session.everything_changed().merge(session.id.changed()))
        .with(Rc::downgrade(shared_session))
        .do_async(|session, _| {
            send_sessions_to_subscribed_clients();
            let session = session.borrow();
            let _ = send_updated_active_controller(&session);
            let _ = send_updated_controller_routing(&session);
        });
}
