use std::cmp;
use std::error::Error;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, SystemTime};

use tokio::sync::mpsc;

use hyper::service::Service;

use parking_lot::Mutex;

pub type StatisticsReciever = mpsc::Receiver<Option<RawStatistics>>;
pub type StatisticsSender = mpsc::Sender<Option<RawStatistics>>;

pub struct RawStatistics {
    pub first_request_time: SystemTime,
    pub last_response_time: SystemTime,
    pub processing_durations: Vec<Duration>,
}

pub struct StatisticsService<T> {
    inner: T,
    statistics: Arc<Mutex<Option<RawStatistics>>>,
}

impl<I> StatisticsService<I> {
    pub fn new(inner: I) -> Self {
        Self {
            inner: inner,
            statistics: Arc::new(Mutex::new(None)),
        }
    }

    pub fn statistics(&self) -> Arc<Mutex<Option<RawStatistics>>> {
        Arc::clone(&self.statistics)
    }
}

impl<T, Request> Service<Request> for StatisticsService<T>
where
    T: Service<Request>,
    T::Future: Send + 'static,
    T::Error: Into<Box<dyn Error + Send + Sync>>,
{
    type Response = T::Response;
    type Error = T::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let ifuture = self.inner.call(req);
        let statistics = self.statistics();

        let f = async move {
            let request_time = SystemTime::now();
            let result = ifuture.await;
            let response_time = SystemTime::now();
            let duration = response_time
                .duration_since(request_time)
                .unwrap_or_default();
            let new_statistics = RawStatistics {
                first_request_time: request_time,
                last_response_time: response_time,
                processing_durations: vec![duration],
            };

            {
                let mut statistics = statistics.lock();
                if let Some(s) = statistics.as_mut() {
                    s.last_response_time = response_time;
                    s.processing_durations.push(duration);
                } else {
                    *statistics = Some(new_statistics);
                }
            }

            result
        };

        Box::pin(f)
    }
}

pub fn collecting() -> (StatisticsSender, impl Future<Output = ()>) {
    let (stat_tx, stat_rx) = mpsc::channel::<Option<RawStatistics>>(100);
    (stat_tx, statistics_collecting(stat_rx))
}

async fn statistics_collecting(mut sreciever: StatisticsReciever) {
    let mut min_duration = Duration::MAX;
    let mut max_duration = Duration::ZERO;
    let mut acc_duration = Duration::ZERO;
    let mut served_count: usize = 0;
    let mut unserved_count: usize = 0;

    while let Some(statistics) = sreciever.recv().await {
        if let Some(statistics) = statistics {
            let s = process_statistics(&statistics);

            print_connection_statistics(&s);

            min_duration = cmp::min(s.conenction_duration, min_duration);
            max_duration = cmp::max(s.conenction_duration, max_duration);
            acc_duration += s.conenction_duration;
            served_count += 1;
        } else {
            unserved_count += 1;
        }
    }

    let avg_duration;
    if served_count > 0 {
        avg_duration = acc_duration / (served_count as u32);
    } else {
        avg_duration = Duration::ZERO;
        min_duration = Duration::ZERO;
    }

    let statistics = GeneralStatistics {
        served_count: served_count,
        unserved_count: unserved_count,
        min_duration: min_duration,
        avg_duration: avg_duration,
        max_duration: max_duration,
    };

    print_general_statistics(&statistics);
}
struct ProcessedConnectionStatistics {
    request_count: usize,
    min_duration: Duration,
    avg_duration: Duration,
    max_duration: Duration,
    conenction_duration: Duration,
}

struct GeneralStatistics {
    served_count: usize,
    unserved_count: usize,
    min_duration: Duration,
    avg_duration: Duration,
    max_duration: Duration,
}

fn process_statistics(statistics: &RawStatistics) -> ProcessedConnectionStatistics {
    let mut min = Duration::MAX;
    let mut max = Duration::ZERO;
    let mut acc = Duration::ZERO;

    for duration in &statistics.processing_durations {
        max = cmp::max(*duration, max);
        min = cmp::min(*duration, min);
        acc += *duration;
    }

    ProcessedConnectionStatistics {
        request_count: statistics.processing_durations.len(),
        min_duration: min,
        avg_duration: acc / (statistics.processing_durations.len() as u32),
        max_duration: max,
        conenction_duration: statistics
            .last_response_time
            .duration_since(statistics.first_request_time)
            .unwrap_or_default(),
    }
}

fn print_connection_statistics(statistics: &ProcessedConnectionStatistics) {
    println!("\n\
    --- Статистика подключения ---\n\
    Количество поступивших запросов - {}\n\
    Минимальное время обработки запроса - {} мс\n\
    Среднее время обработки запроса - {} мс\n\
    Максимальное время обработки запроса - {} мс\n\
    Время сеанса - {} мс",
        statistics.request_count,
        statistics.min_duration.as_millis(),
        statistics.avg_duration.as_millis(),
        statistics.max_duration.as_millis(),
        statistics.conenction_duration.as_millis()
    );
}

fn print_general_statistics(statistics: &GeneralStatistics) {
    println!("\n\
    --- Общая статистика ---\n\
    Количество обработанных клиентских подключений - {}\n\
    Минимальное время сеанса с клиентом - {} мс\n\
    Среднее время сеанса с клиентом - {} мс\n\
    Максимальное время сеанса с клиентом - {} мс\n\
    Количество клиентов не дождавшихся обсулживания - {}",
        statistics.served_count,
        statistics.min_duration.as_millis(),
        statistics.avg_duration.as_millis(),
        statistics.max_duration.as_millis(),
        statistics.unserved_count
    );
}
