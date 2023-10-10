use alhc::prelude::*;
use alhc::*;

use pollster::FutureExt;

fn main() -> DynResult {
    let client = get_client_builder().build().unwrap();
    let data = "Hello World!".repeat(256);

    let r = client
        .post("https://httpbin.org/post")?
        .header("user-agent", "alhc/0.2.0")
        .body_string(data)
        .block_on()?
        .recv_string()
        .block_on()?;

    println!("{r}");

    Ok(())
}
