extern crate futures;
extern crate tokio_core;
extern crate tokio_proto;
extern crate tokio_service;

use std::io;
use std::net::SocketAddr;
use futures::{Future, future};
use tokio_core::io::{Io, Codec, Framed, EasyBuf};
use tokio_core::net::TcpStream;
use tokio_core::reactor::Handle;
use tokio_proto::TcpClient;
use tokio_proto::pipeline::{ClientProto, ClientService};
use tokio_service::{Service, NewService};

pub struct Client {
    inner: Validate<ClientService<TcpStream, WorkqProto>>,
}

pub struct Validate<T> {
    inner: T,
}

pub struct WorkqCodec;

struct WorkqProto;

impl Client {
    /// Establish a connection to a line-based server at the provided `addr`.
    pub fn connect(addr: &SocketAddr, handle: &Handle) -> Box<Future<Item = Client, Error = io::Error>> {
        let ret = TcpClient::new(WorkqProto)
            .connect(addr, handle)
            .map(|client_service| {
                let validate = Validate { inner: client_service};
                Client { inner: validate }
            });

        Box::new(ret)
    }

    pub fn lease(&self, names: Vec<String>, timeout: Option<usize>) -> Box<Future<Item = (), Error = io::Error>> {
        let timeout = match timeout {
            Some(v) => v,
            None => 10000,
        };
        let names_str = names.join(" ");
        let resp = self.call(format!("lease {} {}", names_str, timeout).to_string())
            .and_then(|resp| {
                if resp != "[lease]" {
                    Err(io::Error::new(io::ErrorKind::Other, "expected lease command"))
                } else {
                    Ok(())
                }
            });
        Box::new(resp)
    }
}

impl Service for Client {
    type Request = String;
    type Response = String;
    type Error = io::Error;
    // For simplicity, box the future.
    type Future = Box<Future<Item = String, Error = io::Error>>;

    fn call(&self, req: String) -> Self::Future {
        self.inner.call(req)
    }
}

impl<T> Validate<T> {

    /// Create a new `Validate`
    pub fn new(inner: T) -> Validate<T> {
        Validate { inner: inner }
    }
}


impl<T> Service for Validate<T>
    where T: Service<Request = String, Response = String, Error = io::Error>,
          T::Future: 'static,
{
    type Request = String;
    type Response = String;
    type Error = io::Error;
    // For simplicity, box the future.
    type Future = Box<Future<Item = String, Error = io::Error>>;

    fn call(&self, req: String) -> Self::Future {
        // Make sure that the request does not include any new lines
        if req.chars().find(|&c| c == '\n').is_some() {
            let err = io::Error::new(io::ErrorKind::InvalidInput, "message contained new line");
            return Box::new(future::done(Err(err)))
        }

        // Call the upstream service and validate the response
        Box::new(self.inner.call(req)
            .and_then(|resp| {
                if resp.chars().find(|&c| c == '\n').is_some() {
                    Err(io::Error::new(io::ErrorKind::InvalidInput, "message contained new line"))
                } else {
                    Ok(resp)
                }
            }))
    }
}

impl<T> NewService for Validate<T>
    where T: NewService<Request = String, Response = String, Error = io::Error>,
          <T::Instance as Service>::Future: 'static
{
    type Request = String;
    type Response = String;
    type Error = io::Error;
    type Instance = Validate<T::Instance>;

    fn new_service(&self) -> io::Result<Self::Instance> {
        let inner = try!(self.inner.new_service());
        Ok(Validate { inner: inner })
    }
}

impl Codec for WorkqCodec {
    type In = String;
    type Out = String;

    fn decode(&mut self, buf: &mut EasyBuf) -> Result<Option<String>, io::Error> {
        // Check to see if the frame contains a new line
        if let Some(n) = buf.as_ref().iter().position(|b| *b == b'\n') {
            // remove the serialized frame from the buffer.
            let line = buf.drain_to(n);

            // Also remove the '\n'
            buf.drain_to(1);

            // Turn this data into a UTF string and return it in a Frame.
            return match std::str::from_utf8(&line.as_ref()) {
                Ok(s) => Ok(Some(s.to_string())),
                Err(_) => Err(io::Error::new(io::ErrorKind::Other, "invalid string")),
            }
        }

        Ok(None)
    }

    fn encode(&mut self, msg: String, buf: &mut Vec<u8>) -> io::Result<()> {
        buf.extend_from_slice(msg.as_bytes());
        buf.push(b'\n');

        Ok(())
    }
}

impl<T: Io + 'static> ClientProto<T> for WorkqProto {
    type Request = String;
    type Response = String;

    type Transport = Framed<T, WorkqCodec>;
    type BindTransport = Result<Self::Transport, io::Error>;

    fn bind_transport(&self, io: T) -> Self::BindTransport {
        Ok(io.framed(WorkqCodec))
    }
}
