use chrono::Local;
use env_logger::Builder;
use log::{error, info, LevelFilter};
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::str;
use std::thread;
use std::time::Duration;

struct ContainerBridge {
    container1_addr: SocketAddr,
    container2_addr: SocketAddr,
}

impl ContainerBridge {
    fn new(container1_addr: SocketAddr, container2_addr: SocketAddr) -> Self {
        ContainerBridge {
            container1_addr,
            container2_addr,
        }
    }

    fn start(&self) -> std::io::Result<()> {
        info!(
            "Attempting to connect {} and {}",
            self.container1_addr, self.container2_addr
        );

        loop {
            match (
                TcpStream::connect(self.container1_addr),
                TcpStream::connect(self.container2_addr),
            ) {
                (Ok(stream1), Ok(stream2)) => {
                    info!("Connected to both containers!");
                    self.handle_connection(stream1, stream2)?;
                }
                _ => {
                    error!("Couldn't connect to both containers. Retrying in 5 seconds...");
                    std::thread::sleep(Duration::from_secs(5));
                }
            }
        }
    }

    fn handle_connection(
        &self,
        mut stream1: TcpStream,
        mut stream2: TcpStream,
    ) -> std::io::Result<()> {
        let mut stream1_clone = stream1.try_clone()?;
        let mut stream2_clone = stream2.try_clone()?;

        let handle1 = thread::spawn(move || {
            forward_data(&mut stream1, &mut stream2_clone, "Container1 -> Container2")
        });

        let handle2 = thread::spawn(move || {
            forward_data(&mut stream2, &mut stream1_clone, "Container2 -> Container1")
        });

        handle1.join().unwrap()?;
        handle2.join().unwrap()?;

        Ok(())
    }
}

fn forward_data(from: &mut TcpStream, to: &mut TcpStream, direction: &str) -> std::io::Result<()> {
    let mut buffer = [0; 1024];
    loop {
        match from.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                let data = &buffer[..n];
                info!("{}: {} bytes", direction, n);

                // Try to display the data as UTF-8 string
                match str::from_utf8(data) {
                    Ok(s) => info!("{}: Data: {}", direction, s.trim()),
                    Err(_) => info!("{}: Data: {:?} (non UTF-8)", direction, data),
                }

                to.write_all(data)?;
            }
            Err(e) => {
                error!("{}: Error reading data: {}", direction, e);
                return Err(e);
            }
        }
    }
    info!("Connection from {} closed.", direction);
    Ok(())
}

fn prompt_for_address(service: &str) -> SocketAddr {
    loop {
        println!("Enter the address for {} (e.g., 127.0.0.1:3000):", service);
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .expect("Failed to read line");
        match input.trim().parse() {
            Ok(addr) => return addr,
            Err(_) => println!("Invalid address. Please try again."),
        }
    }
}

fn setup_logger() -> Result<(), io::Error> {
    Builder::new()
        .format(|buf, record| {
            writeln!(
                buf,
                "{} [{}] - {}",
                Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                record.args()
            )
        })
        .filter(None, LevelFilter::Info)
        .init();
    Ok(())
}

fn main() -> std::io::Result<()> {
    setup_logger().expect("Failed to initialize logger");
    let container1_addr = prompt_for_address("Container 1");
    let container2_addr = prompt_for_address("Container 2");
    let bridge = ContainerBridge::new(container1_addr, container2_addr);
    bridge.start()
}
