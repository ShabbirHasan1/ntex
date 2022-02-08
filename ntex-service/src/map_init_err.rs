use std::{future::Future, marker::PhantomData, pin::Pin, task::Context, task::Poll};

use super::ServiceFactory;

/// `MapInitErr` service combinator
pub struct MapInitErr<A, F, E> {
    a: A,
    f: F,
    e: PhantomData<E>,
}

impl<A, F, E> MapInitErr<A, F, E> {
    /// Create new `MapInitErr` combinator
    pub(crate) fn new(a: A, f: F) -> Self {
        Self {
            a,
            f,
            e: PhantomData,
        }
    }
}

impl<A, F, E> Clone for MapInitErr<A, F, E>
where
    A: Clone,
    F: Clone,
{
    fn clone(&self) -> Self {
        Self {
            a: self.a.clone(),
            f: self.f.clone(),
            e: PhantomData,
        }
    }
}

impl<A, R, C, F, E> ServiceFactory<R, C> for MapInitErr<A, F, E>
where
    A: ServiceFactory<R, C>,
    F: Fn(A::InitError) -> E + Clone,
{
    type Response = A::Response;
    type Error = A::Error;

    type Service = A::Service;
    type InitError = E;
    type Future = MapInitErrFuture<A::Future, A::Service, A::InitError, F, E>;

    fn new_service(&self, cfg: C) -> Self::Future {
        MapInitErrFuture {
            fut: self.a.new_service(cfg),
            f: self.f.clone(),
        }
    }
}

pin_project_lite::pin_project! {
    pub struct MapInitErrFuture<Fut, Srv, Err, F, E>
    where
        F: Fn(Err) -> E,
        Fut: Future<Output = Result<Srv, Err>>,
    {
        f: F,
        #[pin]
        fut: Fut,
    }
}

impl<Fut, Srv, Err, F, E> Future for MapInitErrFuture<Fut, Srv, Err, F, E>
where
    F: Fn(Err) -> E,
    Fut: Future<Output = Result<Srv, Err>>,
{
    type Output = Result<Srv, E>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        this.fut.poll(cx).map_err(this.f)
    }
}

#[cfg(test)]
mod tests {
    use crate::{fn_factory_with_config, fn_service, pipeline_factory, ServiceFactory};

    #[ntex::test]
    async fn map_init_err() {
        let factory = pipeline_factory(fn_factory_with_config(|err: bool| async move {
            if err {
                Err(())
            } else {
                Ok(fn_service(|i: usize| async move { Ok::<_, ()>(i * 2) }))
            }
        }))
        .map_init_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "err"))
        .clone();

        assert!(factory.new_service(true).await.is_err());
        assert!(factory.new_service(false).await.is_ok());
    }
}
