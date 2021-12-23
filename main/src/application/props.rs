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

/// Usable for changing values of properties in an infallible way.
///
/// This is a bit like the Flux pattern. One has commands (or actions) that describe how to change
/// state and the store (in this case the value itself) changes its state accordingly.
///
/// This pattern has been introduced in #492 when changing the change-notification mechanism.
/// We moved from an Rx-based approach where each property has its own subscribers to a more
/// flexible and much more memory-friendly approach that works by letting any property change start
/// at the session and letting the session handle the notification centrally.
///
/// This command pattern is actually not necessary for this new change-notification mechanism. We
/// could also provide simple setter methods that return `Affected` values (as we already do when
/// we apply more complex changes than just changing a few properties). This would have the
/// advantage that we can choose more specific return types, not such a generic one (e.g. `Result`
/// if the change is fallible). However, we introduced the pattern and it's too early to remove it.
///
/// Because it also has some potential advantages:
///
/// - It unifies infallible property write access.
/// - It allows for hierarchical changes without the need for closures, e.g. the command to change a
///   target property p of a mapping m in compartment c is expressed as a simple object! This also
///   has a nice symmetry to the way affected properties are returned (which is also hierarchial).
/// - Because of this unification, we could record changes, log them easily, even build some undo
///   system on it.
///
/// Let's see where this goes!
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
