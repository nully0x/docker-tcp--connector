use chrono::Local;
use env_logger::Builder;
use log::{debug, error, info, LevelFilter};
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

struct ServiceProxy {
    listen_addr: SocketAddr,
    target_addr: SocketAddr,
}

impl ServiceProxy {
    fn new(listen_addr: SocketAddr, target_addr: SocketAddr) -> Self {
        ServiceProxy {
            listen_addr,
            target_addr,
        }
    }

    fn start(&self) -> std::io::Result<()> {
        info!(
            "Starting service proxy: Listening on {}, forwarding to target service at {}",
            self.listen_addr, self.target_addr
        );

        let listener = TcpListener::bind(self.listen_addr)?;

        for stream in listener.incoming() {
            match stream {
                Ok(client_stream) => {
                    info!("New connection from client service");
                    let target_addr = self.target_addr;
                    thread::spawn(move || {
                        if let Err(e) = handle_connection(client_stream, target_addr) {
                            error!("Error handling connection: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Error accepting connection: {}", e);
                }
            }
        }
        Ok(())
    }
}

fn handle_connection(mut client_stream: TcpStream, target_addr: SocketAddr) -> std::io::Result<()> {
    let mut target_stream = match TcpStream::connect_timeout(&target_addr, Duration::from_secs(5)) {
        Ok(stream) => stream,
        Err(e) => {
            error!("Failed to connect to target service: {}", e);
            return Err(e);
        }
    };

    info!("Connected to target service at {}", target_addr);

    let mut client_stream_clone = client_stream.try_clone()?;
    let mut target_stream_clone = target_stream.try_clone()?;

    let handle1 = thread::spawn(move || {
        forward_data(
            &mut client_stream,
            &mut target_stream_clone,
            "Client -> Target",
        )
    });

    let handle2 = thread::spawn(move || {
        forward_data(
            &mut target_stream,
            &mut client_stream_clone,
            "Target -> Client",
        )
    });

    handle1.join().unwrap()?;
    handle2.join().unwrap()?;

    Ok(())
}

fn forward_data(from: &mut TcpStream, to: &mut TcpStream, direction: &str) -> std::io::Result<()> {
    let mut buffer = [0; 8192]; // Increased buffer size for better performance
    loop {
        match from.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                let data = &buffer[..n];
                info!("{}: {} bytes", direction, n);

                // Log all data in hexadecimal format
                debug!("{}: Hex data: {:02X?}", direction, data);

                // Try to display printable ASCII characters
                let ascii_string: String = data
                    .iter()
                    .map(|&c| {
                        if c.is_ascii_graphic() || c.is_ascii_whitespace() {
                            c as char
                        } else {
                            '.'
                        }
                    })
                    .collect();
                debug!("{}: ASCII representation: {}", direction, ascii_string);

                // Basic protocol detection
                if let Ok(s) = std::str::from_utf8(data) {
                    if s.starts_with("HTTP/") || s.starts_with("GET ") || s.starts_with("POST ") {
                        info!("{}: HTTP traffic detected", direction);
                        if let Some(status_line) = s.lines().next() {
                            info!("{}: HTTP Status/Request: {}", direction, status_line);
                        }
                    } else if s.starts_with("Q") {
                        info!("{}: Possible database query detected", direction);
                    }
                    //todo!() // Add more protocol detections as needed
                }

                to.write_all(data)?;
                to.flush()?;
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

fn prompt_for_address(prompt: &str) -> SocketAddr {
    loop {
        println!("{}", prompt);
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
        .filter(None, LevelFilter::Debug)
        .init();
    Ok(())
}

fn main() -> std::io::Result<()> {
    setup_logger().expect("Failed to initialize logger");

    let listen_addr =
        prompt_for_address("Enter the address for this proxy to listen on (e.g., 0.0.0.0:8080):");
    let target_addr =
        prompt_for_address("Enter the address of the target service (e.g., 172.17.0.3:5000):");

    if listen_addr == target_addr {
        error!("The proxy listen address must be different from the target service address.");
        std::process::exit(1);
    }

    let proxy = ServiceProxy::new(listen_addr, target_addr);
    proxy.start()
}
