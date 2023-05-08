use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use alhc::prelude::*;
use alhc::*;
use futures::future::join_all;
use pollster::FutureExt;

fn main() -> DynResult {
    async {
        let download_url = Arc::new(std::env::args().last().unwrap_or_default());

        if std::env::args().count() <= 1 || download_url.is_empty() {
            println!(
                "usage: {} <url>",
                std::env::args().collect::<Vec<_>>().join(" ")
            );
            return Ok(());
        }

        let chunk_amount = 4;
        let client = Arc::new({
            let mut c = get_client_builder().build().unwrap();
            c.set_timeout(Duration::from_secs(2));
            c
        });

        println!("Downloading from url: {}", download_url);

        let head_resp = client.head(&download_url)?.await?.recv().await?;
        if let Some(content_length) = head_resp
            .header("Content-Length")
            .and_then(|x| x.parse::<usize>().ok())
        {
            println!("Content Length: {} bytes", content_length);
            let time = Instant::now();
            let mut chunk_jobs = Vec::with_capacity(chunk_amount);
            let chunk_size = content_length / chunk_amount;
            for i in 0..chunk_amount {
                let start_pos = i * chunk_size;
                let end_pos = if i == chunk_amount - 1 {
                    content_length
                } else {
                    start_pos + chunk_size
                };
                let client = client.clone();
                let download_url = download_url.clone();
                chunk_jobs.push(async move {
                    loop {
                        let req = client.get(&download_url)?;
                        let req_job = async {
                            println!("Chunk {} has started: {}-{}", i, start_pos, end_pos);
                            let time = Instant::now();
                            let req =
                                req.header("Range", &format!("bytes={}-{}", start_pos, end_pos));
                            let res = req.await?;
                            let chunk_file = smol::fs::OpenOptions::new()
                                .create(true)
                                .truncate(true)
                                .write(true)
                                .open(format!("test.chunk.{}.tmp", i))
                                .await?;
                            smol::io::copy(res, chunk_file).await?;
                            let time = time.elapsed().as_secs_f64();
                            println!("Chunk {} has finished: {}s", i, time);
                            DynResult::Ok(())
                        };
                        match req_job.await {
                            Ok(_) => return DynResult::Ok(()),
                            Err(e) => {
                                println!("Chunk {} failed to fetch, retrying: {}", i, e);
                            }
                        }
                    }
                });
            }
            let r = join_all(chunk_jobs).await;
            let r = r.into_iter().all(|b| dbg!(b).is_ok());
            if r {
                println!("All chunk downloaded successfully, concating into one file");
                let time = Instant::now();
                let mut result = smol::fs::OpenOptions::new()
                    .create(true)
                    .truncate(true)
                    .write(true)
                    .open("test.bin")
                    .await?;
                for i in 0..chunk_amount {
                    let chunk_file = smol::fs::OpenOptions::new()
                        .truncate(true)
                        .read(true)
                        .open(format!("test.chunk.{}.tmp", i))
                        .await?;
                    smol::io::copy(chunk_file, &mut result).await?;
                }
                let time = time.elapsed().as_secs_f64();
                println!("File finished to concat: {}s", time);
            } else {
                println!("Some of chunks failed to download, cleaning chunks");
            }
            // Clean temp chunks
            for i in 0..chunk_amount {
                let _ = smol::fs::remove_file(format!("test.chunk.{}.tmp", i)).await;
            }
            let time = time.elapsed().as_secs_f64();
            println!("File downloaded: {}s", time);
        } else {
            println!("Content Length: unknown");

            let req = client.get(&download_url)?;
            let time = Instant::now();
            let res = req.await?;
            let result_file = smol::fs::OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open("test.bin")
                .await?;
            smol::io::copy(res, result_file).await?;
            let time = time.elapsed().as_secs_f64();
            println!("File downloaded: {}s", time);
        }
        Ok(())
    }
    .block_on()
}
