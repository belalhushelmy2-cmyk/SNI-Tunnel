//! Actual server TCP connect latency. This is not an HTTP or tunnel health test.
use std::time::Duration;
pub async fn measure(address: &str, port: u16) -> anyhow::Result<u128> {
    crate::netcheck::tcp_connect(address, port, Duration::from_secs(2))
        .await
        .map(|(_, ms)| ms)
}
pub fn display(reading: Option<u128>) -> String {
    match reading {
        Some(0) => "TCP latency <1 ms · live / 3 s".into(),
        Some(ms) => format!("TCP latency {ms} ms · live / 3 s"),
        None => "TCP latency unavailable · retrying / 3 s".into(),
    }
}
#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn actual_listener_connects_and_closed_port_is_unavailable() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(super::measure("127.0.0.1", port).await.is_ok());
        drop(listener);
        assert!(super::measure("127.0.0.1", port).await.is_err());
        assert!(super::display(None).contains("unavailable"));
    }
}
