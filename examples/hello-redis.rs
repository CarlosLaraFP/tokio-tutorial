use mini_redis::{client, Result};

/*
    "Tokio is exactly like ZIO in that regard (fibers): giving you virtual threads atop OS threads."
    - John De Goes

    #[tokio::main] is a macro. It transforms the async fn main() into a synchronous fn main()
    that initializes a runtime instance and executes the async main function.
 */
#[tokio::main]
async fn main() -> Result<()> {
    /*
        Open a connection to the mini-redis address.

        HTTP, the protocol that RESTful APIs use, operates on top of TCP.
        When you make a RESTful request (GET, POST, PUT, DELETE, etc.), the HTTP message
        for that request is sent over a TCP connection. In the case of HTTPS, which is used by AWS,
        the TCP connection is secured using TLS (Transport Layer Security) or its predecessor (SSL).
     */
    let mut client = client::connect("127.0.0.1:6379").await?;

    // Set the key "hello" with value "world"
    client.set("hello", "world".into()).await?;

    // Get key "hello"
    match client.get("hello").await? {
        Some(result) => println!("got value from the server; result = {:?}", result),
        _ => println!("something went wrong")
    }

    Ok(())

    // Due to how Rust scopes work, we do not need to manually close the TCP connection.
}