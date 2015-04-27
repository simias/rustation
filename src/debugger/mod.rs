use std::net::TcpListener;

use cpu::Cpu;
use self::gdb::GdbRemote;

mod gdb;

pub struct Debugger {
    listener: TcpListener,
    client: Option<GdbRemote>,
}

impl Debugger {
    pub fn new() -> Debugger {
        let bind_to = "127.0.0.1:9001";

        // XXX The bind address/port should be configurable
        let listener =
            match TcpListener::bind(bind_to) {
                Ok(l)  => l,
                Err(e) => panic!("Couldn't bind GDB server TCP socket: {}", e),
            };

        println!("Waiting for debugger on {}", bind_to);

        Debugger {
            listener: listener,
            client: None,
        }
    }

    pub fn debug(&mut self, cpu: &mut Cpu) {
        // We loop as long as the remote debugger doesn't tell us to
        // continue
        loop {
            let mut client =
                match self.client.take() {
                    // XXX send update
                    Some(c) => c,
                    None => GdbRemote::new(&self.listener),
                };

            match client.serve(self, cpu) {
                // We're done, we can store the client and resume
                // normal execution
                Ok(_) => {
                    self.client = Some(client);
                    return;
                }
                // We encountered an error with the remote client: we
                // loop
                Err(_) => println!("Lost remote debugger connection"),
            }
        }
    }
}
