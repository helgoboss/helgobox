/// A type which can express what properties are potentially be affected by a change operation.
#[derive(PartialEq)]
pub enum Affected<T> {
    /// Just the given property might be affected.
    One(T),
    /// Multiple properties might be affected.
    Multiple,
}

impl<T> Affected<T> {
    pub fn processing_relevance(&self) -> Option<ProcessingRelevance>
    where
        T: GetProcessingRelevance,
    {
        use Affected::*;
        match self {
            One(p) => p.processing_relevance(),
            Multiple => Some(ProcessingRelevance::ProcessingRelevant),
        }
    }
}

/// Defines how relevant a change to a model object is for the processing logic.
///
/// Depending on this value, the session will decide whether to sync data to the processing layer
/// or not.  
#[derive(Eq, PartialEq, Ord, PartialOrd)]
pub enum ProcessingRelevance {
    /// Lowest relevance level: Syncing of persistent processing state necessary.
    ///
    /// Returned if a change of the given prop would have an effect on control/feedback
    /// processing and is also changed by the processing layer itself, so it shouldn't contain much!
    /// The session takes care to not sync the complete mapping properties but only the ones
    /// mentioned here.
    //
    // Important to keep this on top! Order matters.
    PersistentProcessingRelevant,
    /// Highest relevance level: Syncing of complete mapping state necessary.
    ///
    /// Returned if this is a property that has an effect on control/feedback processing.
    ///
    /// However, we don't include properties here which are changed by the processing layer
    /// (such as `is_enabled`) because that would mean the complete mapping will be synced as a
    /// result, whereas we want to sync processing stuff faster!  
    ProcessingRelevant,
}

pub type ChangeResult<T> = Result<Option<Affected<T>>, String>;

pub trait Change<'a> {
    type Command;
    type Prop;

    fn change(&mut self, cmd: Self::Command) -> ChangeResult<Self::Prop>;
}

pub trait GetProcessingRelevance {
    fn processing_relevance(&self) -> Option<ProcessingRelevance>;
}

pub fn merge_affected<T: PartialEq>(
    affected_1: Option<Affected<T>>,
    affected_2: Option<Affected<T>>,
) -> Option<Affected<T>> {
    match (affected_1, affected_2) {
        (None, None) => None,
        (None, Some(a)) | (Some(a), None) => Some(a),
        (Some(a), Some(b)) => {
            use Affected::*;
            match (a, b) {
                (_, Multiple) | (Multiple, _) => Some(Multiple),
                (One(p1), One(p2)) => {
                    if p1 == p2 {
                        Some(One(p1))
                    } else {
                        Some(Multiple)
                    }
                }
            }
        }
    }
}
