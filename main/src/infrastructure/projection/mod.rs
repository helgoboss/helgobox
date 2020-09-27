use crate::application::{Session, SharedSession, SourceCategory, TargetCategory, WeakSession};
use crate::core::when;
use crate::domain::{MappingCompartment, MappingId};
use rxrust::prelude::*;
use serde::Serialize;
use std::rc::Rc;

pub fn register_session(shared_session: &SharedSession) {
    let session = shared_session.borrow();
    when(session.on_mappings_changed())
        .with(Rc::downgrade(shared_session))
        .do_async(|session, _| {
            let _ = print_controller_projection(&session.borrow());
        });
}

pub fn print_controller_projection(session: &Session) -> Result<(), &'static str> {
    let mapping_projections = session
        .mappings(MappingCompartment::ControllerMappings)
        .map(|m| {
            let m = m.borrow();
            let target_projection = if session.mapping_is_on(m.id()) {
                if m.target_model.category.get() == TargetCategory::Virtual {
                    let control_element = m.target_model.create_control_element();
                    let matching_primary_mappings: Vec<_> = session
                        .mappings(MappingCompartment::PrimaryMappings)
                        .filter(|mp| {
                            let mp = mp.borrow();
                            mp.source_model.category.get() == SourceCategory::Virtual
                                && &mp.source_model.create_control_element() == &control_element
                                && session.mapping_is_on(mp.id())
                        })
                        .collect();
                    if let Some(first_mapping) = matching_primary_mappings.first() {
                        let first_mapping = first_mapping.borrow();
                        let first_mapping_name = first_mapping.name.get_ref();
                        let label = if matching_primary_mappings.len() == 1 {
                            first_mapping_name.clone()
                        } else {
                            format!(
                                "{} +{}",
                                first_mapping_name,
                                matching_primary_mappings.len() - 1
                            )
                        };
                        Some(TargetProjection { label })
                    } else {
                        None
                    }
                } else {
                    Some(TargetProjection {
                        label: m.name.get_ref().clone(),
                    })
                }
            } else {
                None
            };
            // if m.target_model.ca
            MappingProjection {
                id: m.id().to_string(),
                name: m.name.get_ref().clone(),
                target_projection,
            }
        })
        .collect();
    let controller_projection = ControllerProjection {
        mapping_projections,
    };
    let json =
        serde_json::to_string_pretty(&controller_projection).map_err(|_| "couldn't serialize")?;
    println!("{}", json);
    Ok(())
}

#[derive(Serialize)]
struct ControllerProjection {
    mapping_projections: Vec<MappingProjection>,
}

#[derive(Serialize)]
struct MappingProjection {
    id: String,
    name: String,
    target_projection: Option<TargetProjection>,
}

#[derive(Serialize)]
struct TargetProjection {
    label: String,
}
