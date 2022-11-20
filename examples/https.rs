use std::error::Error;

use alhc::ClientBuilder;
use futures::AsyncReadExt;
use pollster::FutureExt;

fn main() {
    async {
        let client = ClientBuilder::default().build();
        let mut result = vec![];
        let _ = client
            .post("https://httpbin.org/anything")
            .header("X-Requested-By", "Yes")
            .body_string("Hello World!".repeat(10))
            .await?
            .read_to_end(&mut result)
            .await;
        println!("{}", String::from_utf8_lossy(&result));
        println!("Sending second");
        let _ = client
            .post("https://httpbin.org/anything")
            .header("X-Requested-By", "No")
            .body_string("Hello ALHC!".repeat(10))
            .await?
            .read_to_end(&mut result)
            .await;
        println!("{}", String::from_utf8_lossy(&result));
        Ok::<(), Box<dyn Error>>(())
    }
    .block_on()
    .unwrap();
}
