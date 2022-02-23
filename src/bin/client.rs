use std::env;
use std::time::Duration;

use stiltesttask::client;


#[tokio::main]
async fn main() {
    let n = match env::args().nth(1).and_then(|n| n.parse::<usize>().ok()) {
        Some(n) => {
            if n > 0 && n <= 100 { n }
            else {
                eprintln!("Значение параметра (количество отправляемых сообщений) должно быть в диапазоне от 1 до 100.");
                std::process::exit(0);
            }
        },
        None => {
            eprintln!("Не задан параметр (количество отправляемых сообщений).");
            std::process::exit(0);
        }
    };

    let c = client::Config {
        uri: "http://127.0.0.1:31589/".parse().unwrap(),
        message_count: n,
        waiting: Duration::from_secs(2),
    };

    client::run(c).await;
}