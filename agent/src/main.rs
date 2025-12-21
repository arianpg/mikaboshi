use clap::Parser;
use pcap::{Capture, Device};
use std::collections::HashSet;
use std::net::IpAddr;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use tonic::transport::Channel;

pub mod packet {
    tonic::include_proto!("packet");
}

use packet::agent_service_client::AgentServiceClient;
use packet::Packet;

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, env = "MIKABOSHI_AGENT_SERVER", default_value = "localhost:50051")]
    server: String,

    #[arg(long, env = "MIKABOSHI_AGENT_DEVICE", default_value = "any")]
    device: String,

    #[arg(long, env = "MIKABOSHI_AGENT_SNAPSHOT", default_value_t = 1024)]
    snapshot: i32,

    #[arg(long, env = "MIKABOSHI_AGENT_PROMISCUOUS", default_value_t = false)]
    promiscuous: bool,

    #[arg(long, env = "MIKABOSHI_AGENT_MOCK", default_value_t = false)]
    mock: bool,

    #[arg(long, env = "MIKABOSHI_AGENT_IPV6", default_value_t = false)]
    ipv6: bool,

    #[arg(long, default_value_t = false)]
    list_devices: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let server_url = if args.server.starts_with("http") {
        args.server.clone()
    } else {
        format!("http://{}", args.server)
    };

    let server_port = extract_port(&args.server).unwrap_or(50051);

    if args.list_devices {
        match Device::list() {
            Ok(devices) => {
                println!("Available devices:");
                for device in devices {
                    println!("  Name: {}", device.name);
                    println!("  Description: {:?}", device.desc);
                    for address in device.addresses {
                        println!("    Address: {:?}", address.addr);
                    }
                    println!();
                }
            }
            Err(e) => eprintln!("Failed to list devices: {}", e),
        }
        return Ok(());
    }

    loop {
        println!("Connecting to {}", server_url);
        
        match run_agent(&server_url, &args, server_port).await {
            Ok(_) => {
                println!("Agent stopped normally.");
                break;
            },
            Err(e) => {
                eprintln!("Agent disconnected or failed: {}", e);
                println!("Reconnecting in 5 seconds...");
                sleep(Duration::from_secs(5)).await;
            }
        }
    }

    Ok(())
}

fn extract_port(addr: &str) -> Option<u16> {
    // Remove protocol if present
    let clean_addr = addr.trim_start_matches("http://").trim_start_matches("https://");
    
    // Find last colon
    if let Some(idx) = clean_addr.rfind(':') {
        if let Ok(port) = clean_addr[idx+1..].parse::<u16>() {
            return Some(port);
        }
    }
    None
}

async fn run_agent(server_url: &str, args: &Args, server_port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let client = AgentServiceClient::connect(server_url.to_string()).await?;
    println!("Connected to server");

    // Create a channel for streaming packets
    let (tx, rx) = mpsc::channel(1024); // Increased buffer size

    // create a stream of batches
    use tokio_stream::StreamExt;
    let request_stream = tokio_stream::wrappers::ReceiverStream::new(rx)
        .chunks_timeout(100, Duration::from_millis(100))
        .map(|packets| compress_packets(packets));

    // Spawn the gRPC client stream handler
    let mut client_clone = client.clone();
    let stream_handle = tokio::spawn(async move {
        match client_clone.stream_packets(request_stream).await {
            Ok(response) => println!("Stream completed: {:?}", response),
            Err(e) => eprintln!("Stream error: {}", e),
        }
    });

    if args.mock {
        println!("Starting in MOCK mode");
        generate_mock_traffic(tx).await;
    } else {
        println!("Starting in LIVE capture mode on device {}", args.device);
        let tx_clone = tx.clone();
        let args_clone = args.clone();
        
        // pcap capture blocks
        let result = tokio::task::spawn_blocking(move || {
            run_live_capture(args_clone, tx_clone, server_port)
        }).await?;

        if let Err(e) = result {
             eprintln!("Error opening device {}: {}", args.device, e);
             eprintln!("Falling back to MOCK mode due to error.");
             generate_mock_traffic(tx).await;
        }
    }
    
    // Wait for stream to finish (which means disconnected)
    let _ = stream_handle.await;

    // If we are here, it means connection lost or done
    Err("Connection lost".into())
}

fn compress_packets(packets: Vec<Packet>) -> packet::PacketBatch {
    use std::collections::HashMap;

    // Key: (src_ip, dst_ip, src_is_agent, dst_is_agent, proto, src_port, dst_port)
    type PacketKey = (Vec<u8>, Vec<u8>, bool, bool, i32, i32, i32);
    
    let mut map: HashMap<PacketKey, Packet> = HashMap::new();

    for p in packets {
        let key = (
            p.src_ip.clone(), 
            p.dst_ip.clone(), 
            p.src_is_agent, 
            p.dst_is_agent, 
            p.proto, 
            p.src_port, 
            p.dst_port
        );

        map.entry(key)
           .and_modify(|existing| existing.size += p.size)
           .or_insert(p);
    }

    packet::PacketBatch {
        packets: map.into_values().collect()
    }
}

fn run_live_capture(args: Args, tx: mpsc::Sender<Packet>, server_port: u16) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut cap = Capture::from_device(args.device.as_str())?
        .promisc(args.promiscuous)
        .snaplen(args.snapshot)
        .timeout(1000) // 1s timeout to allow checking for exit or other things
        .open()?;

    // Set BPF filter
    let filter = format!("not port {}", server_port);
    println!("Setting BPF filter: {}", filter);
    cap.filter(&filter, true)?;
    
    // Identify local IPs
    let mut local_ips = HashSet::new();
    if let Ok(devs) = Device::list() {
        for d in devs {
            for address in d.addresses {
                local_ips.insert(address.addr.to_string());
            }
        }
    }
    local_ips.insert("127.0.0.1".to_string());
    local_ips.insert("::1".to_string());

    println!("Capturing on device {}", args.device);
    println!("Local IPs: {:?}", local_ips);

    let datalink = cap.get_datalink();

    loop {
        // We need a way to check if channel is closed to stop capture, 
        // but pcap blocks. The timeout(1000) helps us yield.
        if tx.is_closed() {
            return Ok(());
        }

        match cap.next_packet() {
            Ok(packet) => {
                use etherparse::{PacketHeaders, IpHeader, TransportHeader};
                use pcap::Linktype;

                let headers_result = match datalink {
                    Linktype(1) => PacketHeaders::from_ethernet_slice(packet.data),
                    Linktype(113) => {
                         // Linux SLL (Cooked)
                         if packet.data.len() > 16 {
                             PacketHeaders::from_ip_slice(&packet.data[16..])
                         } else {
                             Err(etherparse::ReadError::UnexpectedEndOfSlice(0))
                         }
                    },
                     _ => {
                         PacketHeaders::from_ethernet_slice(packet.data)
                     }
                };

                // Try parsing
                if let Ok(headers) = headers_result {
                    let mut src_ip_bytes: Vec<u8> = Vec::new();
                    let mut dst_ip_bytes: Vec<u8> = Vec::new();

                    let mut src_ip_str = String::new(); // for local checking
                    let mut dst_ip_str = String::new(); // for local checking
                    
                    if let Some(ip) = headers.ip {
                        match ip {
                            IpHeader::Version4(ipv4, _) => {
                                src_ip_bytes = ipv4.source.to_vec();
                                dst_ip_bytes = ipv4.destination.to_vec();
                                src_ip_str = ipv4.source.iter().map(|b| b.to_string()).collect::<Vec<String>>().join(".");
                                dst_ip_str = ipv4.destination.iter().map(|b| b.to_string()).collect::<Vec<String>>().join(".");
                            },
                            IpHeader::Version6(ipv6, _) => {
                                if !args.ipv6 {
                                    continue;
                                }
                                src_ip_bytes = ipv6.source.to_vec();
                                dst_ip_bytes = ipv6.destination.to_vec();
                                
                                use std::net::Ipv6Addr;
                                let s = Ipv6Addr::from(ipv6.source);
                                let d = Ipv6Addr::from(ipv6.destination);
                                src_ip_str = s.to_string();
                                dst_ip_str = d.to_string();
                            } 
                        }
                        
                        // Check if agent
                         let src_is_agent = local_ips.contains(&src_ip_str);
                         let dst_is_agent = local_ips.contains(&dst_ip_str);

                         if !src_is_agent && !dst_is_agent {
                             continue;
                         }

                        let mut src_port = 0;
                        let mut dst_port = 0;
                        let mut proto = packet::Protocol::Unknown;
                        
                        if let Some(transport) = headers.transport {
                            match transport {
                                TransportHeader::Tcp(tcp) => {
                                    src_port = tcp.source_port as i32;
                                    dst_port = tcp.destination_port as i32;
                                    proto = packet::Protocol::Tcp;
                                },
                                TransportHeader::Udp(udp) => {
                                    src_port = udp.source_port as i32;
                                    dst_port = udp.destination_port as i32;
                                    proto = packet::Protocol::Udp;
                                },
                                _ => {
                                    proto = packet::Protocol::Other;
                                }
                            }
                        }

                        let info = Packet {
                            src_ip: src_ip_bytes,
                            dst_ip: dst_ip_bytes,
                            src_is_agent,
                            dst_is_agent,
                            size: packet.header.len as i32,
                            proto: proto.into(),
                            src_port,
                            dst_port,
                        };

                        if let Err(_) = tx.blocking_send(info) {
                            return Ok(()); // channel closed
                        }
                    }
                }
            },
            Err(pcap::Error::TimeoutExpired) => {
                continue;
            },
            Err(e) => {
                eprintln!("Error reading packet: {}", e);
            }
        }
    }
}

async fn generate_mock_traffic(tx: mpsc::Sender<Packet>) {
    // 192.168.1.10, .20, 10.0.0.5, 172.16.0.3
    let peers = vec![
        vec![192, 168, 1, 10], 
        vec![192, 168, 1, 20], 
        vec![10, 0, 0, 5], 
        vec![172, 16, 0, 3]
    ];
    let localhost = vec![127, 0, 0, 1];

    let mut rng = rand::thread_rng();
    use rand::Rng;

    loop {
        let delay = rng.gen_range(100..500);
        sleep(Duration::from_millis(delay)).await;

        if tx.is_closed() { return; }

        let peer = peers[rng.gen_range(0..peers.len())].clone();
        let (src, dst) = if rng.gen_bool(0.5) {
            (localhost.clone(), peer)
        } else {
            (peer, localhost.clone())
        };

        let mut src_is_agent = false;
        let mut dst_is_agent = false;
        if src == localhost { src_is_agent = true; }
        if dst == localhost { dst_is_agent = true; }

        let info = Packet {
            src_ip: src,
            dst_ip: dst,
            src_is_agent,
            dst_is_agent,
            size: rng.gen_range(64..1000),
            proto: packet::Protocol::Tcp.into(),
            src_port: 0,
            dst_port: 0,
        };

        if tx.send(info).await.is_err() {
            return;
        }
    }
}
