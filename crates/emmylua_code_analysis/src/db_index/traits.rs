use crate::FileId;

pub trait LuaIndex {
    fn remove(&mut self, file_id: FileId);

    fn clear(&mut self);
}
