use std::time::{Duration, Instant};

const URL: &str = "https://api.control-dev.ru/openapi.json";
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let url = args.first().map(String::as_str).unwrap_or(URL);
    let n = args
        .get(1)
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(8);

    bench_reqwest(url, n, ReqwestTls::Rustls, false);
    bench_reqwest(url, n, ReqwestTls::Rustls, true);
    bench_reqwest_resolved(url, n);
    bench_reqwest_raw_gzip(url, n);
}

#[derive(Clone, Copy)]
enum ReqwestTls {
    Rustls,
}

fn bench_reqwest(url: &str, n: usize, tls: ReqwestTls, http2_prior: bool) {
    let mut builder = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(12))
        .pool_idle_timeout(Duration::from_secs(90));
    builder = match tls {
        ReqwestTls::Rustls => builder.use_rustls_tls(),
    };
    if http2_prior {
        builder = builder.http2_prior_knowledge();
    }
    let client = builder.build().expect("reqwest client");
    let mut rows = Vec::with_capacity(n);
    for _ in 0..n {
        let started = Instant::now();
        let response = client
            .get(url)
            .header("Accept", "application/json, */*")
            .send()
            .expect("reqwest request");
        let status = response.status().as_u16();
        let proto = format!("{:?}", response.version());
        let encoding = response
            .headers()
            .get(reqwest::header::CONTENT_ENCODING)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let body = response.text().expect("reqwest body");
        let fetch_ms = started.elapsed().as_secs_f64() * 1000.0;
        let parse_started = Instant::now();
        let _: serde_json::Value = serde_json::from_str(&body).expect("json");
        rows.push(Row {
            fetch_ms,
            parse_ms: parse_started.elapsed().as_secs_f64() * 1000.0,
            status,
            len: body.len(),
            proto,
            encoding,
        });
    }
    print_rows(
        match (tls, http2_prior) {
            (ReqwestTls::Rustls, false) => "reqwest-rustls",
            (ReqwestTls::Rustls, true) => "reqwest-rustls-h2",
        },
        &rows,
    );
}

fn bench_reqwest_raw_gzip(url: &str, n: usize) {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(12))
        .pool_idle_timeout(Duration::from_secs(90))
        .use_rustls_tls()
        .no_gzip()
        .build()
        .expect("reqwest raw gzip client");
    let mut rows = Vec::with_capacity(n);
    for _ in 0..n {
        let started = Instant::now();
        let response = client
            .get(url)
            .header("Accept", "application/json, */*")
            .header("Accept-Encoding", "gzip")
            .send()
            .expect("reqwest raw gzip request");
        let status = response.status().as_u16();
        let proto = format!("{:?}", response.version());
        let encoding = response
            .headers()
            .get(reqwest::header::CONTENT_ENCODING)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let body = response.bytes().expect("reqwest raw gzip body");
        let fetch_ms = started.elapsed().as_secs_f64() * 1000.0;
        rows.push(Row {
            fetch_ms,
            parse_ms: 0.0,
            status,
            len: body.len(),
            proto,
            encoding,
        });
    }
    print_rows("reqwest-raw-gzip", &rows);
}

fn bench_reqwest_resolved(url: &str, n: usize) {
    let Some(host) = url
        .strip_prefix("https://")
        .and_then(|rest| rest.split('/').next())
    else {
        return;
    };
    let Ok(addr) = "155.212.189.234:443".parse() else {
        return;
    };
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(12))
        .pool_idle_timeout(Duration::from_secs(90))
        .use_rustls_tls()
        .resolve(host, addr)
        .build()
        .expect("reqwest resolved client");
    let mut rows = Vec::with_capacity(n);
    for _ in 0..n {
        let started = Instant::now();
        let response = client
            .get(url)
            .header("Accept", "application/json, */*")
            .send()
            .expect("reqwest resolved request");
        let status = response.status().as_u16();
        let proto = format!("{:?}", response.version());
        let encoding = response
            .headers()
            .get(reqwest::header::CONTENT_ENCODING)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let body = response.text().expect("reqwest resolved body");
        let fetch_ms = started.elapsed().as_secs_f64() * 1000.0;
        let parse_started = Instant::now();
        let _: serde_json::Value = serde_json::from_str(&body).expect("json");
        rows.push(Row {
            fetch_ms,
            parse_ms: parse_started.elapsed().as_secs_f64() * 1000.0,
            status,
            len: body.len(),
            proto,
            encoding,
        });
    }
    print_rows("reqwest-resolved", &rows);
}

struct Row {
    fetch_ms: f64,
    parse_ms: f64,
    status: u16,
    len: usize,
    proto: String,
    encoding: String,
}

fn print_rows(name: &str, rows: &[Row]) {
    let mut fetch = rows.iter().map(|row| row.fetch_ms).collect::<Vec<_>>();
    fetch.sort_by(|a, b| a.total_cmp(b));
    let avg = fetch.iter().sum::<f64>() / fetch.len().max(1) as f64;
    println!(
        "{name}: min {:.1}ms p50 {:.1}ms avg {:.1}ms max {:.1}ms",
        fetch[0],
        fetch[fetch.len() / 2],
        avg,
        fetch[fetch.len() - 1]
    );
    for (idx, row) in rows.iter().enumerate() {
        println!(
            "  #{:02} fetch {:.1}ms parse {:.2}ms status {} bytes {} proto {} enc {}",
            idx + 1,
            row.fetch_ms,
            row.parse_ms,
            row.status,
            row.len,
            row.proto,
            if row.encoding.is_empty() {
                "-"
            } else {
                &row.encoding
            }
        );
    }
}
