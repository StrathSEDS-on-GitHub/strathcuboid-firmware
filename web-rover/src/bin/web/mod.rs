extern crate alloc;

use core::cell::Cell;
use core::str::FromStr;

use alloc::string::{String, ToString};
use core::sync::atomic::{AtomicBool, Ordering};
use critical_section::Mutex;
use embassy_executor::Spawner;
use embassy_net::tcp::TcpSocket;
use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_net::{IpEndpoint, Ipv4Address, Runner, Stack, StackResources};
use embassy_time::{Duration, Timer};
use embedded_io_async::Write;
use esp_hal::rng::Rng;
use esp_radio::wifi::{Interface, Interfaces, WifiController};
use log::{info, warn};
use static_cell::StaticCell;

use crate::rover::{emote_rover, move_rover};
use web_rover::{Emote, Movement};

// based on derekmolloy.ie/an-async-wi-fi-web-server-on-the-esp32-c3-with-embassy-and-no_std-rust-9/
static STACK_RESOURCES: StaticCell<StackResources<3>> = StaticCell::new();
pub static SHARED_STOP: AtomicBool = AtomicBool::new(false);

#[embassy_executor::task]
pub(super) async fn start_web_server(
    spawner: embassy_executor::Spawner,
    wifi_interface: Interfaces<'static>,
    wifi_controller: WifiController<'static>,
) {
    let net_config = embassy_net::Config::ipv4_static(embassy_net::StaticConfigV4 {
        address: embassy_net::Ipv4Cidr::new(embassy_net::Ipv4Address::new(192, 168, 4, 1), 24),
        gateway: None,
        dns_servers: Default::default(),
    });

    let rng = Rng::new();
    let seed = (rng.random() as u64) << 32 | (rng.random() as u64);

    let (stack, runner) = embassy_net::new(
        wifi_interface.access_point,
        net_config,
        STACK_RESOURCES.init(StackResources::new()),
        seed,
    );

    spawner.spawn(net_task(runner).unwrap());
    spawner.spawn(wifi_task(wifi_controller).unwrap());
    log::info!("Waiting for access point network link...");

    stack.wait_config_up().await;

    if let Some(config) = stack.config_v4() {
        info!(
            "Access point is available at http://{}",
            config.address.address()
        );
    }

    info!("Starting web server");
    spawner.spawn(dhcp_task(stack).unwrap());
    spawner.spawn(web_task(stack,spawner).unwrap());
}

#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, Interface<'static>>) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn wifi_task(mut controller: WifiController<'static>) {
    let access_point_config = esp_radio::wifi::ap::AccessPointConfig::default()
        .with_ssid("esp32 wifi")
        .with_auth_method(esp_radio::wifi::AuthenticationMethod::Wpa2Personal)
        .with_password("password1234".into());

    let radio_config = esp_radio::wifi::Config::AccessPoint(access_point_config);

    controller.set_config(&radio_config).unwrap();

    info!("Wi-Fi access point started: SSID=esp32 wifi, password=password1234");

    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}

#[embassy_executor::task]
async fn dhcp_task(stack: Stack<'static>) -> ! {
    let mut rx_metadata = [PacketMetadata::EMPTY; 2];
    let mut tx_metadata = [PacketMetadata::EMPTY; 2];
    let mut socket_rx_buffer = [0u8; 1500];
    let mut socket_tx_buffer = [0u8; 1500];
    let mut packet_buffer = [0u8; 1500];
    let mut response_buffer = [0u8; 1500];
    let mut socket = UdpSocket::new(
        stack,
        &mut rx_metadata,
        &mut socket_rx_buffer,
        &mut tx_metadata,
        &mut socket_tx_buffer,
    );
    socket.bind(67).unwrap();

    let server_ip = core::net::Ipv4Addr::new(192, 168, 4, 1);
    let server_options = edge_dhcp::server::ServerOptions::new(server_ip, None);
    let mut server: edge_dhcp::server::Server<_, 8> =
        edge_dhcp::server::Server::new(|| embassy_time::Instant::now().as_secs(), server_ip);
    let broadcast = IpEndpoint::new(Ipv4Address::new(255, 255, 255, 255).into(), 68);

    info!("DHCP server started");

    loop {
        let (len, _) = socket.recv_from(&mut packet_buffer).await.unwrap();
        let request = match edge_dhcp::Packet::decode(&packet_buffer[..len]) {
            Ok(request) => request,
            Err(_) => continue,
        };
        let mut option_buffer = edge_dhcp::Options::buf();

        if let Some(reply) = server.handle_request(&mut option_buffer, &server_options, &request) {
            if let Ok(response) = reply.encode(&mut response_buffer) {
                let _ = socket.send_to(response, broadcast).await;
            }
        }
    }
}

#[embassy_executor::task]
async fn web_task(stack: Stack<'static>, spawner: Spawner) -> ! {
    let mut rx_buffer = [0u8; 1536];
    let mut tx_buffer = [0u8; 1536];

    loop {
        let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
        socket.set_timeout(Some(Duration::from_secs(3)));

        info!("Listening on TCP :80");

        if let Err(e) = socket.accept(80).await {
            warn!("Accept error: {:?}", e);
            continue;
        }
        log::info!("Client connected from {:?}", socket.remote_endpoint());
        let mut buf = [0u8; 1024];
        let mut len = 0;
        loop {
            match socket.read(&mut buf[len..]).await {
                Ok(0) => break,
                Ok(n) => {
                    len += n;
                    if buf[..len].windows(4).any(|w| w == b"\r\n\r\n") || len == buf.len() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let req = core::str::from_utf8(&buf[..len]).unwrap_or("");
        let method = req.split_whitespace().nth(0).unwrap_or("GET");
        let target = req.split_whitespace().nth(1).unwrap_or("/");
        let path = target.split("?").nth(0).unwrap_or("/");

        let response_header: String;
        let response_body: &[u8];
        match method {
            "GET" => {
                info!("GET request for path: {path}");
                match path {
                    "/" => {
                        let web_status =
                            critical_section::with(|cs| SERVER_STATUS.borrow(cs).get());
                        info!("Web status: {web_status:?}");

                        response_body = match web_status {
                            WebStatus::WaitingForSteppers => {
                                include_str!("waiting_for_steppers.html").as_bytes()
                            }
                            WebStatus::InitError(msg) => {
                                info!("Initialization error: {}", msg);
                                include_str!("init_error.html").as_bytes()
                            }
                            WebStatus::SteppersReady => {
                                include_str!("drivers_ready.html").as_bytes()
                            }
                        };
                        response_header = alloc::format!(
                            "HTTP/1.0 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            response_body.len()
                        );
                    }
                    "/style.css" => {
                        response_body = include_str!("style.css").as_bytes();
                        response_header = alloc::format!(
                            "HTTP/1.0 200 OK\r\nContent-Type: text/css\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            response_body.len()
                        );
                    }
                    "/favicon.ico" => {
                        response_body = include_bytes!("favicon.ico");
                        response_header = alloc::format!(
                            "HTTP/1.0 200 OK\r\nContent-Type: image/x-icon\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            response_body.len()
                        );
                    }
                    _ => {
                        warn!("Unknown path: {path}");
                        response_body = "Not Found".as_bytes();
                        response_header = alloc::format!(
                            "HTTP/1.0 404 Not Found\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            response_body.len()
                        );
                    }
                }
            }
            "POST" => {
                info!("POST request for path: {path}");
                if let Ok(emote) = Emote::from_str(path.trim_start_matches("/api/emote/")){
                    info!("Received emote command: {emote:?}");
                    SHARED_STOP.store(true,Ordering::Relaxed);
                    spawner.spawn(emote_rover(emote).unwrap());
                    response_body = "OK".as_bytes();
                    response_header = alloc::format!(
                        "HTTP/1.0 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        response_body.len()
                    );

                } else if let Ok(movement) = Movement::from_str(path.trim_start_matches("/api/")) {
                    info!("Received movement command: {movement:?}");
                    
                    SHARED_STOP.store(false,Ordering::Relaxed);
                    move_rover(movement).await;
                    response_body = "OK".as_bytes();
                    response_header = alloc::format!(
                        "HTTP/1.0 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        response_body.len()
                    );
                } else {
                    warn!("Unknown movement command: {path}");
                    response_body = "Invalid movement command".as_bytes();
                    response_header = alloc::format!(
                        "HTTP/1.0 400 Bad Request\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        response_body.len()
                    );
                }
            }
            _ => {
                warn!("Invalid method: {method}");
                response_body = "Invalid method".as_bytes();
                response_header = alloc::format!(
                    "HTTP/1.0 405 Invalid Method\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    response_body.len()
                );
            }
        }

        if let Err(e) = socket.write_all(response_header.as_bytes()).await {
            log::warn!("Header write error: {:?}", e);
        } else if let Err(e) = socket.write_all(response_body).await {
            log::warn!("Body write error: {:?}", e);
        }

        let _ = socket.flush().await;
        socket.close();
    }
}

#[derive(Clone, Copy, Debug)]
pub enum WebStatus {
    WaitingForSteppers,
    SteppersReady,
    InitError(&'static str),
}

static SERVER_STATUS: Mutex<Cell<WebStatus>> = Mutex::new(Cell::new(WebStatus::WaitingForSteppers));

pub fn set_server_status(status: WebStatus) {
    critical_section::with(|cs| {
        SERVER_STATUS.borrow(cs).set(status);
    });
}
