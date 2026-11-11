use crate::infrastructure::proto;
use pot::{PersistentBankId, PersistentCategoryId};
use reaper_high::{Fx, Guid, Track};

pub fn convert_track_id_to_proto(track: &Track) -> String {
    track.guid().to_string_without_braces()
}

pub fn convert_fx_id_to_proto(fx: &Fx) -> Option<String> {
    Some(fx.get_or_query_guid().ok()?.to_string_without_braces())
}

pub fn convert_fx_id_to_engine(id: &str) -> anyhow::Result<Guid> {
    let guid = Guid::from_string_without_braces(id)?;
    Ok(guid)
}

pub fn convert_pot_bank_qualified_id_to_proto(
    value: PersistentBankId,
) -> proto::PotBankQualifiedId {
    proto::PotBankQualifiedId {
        main_bank_id: value.bank,
        sub_bank_id: value.sub_bank,
    }
}
pub fn convert_pot_category_qualified_id_to_proto(
    value: PersistentCategoryId,
) -> proto::PotCategoryQualifiedId {
    proto::PotCategoryQualifiedId {
        main_category_id: value.category,
        sub_category_id: value.sub_category,
    }
}
