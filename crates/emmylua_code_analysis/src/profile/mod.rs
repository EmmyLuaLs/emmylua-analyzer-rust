use log::info;
use std::time::Instant;

pub struct Profile<'a> {
    name: &'a str,
    start: Instant,
}

#[allow(unused)]
impl<'a> Profile<'a> {
    pub fn new(name: &'a str) -> Self {
        Self {
            name,
            start: Instant::now(),
        }
    }

    pub fn cond_new(name: &'a str, cond: bool) -> Option<Self> {
        if cond {
            Some(Self::new(name))
        } else {
            None
        }
    }
}

impl<'a> Drop for Profile<'a> {
    fn drop(&mut self) {
        let duration = self.start.elapsed();
        info!("{}: cost {:?}", self.name, duration);
    }
}
