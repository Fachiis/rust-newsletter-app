use std::net::TcpListener;
use zero2prod::run;

// Entry point of the application
#[tokio::main]
async fn main() -> std::io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8000").expect("Failed to bind to port 8000");
    let port = listener.local_addr()?.port();
    println!("{}", format_args!("Server started at http://127.0.0.1:{}", port));
    run(listener)?.await
}
