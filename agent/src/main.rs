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
    #[arg(long, default_value = "localhost:50051")]
    server: String,

    #[arg(long, default_value = "any")]
    device: String,

    #[arg(long, default_value_t = 1024)]
    snapshot: i32,

    #[arg(long, default_value_t = false)]
    promiscuous: bool,

    #[arg(long, default_value_t = false)]
    mock: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let server_url = if args.server.starts_with("http") {
        args.server.clone()
    } else {
        format!("http://{}", args.server)
    };

    println!("Connecting to {}", server_url);
    let client = AgentServiceClient::connect(server_url).await?;

    // Create a channel for streaming packets
    let (tx, rx) = mpsc::channel(100);
    let request_stream = tokio_stream::wrappers::ReceiverStream::new(rx);

    // Spawn the gRPC client stream handler
    let mut client_clone = client.clone();
    tokio::spawn(async move {
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
        // We clone tx because if live capture fails, we might want to use it for mock (though here we just exit or fallback)
        let tx_clone = tx.clone();
        let args_clone = args.clone();
        
        // pcap capture blocks, so we run it in a blocking thread or just here if we don't need to do other things
        // But since we are in tokio main, we should use spawn_blocking for pcap
        let result = tokio::task::spawn_blocking(move || {
            run_live_capture(args_clone, tx_clone)
        }).await?;

        if let Err(e) = result {
            eprintln!("Error opening device {}: {}", args.device, e);
            eprintln!("Falling back to MOCK mode due to error.");
            generate_mock_traffic(tx).await;
        }
    }

    Ok(())
}

fn run_live_capture(args: Args, tx: mpsc::Sender<Packet>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut cap = Capture::from_device(args.device.as_str())?
        .promisc(args.promiscuous)
        .snaplen(args.snapshot)
        .timeout(1000) // 1s timeout to allow checking for exit or other things
        .open()?;

    // Set BPF filter
    let filter = "not port 50051";
    println!("Setting BPF filter: {}", filter);
    cap.filter(filter, true)?;
    
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
        match cap.next_packet() {
            Ok(packet) => {
                use etherparse::{PacketHeaders, IpHeader, TransportHeader};
                use pcap::Linktype;

                let headers_result = match datalink {
                    Linktype(1) => PacketHeaders::from_ethernet_slice(packet.data),
                    Linktype(113) => {
                         // Linux SLL (Cooked)
                         // Header is 16 bytes. The last 2 bytes are the protocol (EtherType).
                         // If we just skip 16 bytes, we should be at the network layer (IP).
                         if packet.data.len() > 16 {
                             PacketHeaders::from_ip_slice(&packet.data[16..])
                         } else {
                             Err(etherparse::ReadError::UnexpectedEndOfSlice(0)) // Dummy error
                         }
                    },
                     _ => {
                         // Fallback or skip
                         PacketHeaders::from_ethernet_slice(packet.data)
                     }
                };

                // Try parsing
                if let Ok(headers) = headers_result {
                    // ... existing logic ...
                    // ... existing logic ...
                    let mut src_ip = String::new();
                    let mut dst_ip = String::new();
                    let mut proto = String::new();
                    
                    if let Some(ip) = headers.ip {
                        match ip {
                            IpHeader::Version4(ipv4, _) => {
                                src_ip = ipv4.source.iter().map(|b| b.to_string()).collect::<Vec<String>>().join(".");
                                dst_ip = ipv4.destination.iter().map(|b| b.to_string()).collect::<Vec<String>>().join(".");
                            },
                            IpHeader::Version6(ipv6, _) => {
                                use std::net::Ipv6Addr;
                                let s = Ipv6Addr::from(ipv6.source);
                                let d = Ipv6Addr::from(ipv6.destination);
                                src_ip = s.to_string();
                                dst_ip = d.to_string();
                            } 
                        }
                        
                        // Check if agent
                         let src_is_agent = local_ips.contains(&src_ip);
                         let dst_is_agent = local_ips.contains(&dst_ip);

                         if !src_is_agent && !dst_is_agent {
                             continue;
                         }

                        let mut src_port = 0;
                        let mut dst_port = 0;
                        
                        if let Some(transport) = headers.transport {
                            match transport {
                                TransportHeader::Tcp(tcp) => {
                                    src_port = tcp.source_port as i32;
                                    dst_port = tcp.destination_port as i32;
                                    proto = "TCP".to_string();
                                },
                                TransportHeader::Udp(udp) => {
                                    src_port = udp.source_port as i32;
                                    dst_port = udp.destination_port as i32;
                                    proto = "UDP".to_string();
                                },
                                _ => {}
                            }
                        }

                        let info = Packet {
                            r#type: "traffic".to_string(),
                            src_ip,
                            dst_ip,
                            src_is_agent,
                            dst_is_agent,
                            size: packet.header.len as i32,
                            proto,
                            src_port,
                            dst_port,
                        };

                        if let Err(_) = tx.blocking_send(info) {
                            return Ok(()); // channel closed
                        }
                    } else {
                        // Log only periodically to avoid spam?
                        // println!("No IP header found in packet of len {}", packet.data.len());
                    }
                } else {
                     // println!("Failed to parse ethernet packet");
                }
            },
            Err(pcap::Error::TimeoutExpired) => {
                // minimize cpu usage or check for exit 
                continue;
            },
            Err(e) => {
                eprintln!("Error reading packet: {}", e);
                // Depending on error, we might want to break
                // e.g. device went down
            }
        }
    }
}

async fn generate_mock_traffic(tx: mpsc::Sender<Packet>) {
    let peers = vec!["192.168.1.10", "192.168.1.20", "10.0.0.5", "172.16.0.3"];
    let mut rng = rand::thread_rng();
    use rand::Rng;

    loop {
        let delay = rng.gen_range(100..500);
        sleep(Duration::from_millis(delay)).await;

        let peer = peers[rng.gen_range(0..peers.len())];
        let (src, dst) = if rng.gen_bool(0.5) {
            ("127.0.0.1".to_string(), peer.to_string())
        } else {
            (peer.to_string(), "127.0.0.1".to_string())
        };

        let info = Packet {
            r#type: "traffic".to_string(),
            src_ip: src,
            dst_ip: dst,
            src_is_agent: false, // In mock, logic in Go didn't strictly set this but it defaults to false/true based on logic.
            // Go mock: srcIsAgent/dstIsAgent were implied by '127.0.0.1' being in localIPs.
            // Here we should probably mimic that.
            dst_is_agent: false,
            size: rng.gen_range(64..1000),
            proto: "TCP".to_string(),
            src_port: 0,
            dst_port: 0,
        };
        // Refine is_agent logic for mock
        let mut info = info;
        if info.src_ip == "127.0.0.1" { info.src_is_agent = true; }
        if info.dst_ip == "127.0.0.1" { info.dst_is_agent = true; }

        if tx.send(info).await.is_err() {
            return;
        }
    }
}
