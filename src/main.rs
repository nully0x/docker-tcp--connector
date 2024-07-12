use chrono::Local;
use env_logger::Builder;
use log::{debug, error, info, LevelFilter};
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

struct DatabaseProxy {
    listen_addr: SocketAddr,
    db_addr: SocketAddr,
}

impl DatabaseProxy {
    fn new(listen_addr: SocketAddr, db_addr: SocketAddr) -> Self {
        DatabaseProxy {
            listen_addr,
            db_addr,
        }
    }

    fn start(&self) -> std::io::Result<()> {
        info!(
            "Starting database proxy: Listening on {}, forwarding to DB at {}",
            self.listen_addr, self.db_addr
        );

        let listener = TcpListener::bind(self.listen_addr)?;

        for stream in listener.incoming() {
            match stream {
                Ok(api_stream) => {
                    info!("New connection from API container");
                    let db_addr = self.db_addr;
                    thread::spawn(move || {
                        if let Err(e) = handle_connection(api_stream, db_addr) {
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

fn handle_connection(mut api_stream: TcpStream, db_addr: SocketAddr) -> std::io::Result<()> {
    let mut db_stream = match TcpStream::connect_timeout(&db_addr, Duration::from_secs(5)) {
        Ok(stream) => stream,
        Err(e) => {
            error!("Failed to connect to DB: {}", e);
            return Err(e);
        }
    };

    info!("Connected to database at {}", db_addr);

    let mut api_stream_clone = api_stream.try_clone()?;
    let mut db_stream_clone = db_stream.try_clone()?;

    let handle1 =
        thread::spawn(move || forward_data(&mut api_stream, &mut db_stream_clone, "API -> DB"));

    let handle2 =
        thread::spawn(move || forward_data(&mut db_stream, &mut api_stream_clone, "DB -> API"));

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

                // Check for specific patterns
                if data.starts_with(b"Q") {
                    info!("{}: Possible PostgreSQL query detected", direction);
                } else if data.starts_with(b"HTTP/1.1")
                    || data.starts_with(b"GET")
                    || data.starts_with(b"POST")
                {
                    info!("{}: HTTP traffic detected", direction);
                    if let Ok(s) = std::str::from_utf8(data) {
                        if let Some(status_line) = s.lines().next() {
                            info!("{}: HTTP Status/Request: {}", direction, status_line);
                        }
                    }
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

    let listen_addr = prompt_for_address("Enter the address for this proxy to listen on (should be different from the actual DB port, e.g., 0.0.0.0:15432):");
    let db_addr = prompt_for_address(
        "Enter the actual address of the DB container (e.g., 0.0.0.0:5432 or 172.17.0.3:5432):",
    );

    if listen_addr == db_addr {
        error!("The proxy listen address must be different from the actual DB address.");
        std::process::exit(1);
    }

    let proxy = DatabaseProxy::new(listen_addr, db_addr);
    proxy.start()
}
