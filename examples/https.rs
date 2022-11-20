use alhc::ClientBuilder;
use futures::AsyncReadExt;
use pollster::FutureExt;

fn main() {
    async {
        let client = ClientBuilder::default().build();
        let mut result = vec![];
        let _ = client
            .get("https://httpbin.org/anything")
            .send()
            .read_to_end(&mut result)
            .await;
        println!("{}", String::from_utf8_lossy(&result));
    }.block_on()
}