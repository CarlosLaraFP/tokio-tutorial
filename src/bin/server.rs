use tokio::net::{TcpListener, TcpStream};
use mini_redis::{Connection, Frame};
use bytes::Bytes;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
/*
    Arc is a thread-safe reference-counting pointer. 'Arc' stands for 'Atomically Reference Counted'.
    The type Arc<T> provides shared ownership of a value of type T, allocated in the heap.
    The HashMap will be shared across many tasks and potentially many threads.
    To support this, it is wrapped in Arc<Mutex<_>>

    Arc allows data to be safely shared across multiple threads by ensuring that the data it points
    to is not deallocated until all references to it are out of scope. Arc accomplishes this with
    atomic reference counting, which means that it can safely increment and decrement the reference
    count even in a multithreaded context. Mutex provides mutual exclusion, meaning that at any
    given time, only one thread can modify the data inside the Mutex. This ensures that when
    multiple threads are attempting to access the same data, they won't cause data races. However,
    Mutex itself does not have the ability to be shared across multiple threads. It cannot be
    safely sent across threads or tasks without being wrapped in an Arc.

    As a rule of thumb, using a synchronous mutex from within asynchronous code is fine as long as
    contention remains low and the lock is not held across calls to .await. Additionally, consider
    using parking_lot::Mutex as a faster alternative to std::sync::Mutex
 */
type Db = Arc<Mutex<HashMap<String, Bytes>>>;
/*
    Using a blocking mutex to guard short critical sections is an acceptable strategy when
    contention is minimal. When a lock is contended, the thread executing the task must block
    and wait on the mutex. This will not only block the current task but it will also block all
    other tasks scheduled on the current thread. In our case, as each key is independent,
    mutex sharding solves the issue. To do this, instead of having a single Mutex<HashMap<_, _>>
    instance, we introduce N distinct instances. The dashmap crate provides an implementation
    of a more sophisticated sharded hash map.
 */
type ShardedDb = Arc<Vec<Mutex<HashMap<String, Vec<u8>>>>>;

fn new_sharded_db(num_shards: usize) -> ShardedDb {
    let mut db = Vec::with_capacity(num_shards);
    for _ in 0..num_shards {
        db.push(Mutex::new(HashMap::new()));
    }
    Arc::new(db)
}


#[tokio::main]
async fn main() {
    /*
        The first thing our Redis server needs to do is to accept inbound TCP sockets. This is done
        by binding tokio::net::TcpListener to port 6379 (bind the listener to the address).
     */
    let listener = TcpListener::bind("127.0.0.1:6379").await.unwrap();

    println!("Listening on 127.0.0.1:6379");

    /*
        Using Arc allows the HashMap to be referenced concurrently from many tasks, potentially
        running on many threads. Mutex guards the HashMap. Throughout Tokio, the term handle is
        used to reference a value that provides access to some shared state.
     */
    let db = Arc::new(Mutex::new(HashMap::new()));

    /*
        Then the sockets are accepted in a loop. Each socket is processed then closed.
        For now, we will read the command, print it to stdout and respond with an error.

        The accept() function accepts a new incoming connection from this listener and will yield
        once a new TCP connection is established. When established, the corresponding TcpStream
        (a TCP stream between a local and a remote socket) and the remote peer's address will be
        returned (IP and port of the new connection).

        To process connections concurrently, a new task is spawned for each inbound connection.
        The connection is processed on this task. A new task is spawned for each inbound socket.
        The socket is moved to the new task and processed there (TcpStream ownership is transferred
        from main() to a Tokio task, which is an asynchronous green thread).

        Tasks are the unit of execution managed by the scheduler. Spawning the task submits it to
        the Tokio scheduler, which then ensures that the task executes when it has work to do.
        The spawned task may be executed on the same thread as where it was spawned, or it may
        execute on a different runtime thread. The task can also be moved between threads after
        being spawned. Tasks in Tokio are very lightweight. Under the hood, they require only a
        single allocation and 64 bytes of memory. Applications should feel free to spawn thousands,
        if not millions of tasks.

        The real power of asynchronous programming comes into play when your tasks are I/O-bound
        rather than CPU-bound. In other words, if your tasks spend most of their time waiting for
        I/O operations (like network or disk I/O) to complete, then while one task is waiting,
        other tasks can use the CPU. This allows you to have a high degree of concurrency -- a large
        number of tasks making progress -- even though you only have a limited number of CPU cores.
     */
    loop {
        let (socket, _) = listener.accept().await.unwrap();

        // Clone the handle to the hash map.
        let db = db.clone();

        println!("Connection accepted");

        // When variables are referenced inside an async block, they're borrowed. Therefore,
        // connections must be moved, otherwise they don't live long enough (loop end = context end).
        tokio::spawn(async move {
            process(socket, db).await;
        });

        //println!("{:?}", socket); // value borrowed after move compilation error
    }
}

/*
    Write a single Frame value to the underlying stream. The Frame value is written to the
    socket using the various write_* functions provided by AsyncWrite. Calling these
    functions directly on a TcpStream is not advised, as this will result in a large number
    of syscalls. However, it is fine to call these functions on a buffered write stream.
    The data will be written to the buffer (block of memory). Once the buffer is full,
    it is flushed to the underlying socket.
 */
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
async fn process(socket: TcpStream, db: Db) {
    /*
        This function takes the shared handle to the HashMap as an argument.
        It also needs to lock the HashMap before using it.
        The value's type for the HashMap is now Bytes (which we can cheaply clone).
     */
    use mini_redis::Command::{self, Get, Set};

    // Connection, provided by `mini-redis`, handles parsing frames from the socket
    let mut connection = Connection::new(socket);

    // Use `read_frame` to receive a command from the connection.
    while let Some(frame) = connection.read_frame().await.unwrap() {
        let response = match Command::from_frame(frame).unwrap() {
            Set(cmd) => {
                let mut db = db.lock().unwrap();
                db.insert(cmd.key().to_string(), cmd.value().clone());
                Frame::Simple("OK".to_string())
            }
            Get(cmd) => {
                let db = db.lock().unwrap();
                // `Frame::Bulk` expects data to be of type `Bytes`
                if let Some(value) = db.get(cmd.key()) {
                    Frame::Bulk(value.clone())
                } else {
                    Frame::Null
                }
            }
            cmd => panic!("unimplemented {:?}", cmd),
        };

        // the Mutex doesn't need to be unlocked/released manually due to Rust scopes

        // Write the response to the client
        connection.write_frame(&response).await.unwrap();
    }
}