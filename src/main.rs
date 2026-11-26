mod tempcli;

#[tokio::main]
async fn main() {
    let code = match tempcli::run().await {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    };
    // Exit right away: tokio's blocking stdin reader would keep the process alive until Enter.
    std::process::exit(code);
}
