use tokio::sync::mpsc;
use tokio::sync::oneshot;
use bytes::Bytes;
use mini_redis::client;
use Command::*;


#[tokio::main]
async fn main() {
    /*
        Creates a bounded mpsc channel for communicating between asynchronous tasks with backpressure.
        The channel will buffer up to the provided number of messages. Once the buffer is full,
        attempts to send new messages will wait until a message is received from the channel.
        All data sent on Sender will become available on Receiver in the same order as it was sent.

        The mpsc channel is used to send commands to the task managing the redis connection.
        The multi-producer capability allows messages to be sent from many tasks. Creating the
        channel returns two values, a sender and a receiver. The two handles are used separately.
        They may be moved to different tasks. The channel is created with a capacity of 32.
        If messages are sent faster than they are received, the channel will store them.
        Once the 32 messages are stored in the channel, calling send(...).await will go to sleep
        until a message has been removed by the receiver. Sending from multiple tasks is done by
        cloning the Sender. It is not possible to clone the Receiver of an mpsc channel.
     */
    let (tx, mut rx) = mpsc::channel(32);

    // The `move` keyword is used to **move** ownership of `rx` into the task.
    let manager = tokio::spawn(async move {
        // Establish a connection to the server
        let mut client = client::connect("127.0.0.1:6379").await.unwrap();

        // Start receiving messages
        while let Some(cmd) = rx.recv().await {
            match cmd {
                Get { key, resp } => {
                    let res = client.get(&key).await;
                    // Ignore errors
                    let _ = resp.send(res);
                }
                Set { key, val, resp } => {
                    let res = client.set(&key, val).await;
                    // Ignore errors
                    let _ = resp.send(res);
                }
            }
        }
    });

    let tx2 = tx.clone();

    // Spawn two tasks, one gets a key, the other sets a key
    let t1 = tokio::spawn(async move {
        let (resp_tx, resp_rx) = oneshot::channel();
        let cmd = Get {
            key: "foo".to_string(),
            resp: resp_tx,
        };

        // Send the GET request
        tx.send(cmd).await.unwrap();

        // Await the response
        let res = resp_rx.await;
        println!("GOT = {:?}", res);
    });

    let t2 = tokio::spawn(async move {
        let (resp_tx, resp_rx) = oneshot::channel();
        let cmd = Set {
            key: "foo".to_string(),
            val: "bar".into(),
            resp: resp_tx,
        };

        // Send the SET request
        tx2.send(cmd).await.unwrap();

        // Await the response
        let res = resp_rx.await;
        println!("GOT = {:?}", res);
    });

    t1.await.unwrap();
    t2.await.unwrap();
    manager.await.unwrap();
}

/*
    Provided by the requester and used by the manager task to send the command response back to the
    requester. The final step is to receive the response back from the manager task. The GET command
    needs to get the value and the SET command needs to know if the operation completed successfully.
    To pass the response, a oneshot channel is used. The oneshot channel is a single-producer,
    single-consumer channel optimized for sending a single value. In our case, the single value is
    the response. Unlike mpsc, no capacity is specified as the capacity is always one. Additionally,
    neither handle can be cloned. To receive responses from the manager task, before sending a
    command, a oneshot channel is created. The Sender half of the channel is included in the command
    to the manager task. The receive half is used to receive the response.
*/
type Responder<T> = oneshot::Sender<mini_redis::Result<T>>;

#[derive(Debug)]
enum Command {
    Get {
        key: String,
        resp: Responder<Option<Bytes>>,
    },
    Set {
        key: String,
        val: Bytes,
        resp: Responder<()>,
    },
}