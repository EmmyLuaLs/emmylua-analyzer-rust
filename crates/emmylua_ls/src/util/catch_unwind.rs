//! Catch panics from a future without spawning an extra task.
//!
//! Replaces the old `tokio::spawn(fut).await.ok()` pattern used to turn a
//! cancellation panic into a normal Result.

use std::any::Any;
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::panic::catch_unwind as std_catch_unwind;
use std::pin::Pin;
use std::task::{Context, Poll};

pub fn catch_unwind<F>(future: F) -> CatchUnwind<F> {
    CatchUnwind(future)
}

pub struct CatchUnwind<F>(F);
impl<F: Future> Future for CatchUnwind<F> {
    type Output = Result<F::Output, Box<dyn Any + Send>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // SAFETY: structural projection; F is never moved after pinning.
        let this = unsafe { self.get_unchecked_mut() };
        let future = unsafe { Pin::new_unchecked(&mut this.0) };
        match std_catch_unwind(AssertUnwindSafe(move || future.poll(cx))) {
            Ok(Poll::Ready(value)) => Poll::Ready(Ok(value)),
            Ok(Poll::Pending) => Poll::Pending,
            Err(payload) => Poll::Ready(Err(payload)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn passes_values() {
        assert_eq!(catch_unwind(async { 42 }).await.unwrap(), 42);
    }

    #[tokio::test]
    async fn catches_panic() {
        assert!(catch_unwind(async { panic!("boom") }).await.is_err());
    }
}
