/// A wrapper that implements Clone but not really by cloning the wrapped value but by
/// creating the inner type's default value.
///
/// This is useful if you have a large nested value of which almost anything inside must be cloned
/// but you also have a few values in there that don't suit themselves to be cloned (e.g. compiled
/// scripts). This must be well documented though, it's a very surprising behavior.
///
/// Alternatives to be considered before reaching out to this:
///
/// - Making the whole graph not cloneable
/// - Using `Rc` or `Arc` (is cloneable but means that the inner value is not "standalone" anymore,
///   can be accessed from multiple places or in case of `Arc` even threads ... and it means that
///   you have less control over when and in which thread deallocation happens).
/// - Writing a dedicated method (not `clone`) which makes it clear that this is not a standard
///   clone operation.
#[derive(Debug)]
pub struct CloneAsDefault<T>(T);

impl<T> CloneAsDefault<T> {
    pub fn new(value: T) -> Self {
        Self(value)
    }

    pub fn get(&self) -> &T {
        &self.0
    }
}

impl<T: Default> Clone for CloneAsDefault<T> {
    fn clone(&self) -> Self {
        Self(T::default())
    }
}
