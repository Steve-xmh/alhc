use std::error::Error;

use alhc::ClientBuilder;

use pollster::FutureExt;

fn main() -> Result<(), Box<dyn Error>> {
    let client = ClientBuilder::default().build();
    let r = client
        .get("https://httpbin.org/anything")?
        .block_on()?
        .recv_string()
        .block_on()?;
    println!("{}", r);
    Ok::<(), Box<dyn Error>>(())
}
