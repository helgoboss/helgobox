use enum_iterator::IntoEnumIterator;

/// A type which can express what properties are potentially be affected by a change operation.
pub enum Affected<T: Copy> {
    /// Just the given property might be affected.
    One(T),
    /// All the given properties might be affected.
    Multiple(Vec<T>),
    /// Many properties might be affected but I'm too lazy at the moment to enumerate them.
    ///
    /// This behaves just like [Self::All] but acts as a marker for possible later UI performance
    /// improvements.
    Whatever,
    /// All properties might be affected (for real ... I'm not just lazy!).
    All,
}

impl<T: Copy> Affected<T> {
    /// Returns an iterator over the affected properties or `None` if potentially all
    /// properties are affected (which one might react to with a complete refresh on the UI side).
    pub fn opt_iter(&self) -> Option<impl Iterator<Item = T>> {
        use Affected::*;
        match self {
            One(p) => Some(vec![*p].into_iter()),
            // TODO-high Cloning the vec is not particularly efficient. Better write an own
            //  iterator that considers One or Multiple without using boxing.
            Multiple(v) => Some(v.clone().into_iter()),
            Whatever | All => None,
        }
    }

    /// For wrapping one or multiple affected props with a surrounding prop.
    pub fn map<R: Copy>(self, f: impl Fn(Option<T>) -> R) -> Affected<R> {
        use Affected::*;
        match self {
            One(p) => One(f(Some(p))),
            Multiple(v) => Multiple(v.into_iter().map(|p| f(Some(p))).collect()),
            Whatever | All => One(f(None)),
        }
    }

    pub fn processing_relevance(&self) -> Option<ProcessingRelevance>
    where
        T: GetProcessingRelevance,
    {
        use Affected::*;
        match self {
            One(p) => p.processing_relevance(),
            Multiple(v) => v.iter().flat_map(|p| p.processing_relevance()).max(),
            Whatever | All => Some(ProcessingRelevance::ProcessingRelevant),
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

pub trait Change {
    type Command;
    type Prop: Copy;

    fn change(&mut self, val: Self::Command) -> Result<Affected<Self::Prop>, String>;
}

pub trait GetProcessingRelevance {
    fn processing_relevance(&self) -> Option<ProcessingRelevance>;
}
