use std::time::Instant;

use alhc::{DynResult, prelude::*};

use pollster::FutureExt;

fn main() {
    async {
        let client = get_client_builder().build().unwrap();

        let mut success = 0;
        let mut failed = 0;

        println!("Sending httpbin");

        for i in 0..10 {
            let instant = Instant::now();
            println!("Requesting {}", i);

            let req = match client.post("https://httpbin.org/anything") {
                Ok(r) => r.body_string("Hello World!".repeat(1000)),
                Err(e) => {
                    println!("Request {} Error: {:?}", i, e);
                    failed += 1;
                    continue;
                }
            };
            let r = match req.await {
                Ok(r) => r.recv_string().await,
                Err(e) => {
                    println!("Request {} Error: {:?}", i, e);
                    failed += 1;
                    continue;
                }
            };
            match r {
                Ok(body) => {
                    let e = instant.elapsed().as_millis();
                    println!("Request {i} ok in {e}ms : {body:?}");
                    success += 1;
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

        DynResult::Ok(())
    }
    .block_on()
    .unwrap();
}
