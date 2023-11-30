pub trait ReadAccess {}
pub trait WriteAccess: ReadAccess {}

#[derive(Copy, Clone, Debug, Default)]
pub struct ReadScope(pub(crate) ());
impl ReadAccess for ReadScope {}

#[derive(Copy, Clone, Debug, Default)]
pub struct ReadWriteScope(pub(crate) ());
impl ReadAccess for ReadWriteScope {}
impl WriteAccess for ReadWriteScope {}
