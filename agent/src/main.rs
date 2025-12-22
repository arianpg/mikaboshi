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

    #[arg(long, env = "MIKABOSHI_AGENT_BATCH_SIZE", default_value_t = 10000)]
    batch_size: usize,

    #[arg(long, env = "MIKABOSHI_AGENT_BATCH_INTERVAL", default_value_t = 100)]
    batch_interval: u64,
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
struct RawPacket {
    src_ip: IpAddr,
    dst_ip: IpAddr,
    src_is_agent: bool,
    dst_is_agent: bool,
    size: i32,
    proto: i32, // store as i32 to match proto enum value
    src_port: i32,
    dst_port: i32,
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
    // Channel now carries simple batches (Vec<RawPacket>) to reduce lock overhead
    let (tx, rx) = mpsc::channel(args.batch_size); 

    // create a stream of batches
    use tokio_stream::StreamExt;
    let request_stream = tokio_stream::wrappers::ReceiverStream::new(rx)
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
        println!("Starting in MOCK mode (Batch: {} pkts, Interval: {} ms)", args.batch_size, args.batch_interval);
        generate_mock_traffic(tx, args.batch_size).await;
    } else {
        println!("Starting in LIVE capture mode on device {} (Batch: {} pkts, Interval: {} ms)", args.device, args.batch_size, args.batch_interval);
        let tx_clone = tx.clone();
        let args_clone = args.clone();
        
        // pcap capture blocks
        let result = tokio::task::spawn_blocking(move || {
            run_live_capture(args_clone, tx_clone, server_port)
        }).await?;

        if let Err(e) = result {
             eprintln!("Error opening device {}: {}", args.device, e);
             eprintln!("Falling back to MOCK mode due to error.");
             generate_mock_traffic(tx, args.batch_size).await;
        }
    }
    
    // Wait for stream to finish (which means disconnected)
    let _ = stream_handle.await;

    // If we are here, it means connection lost or done
    Err("Connection lost".into())
}

fn compress_packets(packets: Vec<RawPacket>) -> packet::PacketBatch {
    use std::collections::HashMap;

    // Key IS the RawPacket itself since it derives Hash/Eq
    let mut map: HashMap<RawPacket, i32> = HashMap::new();

    for p in packets {
        map.entry(p.clone())
           .and_modify(|size| *size += p.size)
           .or_insert(p.size);
    }

    let packets = map.into_iter().map(|(k, total_size)| {
        let (src_ip_bytes, dst_ip_bytes) = match (k.src_ip, k.dst_ip) {
            (IpAddr::V4(s), IpAddr::V4(d)) => (s.octets().to_vec(), d.octets().to_vec()),
            (IpAddr::V6(s), IpAddr::V6(d)) => (s.octets().to_vec(), d.octets().to_vec()),
            (IpAddr::V4(s), IpAddr::V6(d)) => (s.octets().to_vec(), d.octets().to_vec()), // Should not happen usually but valid
            (IpAddr::V6(s), IpAddr::V4(d)) => (s.octets().to_vec(), d.octets().to_vec()),
        };

        Packet {
            src_ip: src_ip_bytes,
            dst_ip: dst_ip_bytes,
            src_is_agent: k.src_is_agent,
            dst_is_agent: k.dst_is_agent,
            size: total_size,
            proto: k.proto,
            src_port: k.src_port,
            dst_port: k.dst_port,
        }
    }).collect();

    packet::PacketBatch {
        packets
    }
}

fn run_live_capture(args: Args, tx: mpsc::Sender<Vec<RawPacket>>, server_port: u16) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut cap = Capture::from_device(args.device.as_str())?
        .promisc(args.promiscuous)
        .snaplen(args.snapshot)
        .timeout(100) // Lower pcap timeout to allow frequent flush checks
        .open()?;

    // Set BPF filter
    let filter = format!("not port {}", server_port);
    println!("Setting BPF filter: {}", filter);
    cap.filter(&filter, true)?;
    
    // Identify local IPs
    let mut local_ips: HashSet<IpAddr> = HashSet::new();
    if let Ok(devs) = Device::list() {
        for d in devs {
            for address in d.addresses {
                local_ips.insert(address.addr);
            }
        }
    }
    local_ips.insert(IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)));
    local_ips.insert(IpAddr::V6(std::net::Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)));

    println!("Capturing on device {}", args.device);
    println!("Local IPs: {:?}", local_ips);

    let datalink = cap.get_datalink();
    
    // Local buffer for micro-batching
    let mut buffer: Vec<RawPacket> = Vec::with_capacity(args.batch_size);
    let mut last_flush = std::time::Instant::now();
    let flush_interval = std::time::Duration::from_millis(args.batch_interval);

    loop {
        // Check flush timer at start of loop (in case we looped from a packet)
        if !buffer.is_empty() && last_flush.elapsed() >= flush_interval {
             if let Err(_) = tx.blocking_send(buffer) {
                return Ok(());
             }
             buffer = Vec::with_capacity(args.batch_size);
             last_flush = std::time::Instant::now();
        }

        // We need a way to check if channel is closed to stop capture, 
        // but pcap blocks. The timeout(100) helps us yield.
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
                    if let Some(ip) = headers.ip {
                        let (src_ip, dst_ip) = match ip {
                            IpHeader::Version4(ipv4, _) => (
                                IpAddr::from(ipv4.source),
                                IpAddr::from(ipv4.destination)
                            ),
                            IpHeader::Version6(ipv6, _) => {
                                if !args.ipv6 {
                                    continue;
                                }
                                (
                                    IpAddr::from(ipv6.source),
                                    IpAddr::from(ipv6.destination)
                                )
                            } 
                        };
                        
                        let src_is_agent = local_ips.contains(&src_ip);
                        let dst_is_agent = local_ips.contains(&dst_ip);
                        
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

                        let info = RawPacket {
                            src_ip,
                            dst_ip,
                            src_is_agent,
                            dst_is_agent,
                            size: packet.header.len as i32,
                            proto: proto.into(),
                            src_port,
                            dst_port,
                        };

                        buffer.push(info);
                        
                        // Buffer full check
                        if buffer.len() >= args.batch_size {
                            if let Err(_) = tx.blocking_send(buffer) {
                                return Ok(()); // channel closed
                            }
                            buffer = Vec::with_capacity(args.batch_size);
                            last_flush = std::time::Instant::now();
                        }
                    }
                }
            },
            Err(pcap::Error::TimeoutExpired) => {
                // Just continue to loop top to check flush timer
                continue;
            },
            Err(e) => {
                eprintln!("Error reading packet: {}", e);
            }
        }
    }
}

async fn generate_mock_traffic(tx: mpsc::Sender<Vec<RawPacket>>, batch_size: usize) {
    // 192.168.1.10, .20, 10.0.0.5, 172.16.0.3
    let peers = vec![
        IpAddr::V4(std::net::Ipv4Addr::new(192, 168, 1, 10)), 
        IpAddr::V4(std::net::Ipv4Addr::new(192, 168, 1, 20)), 
        IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 0, 5)), 
        IpAddr::V4(std::net::Ipv4Addr::new(172, 16, 0, 3))
    ];
    let localhost = IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1));

    let mut rng = rand::thread_rng();
    use rand::Rng;

    let mut buffer: Vec<RawPacket> = Vec::with_capacity(batch_size);

    loop {
        // High speed mock
        let delay = rng.gen_range(0..5); // Faster!
        if delay > 0 {
             sleep(Duration::from_millis(delay)).await;
        }

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

        let info = RawPacket {
            src_ip: src,
            dst_ip: dst,
            src_is_agent,
            dst_is_agent,
            size: rng.gen_range(64..1000),
            proto: packet::Protocol::Tcp.into(),
            src_port: 0,
            dst_port: 0,
        };
        
        buffer.push(info);
        
        if buffer.len() >= batch_size {
            if tx.send(buffer).await.is_err() {
                return;
            }
            buffer = Vec::with_capacity(batch_size);
        }
    }
}
