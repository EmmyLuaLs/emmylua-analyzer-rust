#[derive(Debug)]
#[allow(unused)]
pub enum TokenNodeChange {
    Remove,
    AddLeft(String),
    AddRight(String),
    ReplaceWith(String),
}

#[allow(unused)]
#[derive(Debug, Clone, Copy)]
pub enum TokenExpected {
    Space(usize),
    MaxSpace(usize),
}
