use std::error::Error;

use alhc::*;

use pollster::FutureExt;

fn main() -> Result {
    let client = ClientBuilder::default().build();
    let r = client
        .post("https://httpbin.org/anything")?
        .body_string("Hello World!".repeat(20))
        .block_on()?
        .recv_string()
        .block_on()?;
    println!("{}", r);
    Ok::<(), Box<dyn Error>>(())
}
