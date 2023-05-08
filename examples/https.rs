use alhc::prelude::*;
use alhc::*;

use pollster::FutureExt;

fn main() -> DynResult {
    let client = get_client_builder().build().unwrap();

    let r = client
        .post("https://httpbin.org/anything")?
        .header("user-agent", "alhc/0.2.0")
        .body_string("Hello World!".repeat(20))
        .block_on()?
        .recv_string()
        .block_on()?;

    println!("{r}");

    Ok(())
}
