use std::net::TcpListener;

use cpu::Cpu;
use self::gdb::GdbRemote;

mod gdb;

pub struct Debugger {
    /// Listener waiting for remote connections
    listener: TcpListener,
    /// Holds the current client connection
    client: Option<GdbRemote>,
    /// Internal state: set to true when the remote requests that the
    /// execution should resume
    resume: bool,
    /// If a single step is requested this flag is set
    step: bool,
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
            resume: true,
            step: false,
        }
    }

    pub fn debug(&mut self, cpu: &mut Cpu) {
        let mut client =
            match self.client.take() {
                Some(mut c) => {
                    // Notify the remote that we're halted and waiting
                    // for instructions. I ignore errors here for
                    // simplicity, if the connection hung up for some
                    // reason we'll figure it out soon enough.
                    let _ = c.send_status();
                    c
                }
                None => GdbRemote::new(&self.listener),
            };

        // We loop as long as the remote debugger doesn't tell us to
        // continue
        self.resume = false;

        while !self.resume {
            // Inner debugger loop: handle client requests until it
            // requests that the execution resumes or an error is
            // encountered
            if let Err(_) = client.serve(self, cpu) {
                // We encountered an error with the remote client: we
                // wait for a new connection
                client = GdbRemote::new(&self.listener);
            }
        }

        // Before we resume execution we store the current client
        self.client = Some(client);
    }

    pub fn resume(&mut self) {
        self.resume = true;
    }

    pub fn set_step(&mut self) {
        self.step = true;
    }

    /// Called by the CPU when it's about to execute a new
    /// instruction. This function is called before *all* CPU
    /// instructions so it needs to be as fast as possible.
    pub fn pc_change(&mut self, cpu: &mut Cpu) {
        if self.step {
            // Stepping was requested, we clear the flag and enter
            // debugging mode
            self.step = false;
            self.debug(cpu);
        }
    }
}
