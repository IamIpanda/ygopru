use std::convert::Infallible;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Poll;

use tower::Service;
use tower::ServiceExt;
use tower::util::BoxCloneService;

#[derive(Debug)]
pub struct State {
    pub data: anymap3::Map<dyn std::any::Any + Send + Sync>,
}

impl State {
    pub fn new() -> Self {
        State { data: Default::default() }
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

pub trait IntoResponse<Res> {
    fn into_response(self) -> Res;
}

pub trait FromRequest<Req, Res>: Sized
where
    Req: Send + 'static,
    Res: Send + 'static,
{
    fn from_request(bundle: &mut Bundle<Req, Res>) -> Option<Self>;
}

#[derive(Debug)]
pub struct Bundle<Req, Res> {
    pub request: Req,
    pub state: State,
    pub response: Res,
}

pub trait Handler<T, Req, Res>: Clone + Send + Sync + Sized + 'static {
    type Future: Future<Output = Option<Res>> + Send;

    fn call(&self, bundle: &mut Bundle<Req, Res>) -> Self::Future;
}

impl<F, Fut, Output, Req, Res> Handler<((),), Req, Res> for F
where
    F: Fn() -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Output> + Send + 'static,
    Output: IntoResponse<Res> + 'static,
    Req: Send + 'static,
    Res: Send + 'static,
{
    type Future = Pin<Box<dyn Future<Output = Option<Res>> + Send>>;

    fn call(&self, _bundle: &mut Bundle<Req, Res>) -> Self::Future {
        let fut = (self)();
        Box::pin(async move { Some(fut.await.into_response()) })
    }
}

impl<F, Output, Req, Res> Handler<Option<()>, Req, Res> for F
where
    F: Fn() -> Output + Clone + Send + Sync + 'static,
    Output: IntoResponse<Res> + 'static,
    Req: Send + 'static,
    Res: Send + 'static,
{
    type Future = std::future::Ready<Option<Res>>;

    fn call(&self, _bundle: &mut Bundle<Req, Res>) -> Self::Future {
        std::future::ready(Some((self)().into_response()))
    }
}

macro_rules! impl_handler {
    ([$($ty:ident),*], $last:ident) => {
        #[allow(non_snake_case, unused_mut)]
        impl<F, Fut, Output, Req, Res, $($ty,)* $last> Handler<((), $($ty,)* $last,), Req, Res> for F
        where
            F: Fn($($ty,)* $last,) -> Fut + Clone + Send + Sync + 'static,
            Fut: Future<Output = Output> + Send + 'static,
            Output: IntoResponse<Res> + 'static,
            Req: Send + 'static,
            Res: Send + 'static,
            $( $ty: FromRequest<Req, Res> + Send + 'static, )*
            $last: FromRequest<Req, Res> + Send + 'static,
        {
            type Future = Pin<Box<dyn Future<Output = Option<Res>> + Send>>;

            fn call(&self, bundle: &mut Bundle<Req, Res>) -> Self::Future {
                $(
                    let $ty = match $ty::from_request(bundle) {
                        Some(value) => value,
                        None => return Box::pin(std::future::ready(None)),
                    };
                )*

                let $last = match $last::from_request(bundle) {
                    Some(value) => value,
                    None => return Box::pin(std::future::ready(None)),
                };

                let handler = self.clone();
                Box::pin(async move {
                    let fut = handler($($ty,)* $last,);
                    Some(fut.await.into_response())
                })
            }
        }

        #[allow(non_snake_case, unused_mut)]
        impl<F, Output, Req, Res, $($ty,)* $last> Handler<Option<((), $($ty,)* $last,)>, Req, Res> for F
        where
            F: Fn($($ty,)* $last,) -> Output + Clone + Send + Sync + 'static,
            Output: IntoResponse<Res> + 'static,
            Req: Send + 'static,
            Res: Send + 'static,
            $( $ty: FromRequest<Req, Res> + Send, )*
            $last: FromRequest<Req, Res> + Send,
        {
            type Future = std::future::Ready<Option<Res>>;

            fn call(&self, bundle: &mut Bundle<Req, Res>) -> Self::Future {
                $(
                    let $ty = match $ty::from_request(bundle) {
                        Some(value) => value,
                        None => return std::future::ready(None),
                    };
                )*

                let $last = match $last::from_request(bundle) {
                    Some(value) => value,
                    None => return std::future::ready(None),
                };

                std::future::ready(Some((self)($($ty,)* $last,).into_response()))
            }
        }
    };
}

impl_handler!([], T1);
impl_handler!([T1], T2);
impl_handler!([T1, T2], T3);
impl_handler!([T1, T2, T3], T4);
impl_handler!([T1, T2, T3, T4], T5);
impl_handler!([T1, T2, T3, T4, T5], T6);
impl_handler!([T1, T2, T3, T4, T5, T6], T7);
impl_handler!([T1, T2, T3, T4, T5, T6, T7], T8);
impl_handler!([T1, T2, T3, T4, T5, T6, T7, T8], T9);
impl_handler!([T1, T2, T3, T4, T5, T6, T7, T8, T9], T10);
impl_handler!([T1, T2, T3, T4, T5, T6, T7, T8, T9, T10], T11);
impl_handler!([T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11], T12);
impl_handler!([T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12], T13);
impl_handler!([T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13], T14);
impl_handler!([T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14], T15);
impl_handler!([T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15], T16);

// ============================================================
// HandlerService
// ============================================================

pub struct HandlerService<H, T, Req, Res> {
    handler: H,
    _marker: PhantomData<fn() -> (T, Req, Res)>,
}

impl<H, T, Req, Res> HandlerService<H, T, Req, Res> {
    fn new(handler: H) -> Self {
        Self { handler, _marker: PhantomData }
    }
}

impl<H, T, Req, Res> Clone for HandlerService<H, T, Req, Res>
where
    H: Clone,
{
    fn clone(&self) -> Self {
        Self { handler: self.handler.clone(), _marker: PhantomData }
    }
}

impl<H, T, Req, Res> Service<Bundle<Req, Res>> for HandlerService<H, T, Req, Res>
where
    H: Handler<T, Req, Res> + Clone,
    Req: Send + 'static,
    Res: Send + 'static,
{
    type Response = Bundle<Req, Res>;
    type Error = Infallible;
    type Future = HandlerServiceFuture<H::Future, Req, Res>;

    fn call(&mut self, mut bundle: Bundle<Req, Res>) -> Self::Future {
        let future = self.handler.call(&mut bundle);
        HandlerServiceFuture { future, bundle: Some(bundle) }
    }

    fn poll_ready(&mut self, _: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}

pin_project_lite::pin_project! {
    pub struct HandlerServiceFuture<F, Req, Res> {
        #[pin]
        future: F,
        bundle: Option<Bundle<Req, Res>>,
    }
}

impl<F, Req, Res> Future for HandlerServiceFuture<F, Req, Res>
where
    F: Future<Output = Option<Res>>,
{
    type Output = Result<Bundle<Req, Res>, Infallible>;

    fn poll(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        match this.future.poll(cx) {
            Poll::Ready(Some(response)) => {
                let mut bundle = this.bundle.take().unwrap();
                bundle.response = response;
                Poll::Ready(Ok(bundle))
            }
            Poll::Ready(None) => {
                Poll::Ready(Ok(this.bundle.take().unwrap()))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

// ============================================================
// Call
// ============================================================

pub trait Call<Req, Res>: Send + Sync + 'static {
    fn call(&self, bundle: Bundle<Req, Res>) -> Pin<Box<dyn Future<Output = Bundle<Req, Res>> + Send>>;
    fn priority(&self) -> u8;
}

// ============================================================
// RegisteredHandler
// ============================================================

pub struct RegisteredHandler<Req, Res> {
    pub service: BoxCloneService<Bundle<Req, Res>, Bundle<Req, Res>, Infallible>,
    pub priority: u8,
    pub name: &'static str,
    pub module_name: &'static str,
}

unsafe impl<Req, Res> Sync for RegisteredHandler<Req, Res> {}

impl<Req, Res> RegisteredHandler<Req, Res>
where
    Req: Send + 'static,
    Res: Send + 'static,
{
    pub fn new<T: 'static>(
        priority: u8,
        name: &'static str,
        module_name: &'static str,
        handler: impl Handler<T, Req, Res>,
    ) -> Self {
        let service = HandlerService::new(handler);
        Self {
            priority,
            name,
            module_name,
            service: BoxCloneService::new(service),
        }
    }
}

impl<Req, Res> Service<Bundle<Req, Res>> for RegisteredHandler<Req, Res>
where
    Req: Send + 'static,
    Res: Send + 'static,
{
    type Response = Bundle<Req, Res>;
    type Error = Infallible;
    type Future = futures::future::BoxFuture<'static, Result<Bundle<Req, Res>, Infallible>>;

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, bundle: Bundle<Req, Res>) -> Self::Future {
        self.service.call(bundle)
    }
}

impl<Req, Res> Clone for RegisteredHandler<Req, Res> {
    fn clone(&self) -> Self {
        Self {
            service: self.service.clone(),
            priority: self.priority,
            name: self.name,
            module_name: self.module_name,
        }
    }
}

impl<Req, Res> Call<Req, Res> for RegisteredHandler<Req, Res>
where
    Req: Send + 'static,
    Res: Send + 'static,
{
    fn call(&self, bundle: Bundle<Req, Res>) -> Pin<Box<dyn Future<Output = Bundle<Req, Res>> + Send>> {
        let service = self.service.clone();
        Box::pin(async move { service.oneshot(bundle).await.unwrap() })
    }

    fn priority(&self) -> u8 {
        self.priority
    }
}

struct HandlerWrapper<H, T, Req, Res> {
    handler: H,
    priority: u8,
    _phantom: PhantomData<fn() -> (T, Req, Res)>,
}

impl<H, T, Req, Res> Clone for HandlerWrapper<H, T, Req, Res>
where
    H: Clone,
{
    fn clone(&self) -> Self {
        Self {
            handler: self.handler.clone(),
            priority: self.priority,
            _phantom: PhantomData,
        }
    }
}

impl<H, T, Req, Res> Call<Req, Res> for HandlerWrapper<H, T, Req, Res>
where
    H: Handler<T, Req, Res>,
    <H as Handler<T, Req, Res>>::Future: 'static,
    T: 'static,
    Req: Send + 'static,
    Res: Send + 'static,
{
    fn call(&self, bundle: Bundle<Req, Res>) -> Pin<Box<dyn Future<Output = Bundle<Req, Res>> + Send>> {
        let handler = self.handler.clone();
        Box::pin(async move {
            let mut bundle = bundle;
            if let Some(response) = handler.call(&mut bundle).await {
                bundle.response = response;
            }
            bundle
        })
    }

    fn priority(&self) -> u8 {
        self.priority
    }
}

pub struct FrozenHandler<Req, Res> {
    pub name: &'static str,
    pub module_name: &'static str,
    handler: Arc<dyn Call<Req, Res>>,
}

impl<Req, Res> Clone for FrozenHandler<Req, Res> {
    fn clone(&self) -> Self {
        Self {
            name: self.name,
            module_name: self.module_name,
            handler: self.handler.clone(),
        }
    }
}

impl<Req, Res> FrozenHandler<Req, Res>
where
    Req: Send + 'static,
    Res: Send + 'static,
{
    pub fn new<T: 'static, H: Handler<T, Req, Res>>(
        priority: u8,
        name: &'static str,
        module_name: &'static str,
        handler: H,
    ) -> Self
    where
        <H as Handler<T, Req, Res>>::Future: 'static,
    {
        let wrapper = HandlerWrapper {
            handler,
            priority,
            _phantom: PhantomData,
        };
        Self {
            name,
            module_name,
            handler: Arc::new(wrapper),
        }
    }
}

impl<Req, Res> Call<Req, Res> for FrozenHandler<Req, Res>
where
    Req: Send + 'static,
    Res: Send + 'static,
{
    fn call(&self, bundle: Bundle<Req, Res>) -> Pin<Box<dyn Future<Output = Bundle<Req, Res>> + Send>> {
        self.handler.call(bundle)
    }

    fn priority(&self) -> u8 {
        self.handler.priority()
    }
}
