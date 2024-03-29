use std::{error::Error, time::Instant};

use alhc::prelude::*;
use alhc::*;

use pollster::FutureExt;

fn main() {
    // tracing_subscriber::fmt()
    //     .with_max_level(tracing::Level::DEBUG)
    //     .init();
    async {
        let client = get_client_builder().build().unwrap();

        let mut success = 0;
        let mut failed = 0;

        println!("Sending httpbin");

        for i in 0..10 {
            let instant = Instant::now();
            println!("Requesting {}", i);
            let r = client
                .post("https://httpbin.org/anything")?
                .body_string("Hello World!".repeat(1000))
                .await?
                .recv_string()
                .await;
            if let Err(err) = &r {
                println!("Request {} Error: {:?}", i, err);
            } else {
                let e = instant.elapsed().as_millis();
                println!("Request {i} ok in {e}ms : {r:?}");
            }

            match r {
                Ok(_) => {
                    success += 1;
                    println!("Request {} ok", i);
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

        Ok::<(), Box<dyn Error>>(())
    }
    .block_on()
    .unwrap();
}
