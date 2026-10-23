//! Staggered dual-stack TCP dialing with the caller's unchanged overall budget.
use std::{collections::VecDeque, future::Future, io, net::SocketAddr, time::Duration};
use tokio::{
    net::TcpStream,
    task::JoinSet,
    time::{sleep_until, Instant},
};

fn interleave(addresses: Vec<SocketAddr>) -> Vec<SocketAddr> {
    let mut unique = Vec::new();
    for address in addresses {
        if !unique.contains(&address) {
            unique.push(address);
        }
    }
    let first_v4 = unique.first().is_none_or(SocketAddr::is_ipv4);
    let (mut first, mut second): (VecDeque<_>, VecDeque<_>) =
        unique.into_iter().partition(|a| a.is_ipv4() == first_v4);
    let mut result = Vec::new();
    while !first.is_empty() || !second.is_empty() {
        if let Some(a) = first.pop_front() {
            result.push(a);
        }
        if let Some(a) = second.pop_front() {
            result.push(a);
        }
    }
    result
}
async fn race<C, F>(
    addresses: Vec<SocketAddr>,
    delay: Duration,
    connect: C,
) -> io::Result<TcpStream>
where
    C: Fn(SocketAddr) -> F,
    F: Future<Output = io::Result<TcpStream>> + Send + 'static,
{
    let mut remaining = interleave(addresses).into_iter();
    let first = remaining.next().ok_or_else(|| {
        io::Error::new(io::ErrorKind::AddrNotAvailable, "DNS returned no addresses")
    })?;
    let mut jobs = JoinSet::new();
    jobs.spawn(connect(first));
    let mut next = Instant::now() + delay;
    let mut last = io::Error::new(io::ErrorKind::ConnectionRefused, "TCP connection failed");
    loop {
        tokio::select! {
            result=jobs.join_next(), if !jobs.is_empty()=>{
                match result {
                    Some(Ok(Ok(stream)))=>return Ok(stream),
                    Some(Ok(Err(error)))=>last=error,
                    Some(Err(error))=>last=io::Error::other(error),
                    None=>{}
                }
                if jobs.is_empty(){if let Some(address)=remaining.next(){jobs.spawn(connect(address));next=Instant::now()+delay;}else{return Err(last);}}
            }
            _=sleep_until(next),if remaining.len()>0&&jobs.len()<4=>{
                if let Some(address)=remaining.next(){jobs.spawn(connect(address));next=Instant::now()+delay;}
            }
        }
    }
}
pub async fn connect(host: &str, port: u16, timeout: Duration) -> anyhow::Result<TcpStream> {
    use anyhow::Context;
    tokio::time::timeout(timeout, async {
        let addresses = tokio::net::lookup_host((host, port)).await?.collect();
        race(addresses, Duration::from_millis(250), |a| {
            TcpStream::connect(a)
        })
        .await
    })
    .await
    .context("DNS/TCP connection deadline exceeded")?
    .context("DNS/TCP connection failed")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn families_alternate_and_duplicates_are_removed() {
        let a: SocketAddr = "[::1]:443".parse().unwrap();
        let b = "[::2]:443".parse().unwrap();
        let c = "127.0.0.1:443".parse().unwrap();
        assert_eq!(interleave(vec![a, b, a, c]), vec![a, c, b]);
    }
    #[tokio::test]
    async fn stalled_first_address_does_not_block_working_alternative() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target = listener.local_addr().unwrap();
        let stalled: SocketAddr = "127.0.0.2:1".parse().unwrap();
        let stream = tokio::time::timeout(
            Duration::from_secs(2),
            race(
                vec![stalled, target],
                Duration::from_millis(25),
                move |address| async move {
                    if address == stalled {
                        std::future::pending::<()>().await;
                    }
                    TcpStream::connect(address).await
                },
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(stream.peer_addr().unwrap(), target);
    }
    #[tokio::test]
    async fn closed_port_and_empty_resolution_return_errors() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target = listener.local_addr().unwrap();
        drop(listener);
        assert!(connect("127.0.0.1", target.port(), Duration::from_secs(1))
            .await
            .is_err());
        assert!(race(vec![], Duration::from_millis(1), TcpStream::connect)
            .await
            .is_err());
    }
}
