use axum::Router;
use futures::stream::StreamExt;
use std::net::SocketAddr;

use tokio::sync::broadcast;
use tonic::{transport::Server, Request, Response, Status};
use tower_http::services::ServeDir;
use tower_http::cors::{CorsLayer, Any};

pub mod packet {
    tonic::include_proto!("packet");
}

use packet::agent_service_server::{AgentService, AgentServiceServer};
use packet::{Empty, Packet};

// Shared state
struct AppState {
    tx: broadcast::Sender<Packet>,
}

#[derive(Default)]
struct GrpcService {
    tx: Option<broadcast::Sender<Packet>>,
}

#[tonic::async_trait]
impl AgentService for GrpcService {
    async fn stream_packets(
        &self,
        request: Request<tonic::Streaming<Packet>>,
    ) -> Result<Response<Empty>, Status> {
        let mut stream = request.into_inner();
        let tx = self.tx.clone().ok_or(Status::internal("Internal error"))?;

        while let Some(result) = stream.next().await {
            match result {
                Ok(packet) => {
                     // Broadcast packet to all subscribers (gRPC-Web clients)
                     let _ = tx.send(packet);
                }
                Err(e) => return Err(e),
            }
        }

        Ok(Response::new(Empty {}))
    }

    type SubscribeStream = tokio_stream::wrappers::ReceiverStream<Result<Packet, Status>>;

    async fn subscribe(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<Self::SubscribeStream>, Status> {
        let tx = self.tx.clone().ok_or(Status::internal("Internal error"))?;
        let mut rx = tx.subscribe();

        // Create a channel for this specific client stream
        let (client_tx, client_rx) = tokio::sync::mpsc::channel(100);

        tokio::spawn(async move {
            while let Ok(packet) = rx.recv().await {
                if client_tx.send(Ok(packet)).await.is_err() {
                    break;
                }
            }
        });

        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(client_rx)))
    }
}


use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Port for the gRPC server (including gRPC-Web)
    #[arg(long, default_value_t = 50051)]
    grpc_port: u16,

    /// Port for the HTTP server (static files)
    #[arg(long, default_value_t = 8080)]
    http_port: u16,

    /// Capacity of the broadcast channel (buffer size)
    #[arg(long, default_value_t = 4096)]
    channel_capacity: usize,

    /// Timeout for peer inactivity (seconds)
    #[arg(long, default_value_t = 30)]
    peer_timeout: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    // Channel for broadcasting packets
    let (tx, _rx) = broadcast::channel(args.channel_capacity);

    // --- gRPC Server (including gRPC-Web) ---
    let grpc_addr = SocketAddr::from(([0, 0, 0, 0], args.grpc_port));
    let grpc_service = GrpcService { tx: Some(tx.clone()) }; 
    
    // Enable gRPC-Web and CORS
    let service = AgentServiceServer::new(grpc_service);
    let service = tonic_web::enable(service);

    println!("gRPC (Native + Web) server listening on {}", grpc_addr);
    
    // Spawn gRPC server
    tokio::spawn(async move {
        Server::builder()
            .accept_http1(true) // Required for gRPC-Web
            .layer(CorsLayer::new()
                .allow_origin(Any)
                .allow_headers(Any)
                .allow_methods(Any)
            )
            .add_service(service)
            .serve(grpc_addr)
            .await
            .unwrap();
    });

    // --- HTTP Server (Static Files) ---
    // Serve static files from web/dist
    let app = Router::new()
        .route("/config", axum::routing::get(move || async move {
            axum::Json(serde_json::json!({
                "grpcPort": args.grpc_port,
                "peerTimeout": args.peer_timeout * 1000 // Convert to ms
            }))
        }))
        .nest_service("/", ServeDir::new("web/dist"));

    let http_addr = SocketAddr::from(([0, 0, 0, 0], args.http_port));
    println!("HTTP server listening on {}", http_addr);
    
    let listener = tokio::net::TcpListener::bind(http_addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();

    Ok(())
}
