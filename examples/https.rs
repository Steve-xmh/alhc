use alhc::prelude::*;
use pollster::FutureExt as _;

fn main() -> alhc::DynResult {
    let client = get_client_builder().build()?;
    let data = "Hello World!".repeat(256);

    let body = client
        .post("https://httpbin.org/post")?
        .header("user-agent", "alhc/0.2.0")
        .body_string(data)
        .block_on()?
        .recv_string()
        .block_on()?;

    println!("{body}");
    Ok(())
}
