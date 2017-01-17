extern crate tokio_workq as workq;

#[macro_use]
extern crate futures;
extern crate tokio_core;
extern crate tokio_proto;
extern crate tokio_service;

use futures::Future;

use tokio_core::reactor::Core;
use tokio_service::Service;

pub fn main() {
    let mut core = Core::new().unwrap();
    let addr = "127.0.0.1:9922".parse().unwrap();
    let handle = core.handle();
    core.run(
        workq::Client::connect(&addr, &handle)
            .and_then(|client| {
                // Start with a ping
                client.lease(vec!["ping1".to_string()], None)
                    .and_then(move |_| {
                        println!("Pong received...");
                        client.call("Goodbye".to_string())
                    })
                    .and_then(|response| {
                        println!("CLIENT: {:?}", response);
                        Ok(())
                    })
            })
    ).unwrap();
}
