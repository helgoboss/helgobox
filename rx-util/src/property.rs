use crate::SharedEvent;
use rxrust::prelude::*;
use std::fmt;

/// Convenience function.
///
/// Useful when many properties need to be initialized (can be easily renamed to a shortcut).
pub fn create_local_prop<'a, T: PartialEq>(initial_value: T) -> LocalProp<'a, T> {
    LocalProp::new(initial_value)
}

pub fn create_shared_prop<T: PartialEq>(initial_value: T) -> SharedProp<T> {
    SharedProp::new(initial_value)
}

pub type LocalStaticProp<T> = LocalProp<'static, T>;

pub type LocalProp<'a, T> = BaseProp<T, LocalSubject<'a, (), ()>>;

pub type SharedProp<T> = BaseProp<T, SharedSubject<(), ()>>;

/// A reactive property which has the following characteristics:
///
/// - It can be initialized with a transformer, which is an fn that transforms the value passed to
///   the setter before actually setting the value. It's good for restricting the value range of
///   that property. It's not good for maintaining object-wide invariants because transformers which
///   enclose over surrounding state are not advisable and therefore currently disabled by taking fn
///   only.
/// - It's cloneable if its value is cloneable (cloning it clones the value and the transformer, not
///   the change listeners)
/// - Equality operators are based just on the value, not on transformers and listeners
pub struct BaseProp<T, S> {
    value: T,
    subject: S,
    transformer: fn(T) -> T,
}

impl<T, S> BaseProp<T, S> {
    /// Creates the property with an initial value and identity transformer.
    pub fn new(initial_value: T) -> Self
    where
        S: Default,
    {
        Self {
            value: initial_value,
            subject: Default::default(),
            transformer: |v| v,
        }
    }

    /// Creates the property with an initial value and a custom transformer. The transformer is not
    /// applied to the initial value.
    pub fn new_with_transformer(initial_value: T, transformer: fn(T) -> T) -> Self
    where
        S: Default,
    {
        Self {
            value: initial_value,
            subject: Default::default(),
            transformer,
        }
    }

    /// Returns a copy of the current value of this property.
    pub fn get(&self) -> T
    where
        T: Copy,
    {
        self.value
    }

    /// Returns the current value of this property.
    pub fn get_ref(&self) -> &T {
        &self.value
    }
    /// Sets this property to the given value. If a transformer has been defined, the given value
    /// might be changed into another one before. Observers are notified only if the given value
    /// is different from the current value.
    pub fn set(&mut self, value: T)
    where
        T: PartialEq,
        S: Observer<(), ()>,
    {
        let transformed_value = (self.transformer)(value);
        if transformed_value == self.value {
            return;
        }
        self.value = transformed_value;
        self.subject.next(());
    }
}

impl<'a, T> LocalProp<'a, T> {
    /// Fires whenever the value is changed.
    ///
    /// Event always contains a unit value instead of the
    /// new value. This is perfect for combining observables because observables can be combined
    /// much easier if they have the same type. UI event handlers for example are often not
    /// interested in the new value anyway because they will just call some reusable
    /// invalidation code that queries the new value itself.
    pub fn changed(&self) -> impl LocalObservable<'a, Item = (), Err = ()> {
        self.subject.clone()
    }
}

impl<T> SharedProp<T> {
    pub fn changed(&self) -> impl SharedEvent<()> {
        self.subject.clone()
    }
}

impl<T: fmt::Debug, S> fmt::Debug for BaseProp<T, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Property")
            .field("value", &self.value)
            .finish()
    }
}

impl<T: Clone, S: Default> Clone for BaseProp<T, S> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            subject: Default::default(),
            transformer: self.transformer,
        }
    }
}

impl<T: Default, S: Default> Default for BaseProp<T, S> {
    fn default() -> Self {
        Self {
            value: Default::default(),
            subject: Default::default(),
            transformer: |v| v,
        }
    }
}

impl<T: PartialEq, S: Default> From<T> for BaseProp<T, S> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl<'a, T: PartialEq, S> PartialEq for BaseProp<T, S> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get() {
        // Given
        let p = LocalProp::new(5);
        // When
        // Then
        assert_eq!(p.get(), 5);
    }

    #[test]
    fn set() {
        // Given
        let mut p = LocalProp::new(5);
        // When
        p.set(6);
        // Then
        assert_eq!(p.get(), 6);
    }

    #[test]
    fn clone() {
        // Given
        let p = LocalProp::new(5);
        // When
        let p2 = p.clone();
        // Then
        assert_eq!(p.get(), 5);
        assert_eq!(p2.get(), 5);
    }

    #[test]
    fn clone_set_independent() {
        // Given
        let mut p = LocalProp::new(5);
        // When
        let mut p2 = p.clone();
        p.set(2);
        p2.set(7);
        // Then
        assert_eq!(p.get(), 2);
        assert_eq!(p2.get(), 7);
    }

    #[test]
    fn transformer() {
        // Given
        let mut p = LocalProp::new_with_transformer(5, |v| v.min(100));
        // When
        p.set(105);
        // Then
        assert_eq!(p.get(), 100);
    }

    #[test]
    fn clone_transformer_works() {
        // Given
        let p = LocalProp::new_with_transformer(5, |v| v.min(100));
        // When
        let mut p2 = p.clone();
        p2.set(105);
        // Then
        assert_eq!(p2.get(), 100);
    }

    #[test]
    fn observe() {
        // Given
        let mut invocation_count = 0;
        // When
        {
            let mut p = LocalProp::new(5);
            p.changed().subscribe(|_v| invocation_count += 1);
            p.set(6);
        }
        // Then
        assert_eq!(invocation_count, 1);
    }

    #[test]
    fn clone_observe_independent() {
        // Given
        let mut p_invocation_count = 0;
        let mut p2_invocation_count = 0;
        // When
        {
            let mut p = LocalProp::new(5);
            p.changed().subscribe(|_v| p_invocation_count += 1);
            let mut p2 = p.clone();
            p2.changed().subscribe(|_v| p2_invocation_count += 1);
            p.set(6);
            p2.set(6);
        }
        // Then
        assert_eq!(p_invocation_count, 1);
        assert_eq!(p2_invocation_count, 1);
    }
}
