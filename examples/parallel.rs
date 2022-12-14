use std::{error::Error, sync::Arc, time::Instant};

use alhc::*;

use pollster::FutureExt;

fn main() {
    async {
        let client = Arc::new(ClientBuilder::default().build());

        let mut success = 0;
        let mut failed = 0;

        println!("Sending httpbin");

        for (i, r) in futures::future::join_all((0..10).map(|i| {
            let client = client.clone();
            async move {
                let instant = Instant::now();
                let r = client.get("https://httpbin.org/anything")?;
                // println!("Requesting {}", i);
                let r = r.await?.recv_string().await;
                if let Err(err) = &r {
                    println!("Request {} Error: {}", i, err);
                } else {
                    let e = instant.elapsed().as_millis();
                    println!("Request {} ok in {}ms", i, e);
                }
                Ok::<String, Box<dyn Error>>(r?)
            }
        }))
        .await
        .into_iter()
        .enumerate()
        {
            match r {
                Ok(_) => {
                    success += 1;
                    // println!("Request {} ok", i);
                }
                Err(err) => {
                    failed += 1;
                    println!("Request {} Error: {}", i, err);
                }
            }
        }

        println!(
            "Sent {} requests, {} succeed, {} failed",
            success + failed,
            success,
            failed
        );

        Result::Ok(())
    }
    .block_on()
    .unwrap();
}
