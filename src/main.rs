use tokio::net::{TcpListener, TcpStream};
use mini_redis::{Connection, Frame};

#[tokio::main]
async fn main() {
    /*
        The first thing our Redis server needs to do is to accept inbound TCP sockets. This is done
        by binding tokio::net::TcpListener to port 6379 (bind the listener to the address).
     */
    let listener = TcpListener::bind("127.0.0.1:6379").await.unwrap();

    /*
        Then the sockets are accepted in a loop. Each socket is processed then closed.
        For now, we will read the command, print it to stdout and respond with an error.

        The accept() function accepts a new incoming connection from this listener and will yield
        once a new TCP connection is established. When established, the corresponding TcpStream
        (a TCP stream between a local and a remote socket) and the remote peer's address will be
        returned (IP and port of the new connection).
     */
    loop {
        let (socket, _) = listener.accept().await.unwrap();
        process(socket).await;
    }
}

async fn process(socket: TcpStream) {
    // The `Connection` lets us read/write redis **frames** instead of
    // byte streams. The `Connection` type is defined by mini-redis.
    let mut connection = Connection::new(socket);

    /*
        Read a single Frame value from the underlying stream. The function waits until it has
        retrieved enough data to parse a frame. Any data remaining in the read buffer after the
        frame has been parsed is kept there for the next call to read_frame.
     */
    if let Some(frame) = connection.read_frame().await.unwrap() {
        println!("GOT: {:?}", frame);

        // A frame in the Redis protocol.
        // Respond with an error
        let response = Frame::Error("unimplemented".to_string());

        /*
            Write a single Frame value to the underlying stream. The Frame value is written to the
            socket using the various write_* functions provided by AsyncWrite. Calling these
            functions directly on a TcpStream is not advised, as this will result in a large number
            of syscalls. However, it is fine to call these functions on a buffered write stream.
            The data will be written to the buffer (block of memory). Once the buffer is full,
            it is flushed to the underlying socket.
         */
        connection.write_frame(&response).await.unwrap();
        /*
            In the context of TCP connections, a buffer is a block of memory used for temporarily
            holding data while it's being moved from one place to another. It serves as a kind of
            intermediary storage. In network programming, buffers are commonly used when transmitting
            data over a network, such as in a TCP connection. When a server is returning a response
            to a client, the response data is first written to a buffer. The size of this buffer can
            vary, but the idea is that it holds the response data temporarily before it is sent over
            the network. When we say, "Once the buffer is full, it is flushed to the underlying socket,"
            it means that when the buffer is filled up with response data, this data is sent from the
            buffer through the socket to the client. This operation is known as "flushing" the buffer.
            Why use a buffer? Sending data across a network can be an expensive operation in terms
            of system resources and time. Writing data to a local buffer is generally faster. By
            accumulating data in a buffer and sending it all at once, we can make more efficient
            use of the network connection.

            In the context of flushing a buffer to a socket, the data is copied from the buffer to
            the network stack. The original data in the buffer remains and can be modified or deleted
            without affecting the copied data that's being sent over the network.
            Flushing a buffer to a socket does involve a copy operation. After the data has been
            copied to the kernel and is safely enqueued for transmission, the application is free to
            reuse or deallocate the buffer memory. The specific timing of when this happens can
            depend on the details of the application and the networking library or framework being used.
         */
    }
}