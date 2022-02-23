use std::time::{Duration, Instant};

use hyper::Uri;
use hyper::body::Body;
use hyper::client::Client;
use hyper::client::connect::HttpConnector;

use futures::stream::{FuturesUnordered, StreamExt};

use tokio::time;

pub struct Config {
    pub uri: Uri,
    pub message_count: usize,
    pub waiting: Duration,
}

struct Statistics {
    response_count: usize,
    min_duration: Duration,
    avg_duration: Duration,
    max_duration: Duration,
    total_duation: Duration,
}

pub async fn run(Config { uri, message_count, waiting }: Config) {
    let client = Client::builder()
        .http2_only(true)
        .build_http::<Body>();

    let mut durations = Vec::new();
    durations.reserve(message_count);

    let mut futs = FuturesUnordered::new();
    for _ in 0..message_count {
        futs.push(measure_response_time(client.clone(), uri.clone()));
    }

    let timei = Instant::now();
    
    if let Ok(Some(Some(d))) = time::timeout(waiting, futs.next()).await {
        durations.push(d);

        while let Some(Some(d)) = futs.next().await {
            durations.push(d);
        }
    }

    let total_duration = timei.elapsed();
    let sum = durations.iter().sum::<Duration>();

    let statistics = Statistics {
        response_count: durations.len(),
        min_duration: durations.iter().min().unwrap_or(&Duration::ZERO).clone(),
        avg_duration: if !durations.is_empty() { sum / durations.len() as u32 } else { Duration::ZERO },
        max_duration: durations.iter().max().unwrap_or(&Duration::ZERO).clone(),
        total_duation: total_duration
    };

    print_statistics(&statistics);
}

async fn measure_response_time(client: Client<HttpConnector>, uri: Uri) -> Option<Duration> {
    let timei = Instant::now();
    let r = client.get(uri).await;
    let duration = timei.elapsed();
    if r.is_ok() { Some(duration) } else { None }
}

fn print_statistics(statistics: &Statistics) {
    println!("--- Статистика ---\n\
    Количество сообщений, на которые был получен ответ - {}\n\
    Минимальное время ответа сервера на сообщение - {} мс\n\
    Среднее время ответа сервера на сообщение - {} мс\n\
    Максимальное время ответа сервера на сообщение - {} мс\n\
    Общее время обмена данными с сервером - {} мс",
    statistics.response_count,
    statistics.min_duration.as_millis(),
    statistics.avg_duration.as_millis(),
    statistics.max_duration.as_millis(),
    statistics.total_duation.as_millis());
}