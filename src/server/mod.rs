use std::error::Error;
use std::time::Duration;

use tokio::{signal, time};
use tokio::sync::broadcast::{self, Receiver};
use tokio::net::{TcpListener, TcpStream, ToSocketAddrs};

use futures::future;
use futures::stream::{FuturesUnordered, StreamExt};

use hyper::service::Service;
use hyper::server::conn::Http;
use hyper::{Body, Request, Response};

mod statistics;

use statistics::{StatisticsSender, StatisticsService};

pub struct Server<A, S> {
    addr: A,
    max_connections: usize,
    http: Http,
    service: S,
}

pub struct Builder<A> {
    addr: A,
    max_connections: usize,
    http: Http,
}

impl<A> Builder<A>
where
    A: ToSocketAddrs,
{
    pub fn max_connections(mut self, num: usize) -> Self {
        self.max_connections = num;
        self
    }

    pub fn http2_only(mut self, val: bool) -> Self {
        self.http.http2_only(val);
        self
    }

    pub fn service<S>(self, service: S) -> Server<A, S> {
        Server {
            addr: self.addr,
            max_connections: self.max_connections,
            http: self.http,
            service: service,
        }
    }
}

impl<A> Server<A, ()> {
    const DEFAULT_MAX_CONNECTIONS: usize = 25_000;

    pub fn bind(addrs: A) -> Builder<A> {
        Builder {
            addr: addrs,
            max_connections: Self::DEFAULT_MAX_CONNECTIONS,
            http: Http::new(),
        }
    }
}

impl<A, S> Server<A, S>
where
    A: ToSocketAddrs + 'static,
    S: Service<Request<Body>, Response = Response<Body>> + Send + Clone + 'static,
    S::Error: Error + Send + Sync,
    S::Future: Send,
{
    const DEFAULT_SHUTDOWN_DELAY: Duration = Duration::from_secs(5);

    async fn process_stream(
        stream: TcpStream,
        http: Http,
        service: S,
        stat_sender: StatisticsSender,
        mut shutdown: Receiver<()>,
    ) {
        let service = StatisticsService::new(service);
        let statistics = service.statistics();

        let connection = http.serve_connection(stream, service);

        let shutdown_signal = shutdown.recv();

        tokio::pin!(connection);

        let _ = tokio::select! {
            r = connection.as_mut() => r,
            _ = shutdown_signal => {
                connection.as_mut().graceful_shutdown();
                connection.await
            }
        };

        let statistics = std::mem::take(&mut *statistics.lock());
        let _ = stat_sender.send(statistics).await;
    }

    pub async fn run(self) {
        let listener = TcpListener::bind(self.addr)
            .await
            .expect("Can't bind to the address.");
        let mut jhs = FuturesUnordered::new();

        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
        drop(shutdown_rx);

        let (stat_sender, collecting) = statistics::collecting();
        let stat_jh = tokio::spawn(collecting);

        loop {
            tokio::select! {
                Ok((stream, _)) = listener.accept(), if jhs.len() < self.max_connections => {
                    jhs.push(tokio::spawn(Self::process_stream(stream, self.http.clone(), self.service.clone(), stat_sender.clone(), shutdown_tx.subscribe())));
                },
                _ = jhs.next(), if !jhs.is_empty() => {},
                _ = signal::ctrl_c() => break,
                else => break,
            }
        }

        drop(stat_sender);

        if shutdown_tx.send(()).is_ok() {
            let _ = time::timeout(Self::DEFAULT_SHUTDOWN_DELAY, future::join_all(jhs)).await;
        }

        let _ = stat_jh.await;
    }
}
