extern crate alloc;

use embassy_net::tcp::TcpSocket;
use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_net::{IpEndpoint, Ipv4Address, Runner, Stack};
use embassy_time::{Duration, Timer};
use esp_radio::wifi::{Interface, WifiController};
use log::{info, warn};

// based on derekmolloy.ie/an-async-wi-fi-web-server-on-the-esp32-c3-with-embassy-and-no_std-rust-9/

#[embassy_executor::task]
pub async fn net_task(mut runner: Runner<'static, Interface<'static>>) -> ! {
    runner.run().await
}

#[embassy_executor::task]
pub async fn wifi_task(mut controller: WifiController<'static>) {
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
pub async fn dhcp_task(stack: Stack<'static>) -> ! {
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
pub async fn web_task(stack: Stack<'static>) -> ! {
    let mut rx_buffer = [0u8; 1536];
    let mut tx_buffer = [0u8; 1536];

    loop {
        let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
        socket.set_timeout(Some(Duration::from_secs(3)));

        info!("Listening on TCP :80");

        if let Err(e) = socket.accept(80).await {
            warn!("Accept error: {:?}", e);
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
        info!("Received request: {}", req);
        let method = req.split_whitespace().nth(0).unwrap_or("GET");
        let target = req.split_whitespace().nth(1).unwrap_or("/");
        let path = target.split("?").nth(0).unwrap_or("/");
        match method {
            "GET" => {
                info!("GET request for target: {}, path: {}", target, path);
                match path {
                    "/left" => info!("Left"),
                    "/right" => info!("Right"),
                    "/forward" => info!("Forward"),
                    "/back" => info!("Back"),
                    _ => info!("Unknown path: {}", path),
                }

                let body = core::str::from_utf8(include_bytes!("index.html")).unwrap();
                let header = alloc::format!(
                    "HTTP/1.0 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );

                if let Err(e) = socket.write(header.as_bytes()).await {
                    log::warn!("Write error: {:?}", e);
                } else if let Err(e) = socket.write(body.as_bytes()).await {
                    log::warn!("Write error: {:?}", e);
                }
            }
            _ => info!("Unknown method: {}", method),
        }

        let _ = socket.flush().await;
        socket.close();
    }
}
